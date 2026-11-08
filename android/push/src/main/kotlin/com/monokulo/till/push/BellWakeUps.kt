package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.io.IOException
import java.io.InterruptedIOException
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

/** Which UnifiedPush distributor would carry the dings (decision 91). */
sealed interface Distributor {
    data class Found(val packageName: String) : Distributor

    data object None : Distributor

    /** Several installed and no default: the cashier picks one of [packages]. */
    data class Choose(val packages: List<String>) : Distributor
}

/** The distributors on the device, as [BellWakeUps] needs them; [UnifiedPushSubscriptions] over the connector. */
interface Distributors {
    /** The saved distributor while it is installed, else the default (then saved), else what there is to choose from. */
    fun resolve(): Distributor

    /** The cashier's pick from [Distributor.Choose]. */
    fun save(packageName: String)

    /** An app came or went, which may have been a distributor. */
    val changes: SharedFlow<Unit>
}

/** The till's point-of-sale sign-in, as registering with a Bell needs it (decision 132). */
interface PosSide {
    /** Where the till is signed in; null while it isn't signed in to a point of sale (or runs without one). */
    val signedIn: PosSignIn?

    /** An access token for [BellClient.register], or null when the point of sale wants the cashier to sign in again. */
    suspend fun accessToken(): String?
}

/**
 * The till's sign-in at its point of sale: the application and environment it
 * signed in through, and [locationId], the Square location it watches or the
 * Clover merchant it is at (a Clover merchant is its own location, acceptance D1).
 * [environment] is spelt as a Bell's `/v1/info` spells it.
 */
data class PosSignIn(
    val pointOfSale: PointOfSale,
    val applicationId: String,
    val environment: String,
    val locationId: String,
)

/**
 * Wake-ups over a Bell (decisions 91, 94, 95, 97): keeps the till
 * registered with the Bell [WakeUpStore.choice] names, and nothing
 * registered when it names none. One pass at a time brings what the Bell
 * holds into line with the choice; [status] says how that went, and the
 * shift paces its timer by it. Everything Android sits behind the
 * interfaces it takes, so it runs on the JVM in tests, on [scope]'s clock.
 */
class BellWakeUps(
    private val push: PushSubscriptions,
    private val distributors: Distributors,
    private val bells: BellClient,
    private val store: WakeUpStore,
    /** The project's Bell: what [WakeUpChoice.MonokuloBell] means. */
    private val monokulo: BellAddress,
    private val pos: PosSide,
    private val scope: CoroutineScope,
    /** Unix seconds. */
    private val now: () -> Long,
) : WakeUps {
    private val mutableChoice = MutableStateFlow(store.choice)
    override val choice: StateFlow<WakeUpChoice> = mutableChoice.asStateFlow()

    private val mutableStatus = MutableStateFlow(if (store.choice == WakeUpChoice.TimerOnly) WakeUpStatus.Off else WakeUpStatus.Connecting)
    override val status: StateFlow<WakeUpStatus> = mutableStatus.asStateFlow()

    /** Held for each pass and each unregistering: what the Bell holds changes under it only. */
    private val lock = Mutex()
    @Volatile private var pass: Job? = null
    private var retry: Job? = null
    private var waits = 0

    /** The key and the distributor this process subscribed with; null until it has (the connector is asked again at every start). */
    private var subscribedFor: String? = null
    private var subscribedThrough: String? = null

    /** The distributor a change of apps took away, until its [PushFailure.Lost] arrives (decision 118). */
    @Volatile private var departed: String? = null

    /**
     * The subscription the last pass took from the distributor, whether or not the Bell then
     * took it: an endpoint arriving outside a pass is news only when it isn't this one (decision 123).
     */
    @Volatile private var taken: PushSubscription? = null

    @Volatile private var lastDingAt: Long? = null
    @Volatile private var connectedAt: Long? = null

    init {
        scope.launch { push.dings.collect { ding() } }
        scope.launch { push.state.collect(::pushChanged) }
        scope.launch { distributors.changes.collect { distributorsChanged() } }
        // An app start: register again, or finish taking a registration back.
        if (store.choice != WakeUpChoice.TimerOnly || store.registration != null) run(restart = false)
    }

    override fun choose(choice: WakeUpChoice) {
        val current = store.choice
        val retrying = mutableStatus.value is WakeUpStatus.Refused || mutableStatus.value is WakeUpStatus.Unreachable
        if (choice == current && !retrying) return
        store.choice = choice
        if (choice is WakeUpChoice.OwnBell) store.ownBell = choice.bell
        mutableChoice.value = choice
        mutableStatus.value = if (choice == WakeUpChoice.TimerOnly) WakeUpStatus.Off else WakeUpStatus.Connecting
        waits = 0
        run(restart = true)
    }

    override fun chooseDistributor(packageName: String) {
        distributors.save(packageName)
        if (store.choice == WakeUpChoice.TimerOnly) return
        mutableStatus.value = WakeUpStatus.Connecting
        run(restart = true)
    }

    override fun recheck() {
        if (store.choice == WakeUpChoice.TimerOnly) return
        run(restart = false)
    }

    override fun timerFound(sales: Int) {
        if (sales <= 0) return
        val connected = mutableStatus.value as? WakeUpStatus.Connected ?: return
        // A sale the Bell rang for, or one from before the till was connected, isn't a missed wake-up.
        val heard = maxOf(lastDingAt ?: Long.MIN_VALUE, connectedAt ?: Long.MIN_VALUE)
        if (now() - heard <= MISSED_AFTER.inWholeSeconds) return
        mutableStatus.value = WakeUpStatus.MissingWakeups(connected.lastDingAt)
        run(restart = false)
    }

    override suspend fun unregister(forget: Boolean) {
        synchronized(this) { retry?.cancel() }
        pass?.cancelAndJoin()
        departed = null
        lock.withLock {
            store.registration?.let { takeBack(it) }
            unsubscribe()
            if (forget) store.choice = WakeUpChoice.TimerOnly
            mutableChoice.value = store.choice
            mutableStatus.value = if (store.choice == WakeUpChoice.TimerOnly) WakeUpStatus.Off else WakeUpStatus.NoPos
        }
    }

    /** Start a pass now: [restart] stops one under way (the choice changed); otherwise one under way is enough. */
    private fun run(restart: Boolean) = synchronized(this) {
        retry?.cancel()
        val running = pass
        if (running?.isActive == true && !restart) return@synchronized
        running?.cancel()
        pass = scope.launch {
            running?.join()
            lock.withLock { reconcile() }
        }
    }

    /** A pass: what the Bell holds brought into line with the choice, ending in a [status] (decision 95's order). */
    private suspend fun reconcile() {
        val choice = store.choice
        val bell = choice.bell(monokulo)
        store.registration?.takeIf { it.bell != bell }?.let { takeBack(it) }
        if (bell == null) {
            unsubscribe()
            settle(WakeUpStatus.Off)
            return
        }
        val before = mutableStatus.value
        if (before !is WakeUpStatus.Connected && before !is WakeUpStatus.MissingWakeups) mutableStatus.value = WakeUpStatus.Connecting
        // Read once: a pass registers for the one sign-in it started with.
        val signedIn = try {
            pos.signedIn
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            settle(WakeUpStatus.Unreachable(TILL_FAILED))
            return
        }
        val outcome = try {
            register(bell, signedIn)
        } catch (e: CancellationException) {
            throw e
        } catch (e: BellException) {
            problemStatus(e, signedIn?.pointOfSale, now())
        } catch (e: IOException) {
            WakeUpStatus.Unreachable(unreachableReason(e))
        } catch (_: Exception) {
            // The store, the Keystore, the connector or the point of sale failed on this device: a pass
            // never takes the till down mid-shift, and tries again as an unreachable Bell does (decision 120).
            WakeUpStatus.Unreachable(TILL_FAILED)
        }
        settle(outcome)
    }

    /**
     * Register with [bell] for [signedIn], stopping at the first thing in the way; the status
     * that leaves. The Bell's entry for the till's point of sale is checked in acceptance
     * T5.2's order: there at all, then the till's environment among its environments, then
     * that environment's application (decisions 132, 305).
     */
    private suspend fun register(bell: BellAddress, signedIn: PosSignIn?): WakeUpStatus {
        val info = bells.info(bell)
        if (signedIn == null) {
            release()
            return WakeUpStatus.NoPos
        }
        val pointOfSale = signedIn.pointOfSale
        val environments = info[pointOfSale] ?: run {
            release()
            return WakeUpStatus.Refused(PosWords.notRung(pointOfSale))
        }
        // Environments other than the till's, ones this build doesn't know included, are never read.
        val applicationId = environments[signedIn.environment] ?: run {
            release()
            return WakeUpStatus.Refused(PosWords.notRung(pointOfSale, signedIn.environment))
        }
        if (applicationId != signedIn.applicationId) {
            release()
            return WakeUpStatus.SignInNeeded(pointOfSale, applicationId)
        }
        val through = when (val distributor = distributors.resolve()) {
            Distributor.None -> {
                release()
                unsubscribe()
                return WakeUpStatus.NoDistributor
            }
            is Distributor.Choose -> {
                release()
                return WakeUpStatus.ChooseDistributor(distributor.packages)
            }
            is Distributor.Found -> distributor.packageName
        }
        // Another distributor than the one subscribed through: that one went, and its end is on its way (decision 122).
        val before = subscribedThrough ?: store.registration?.distributor
        if (before != null && before != through) departed = before
        val subscription = subscription(info.vapidPublicKey, through).let { state ->
            when (state) {
                is PushState.Subscribed -> state.subscription
                is PushState.Failed -> return pushFailedStatus(state.reason)
                else -> return WakeUpStatus.Unreachable(PUSH_SILENT)
            }
        }
        // The Bell keys a registration by its endpoint: a new one is a new registration (decision 95). So is one
        // made for another point of sale or environment, or before registrations named theirs (decision 306).
        store.registration?.takeIf { it.subscription.endpoint != subscription.endpoint || !it.isFor(signedIn) }?.let { takeBack(it) }
        val token = pos.accessToken() ?: run {
            release()
            return WakeUpStatus.NoPos
        }
        // Once the Bell has the request, its answer is kept whatever happens to the pass.
        withContext(NonCancellable) {
            bells.register(bell, pointOfSale, signedIn.environment, token, signedIn.locationId, subscription).let { answer ->
                store.registration = BellRegistration(
                    bell = bell,
                    registrationId = answer.registrationId,
                    vapidPublicKey = info.vapidPublicKey,
                    subscription = subscription,
                    distributor = through,
                    pointOfSale = pointOfSale.wire,
                    environment = signedIn.environment,
                )
            }
        }
        val current = mutableStatus.value
        return if (current is WakeUpStatus.MissingWakeups) current else WakeUpStatus.Connected(lastDingAt)
    }

    /** The subscription for [vapidPublicKey] through [through]: the one held, or a new one the distributor gives. */
    private suspend fun subscription(vapidPublicKey: String, through: String): PushState {
        val held = push.state.value
        val state = if (held is PushState.Subscribed && subscribedFor == vapidPublicKey && subscribedThrough == through) {
            held
        } else {
            // A rotated key, or another distributor: the old subscription ends first (D16).
            val previousKey = subscribedFor ?: store.registration?.vapidPublicKey
            if (previousKey != null && previousKey != vapidPublicKey) push.unsubscribe()
            subscribedFor = vapidPublicKey
            subscribedThrough = through
            push.subscribe(vapidPublicKey)
            withTimeoutOrNull(SUBSCRIBE_WAIT) { push.state.first { it !is PushState.Subscribing } } ?: PushState.Subscribing
        }
        taken = (state as? PushState.Subscribed)?.subscription
        return state
    }

    /** Take a registration back: `DELETE`, tried once within the time it gets, and forgotten here whatever the answer (decision 97). */
    private suspend fun takeBack(registration: BellRegistration) {
        withContext(NonCancellable) {
            withTimeoutOrNull(UNREGISTER_WAIT) {
                try {
                    bells.unregister(registration.bell, registration.registrationId)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // The Bell drops a dead endpoint by itself (its failure limit).
                }
            }
            store.registration = null
            // Its dings went with it: the next registration's Last wake-up starts at None yet.
            lastDingAt = null
        }
    }

    /** No registration while the till can't use it: the Bell would ring for nothing. */
    private suspend fun release() {
        store.registration?.let { takeBack(it) }
    }

    /**
     * End the distributor's subscription, one an earlier run of the app made included. The
     * app ending it is owed no [PushFailure.Lost], so no departure waits for one (decision 118).
     */
    private fun unsubscribe() {
        subscribedFor = null
        subscribedThrough = null
        departed = null
        taken = null
        push.unsubscribe()
    }

    /** The pass is over: [outcome] shown, and a retry set for the statuses that retry by themselves (D5). */
    private fun settle(outcome: WakeUpStatus) {
        if (outcome is WakeUpStatus.Connected && mutableStatus.value !is WakeUpStatus.Connected) connectedAt = now()
        mutableStatus.value = outcome
        val wait: Duration = when {
            outcome is WakeUpStatus.Unreachable -> backoff()
            outcome is WakeUpStatus.Refused && outcome.retryAt != null -> maxOf(0, outcome.retryAt - now()).seconds
            else -> {
                waits = 0
                return
            }
        }
        synchronized(this) {
            retry = scope.launch {
                delay(wait)
                run(restart = false)
            }
        }
    }

    /** 1 minute, doubling to 15 (D5). */
    private fun backoff(): Duration {
        val wait = minOf(FIRST_RETRY * (1 shl minOf(waits, 4)), LAST_RETRY)
        waits++
        return wait
    }

    private fun ding() {
        val at = now()
        lastDingAt = at
        mutableStatus.update { status ->
            when (status) {
                is WakeUpStatus.Connected, is WakeUpStatus.MissingWakeups -> WakeUpStatus.Connected(at)
                else -> status
            }
        }
    }

    /**
     * The distributor changed things outside a pass: a new endpoint, one it gives after the
     * pass stopped waiting for it (decision 123), or the subscription ended. A subscription
     * the last pass already took is nothing new, whatever the Bell made of it.
     */
    private fun pushChanged(state: PushState) {
        if (store.choice == WakeUpChoice.TimerOnly) return
        val held = store.registration
        when {
            state is PushState.Subscribed && state.subscription != held?.subscription && state.subscription != taken -> run(restart = false)
            state is PushState.Failed && state.reason == PushFailure.Lost && held != null -> scope.launch { distributorGone() }
        }
    }

    /**
     * The subscription ended: the Bell's registration goes with it (decision 91). The distributor
     * ending it itself is [WakeUpStatus.NoDistributor]; one uninstalled with another left (the
     * connector reports that as an end too) is a pass through what is installed (decision 118).
     */
    private suspend fun distributorGone() {
        val gone = departed ?: subscribedThrough
        departed = null
        pass?.cancelAndJoin()
        lock.withLock {
            try {
                store.registration?.let { takeBack(it) }
                subscribedFor = null
                subscribedThrough = null
                taken = null
                if (store.choice == WakeUpChoice.TimerOnly) return
                when (val distributor = distributors.resolve()) {
                    Distributor.None -> settle(WakeUpStatus.NoDistributor)
                    is Distributor.Found ->
                        if (distributor.packageName == gone) settle(WakeUpStatus.NoDistributor) else run(restart = false)
                    is Distributor.Choose -> run(restart = false)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                settle(WakeUpStatus.Unreachable(TILL_FAILED))
            }
        }
    }

    /** An app came or went: look again when waiting for a distributor, or when the one in use may be gone. */
    private fun distributorsChanged() {
        if (store.choice == WakeUpChoice.TimerOnly) return
        val waiting = mutableStatus.value is WakeUpStatus.NoDistributor || mutableStatus.value is WakeUpStatus.ChooseDistributor
        val through = subscribedThrough ?: store.registration?.distributor
        // A failure to look is a look at the next pass, not the end of this collector (decision 120).
        val moved = through != null && runCatching { distributors.resolve() }.getOrNull() != Distributor.Found(through)
        if (moved) departed = through
        if (waiting || moved) run(restart = false)
    }

    companion object {
        /** A timer-found sale with no ding this long before it is a missed wake-up (decision 94). */
        val MISSED_AFTER = 60.seconds
        val FIRST_RETRY = 1.minutes
        val LAST_RETRY = 15.minutes
        val UNREGISTER_WAIT = 10.seconds

        /** How long the distributor gets to answer with an endpoint (it may have to reach its server). */
        val SUBSCRIBE_WAIT = 30.seconds

        const val PUSH_REFUSED = "the UnifiedPush app refused"
        const val PUSH_NO_KEYS = "the UnifiedPush app gave no keys"
        /** A distributor just installed answers nothing until it has been opened once (decision 123). */
        const val PUSH_SILENT = "the UnifiedPush app didn't answer: open it once"
        const val PUSH_OFFLINE = "the UnifiedPush app has no connection"
        const val TILL_FAILED = "something on this device failed"

        /** Why the Bell couldn't be reached, in the cashier's words (A1.7). */
        fun unreachableReason(e: IOException): String = when (e) {
            is NotABellException -> "it isn't a Bell"
            is BellException -> "it answered ${e.status}"
            is InterruptedIOException -> "it didn't answer"
            else -> "no connection"
        }

        /**
         * What a Bell's problem means for the till (decision 95, A1.7), signed in to
         * [pointOfSale] (null before a sign-in): the Bell names its slugs by point of sale,
         * `square-refused`, `clover-unavailable` (B46). [now] is Unix seconds.
         */
        fun problemStatus(e: BellException, pointOfSale: PointOfSale?, now: Long): WakeUpStatus {
            val refused = pointOfSale?.let { "${it.wire}-refused" }
            val unavailable = pointOfSale?.let { "${it.wire}-unavailable" }
            return when (e.type) {
                null -> statusOnly(e)
                refused -> WakeUpStatus.Refused(PosWords.refused(requireNotNull(pointOfSale)))
                "too-many-registrations" -> WakeUpStatus.Refused("this ${PosWords.place(pointOfSale)} already has 32 tills registered")
                "endpoint-registered" -> WakeUpStatus.Refused("this device is registered under another ${PosWords.account(pointOfSale)}")
                "rate-limited" -> {
                    val wait = (e.retryAfter ?: FIRST_RETRY.inWholeSeconds).coerceAtLeast(1)
                    WakeUpStatus.Refused("too many registrations from this network: trying again in ${about(wait)}", retryAt = now + wait)
                }
                unavailable, "internal" -> WakeUpStatus.Unreachable(unreachableReason(e))
                else -> statusOnly(e)
            }
        }

        private fun statusOnly(e: BellException): WakeUpStatus =
            if (e.status >= 500) WakeUpStatus.Unreachable(unreachableReason(e)) else WakeUpStatus.Refused(e.detail ?: "it answered ${e.status}")

        /**
         * A wait of [seconds] in words loose enough to stay true while the footnote
         * shows them, since it doesn't count down (decision 124).
         */
        fun about(seconds: Long): String = when {
            seconds <= 90 -> "about a minute"
            else -> "about ${(seconds + 30) / 60} minutes"
        }

        /** The distributor couldn't make the subscription (decision 95). */
        fun pushFailedStatus(reason: PushFailure): WakeUpStatus = when (reason) {
            PushFailure.NoDistributor, PushFailure.Lost -> WakeUpStatus.NoDistributor
            PushFailure.Network -> WakeUpStatus.Unreachable(PUSH_OFFLINE)
            PushFailure.NoKeys -> WakeUpStatus.Unreachable(PUSH_NO_KEYS)
            PushFailure.ActionRequired, PushFailure.Other -> WakeUpStatus.Unreachable(PUSH_REFUSED)
        }
    }
}
