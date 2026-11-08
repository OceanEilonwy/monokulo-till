package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * Settings › Wake-ups (`docs/design.md` §3): what makes the till check
 * its point of sale, as the cashier chose it (the acceptance criteria's "mode").
 */
@Serializable
sealed interface WakeUpChoice {
    /** The default: the till checks its point of sale every 2 seconds during a shift. */
    @Serializable
    @SerialName("timer")
    data object TimerOnly : WakeUpChoice

    /** The project's Bell, `BuildConfig.BELL_URL`. */
    @Serializable
    @SerialName("monokulo")
    data object MonokuloBell : WakeUpChoice

    /** A Bell the merchant runs. */
    @Serializable
    @SerialName("own")
    data class OwnBell(val bell: BellAddress) : WakeUpChoice

    /** The Bell this choice names, given the project's; null for the timer alone. */
    fun bell(monokulo: BellAddress): BellAddress? = when (this) {
        TimerOnly -> null
        MonokuloBell -> monokulo
        is OwnBell -> bell
    }
}

/**
 * What a Bell says about itself: `GET /v1/info` (`docs/bell.md`), as [BellApi] reads it.
 * One Bell may ring several environments of each point of sale at once (Square's production
 * and its sandbox, say), each through its own application (decision 305).
 */
data class BellInfo(
    /**
     * The points of sale this Bell rings tills for, by wire name ([PointOfSale.wire]), each
     * mapping its environments to the application the till must sign in through for this Bell
     * to hear its sales there. Square's environments are `production` and `sandbox`; Clover's
     * `north_america`, `europe`, `latin_america` and `sandbox`. A point of sale the Bell doesn't
     * run is absent; a name this build doesn't know, of either, is kept and never read.
     */
    val pointsOfSale: Map<String, Map<String, String>>,
    /** The key every ding is signed with: what the subscription is made for. */
    val vapidPublicKey: String,
) {
    /** The Bell's environments for [pointOfSale], each to its application; null when it doesn't ring that point of sale's tills. */
    operator fun get(pointOfSale: PointOfSale): Map<String, String>? = pointsOfSale[pointOfSale.wire]
}

/** A registration the Bell holds for this till: `POST /v1/register`'s answer and what it was made with. */
@Serializable
data class BellRegistration(
    val bell: BellAddress,
    /** `br_…`: known only to this till, and all `DELETE /v1/register/{id}` needs. */
    @SerialName("registration_id") val registrationId: String,
    /** The Bell's key the subscription was made for: another one means subscribing again (decision 95). */
    @SerialName("vapid_public_key") val vapidPublicKey: String,
    /** The subscription registered, to tell when the distributor has changed it. */
    val subscription: PushSubscription,
    /**
     * The distributor's package the subscription was made through: at a start, before
     * this process has subscribed, the one an uninstall took away (decision 122).
     */
    val distributor: String,
    /**
     * The point of sale ([PointOfSale.wire]) and environment it was made for: a till whose
     * sign-in moved to another registers again (decision 306). Null in a registration kept
     * from before a Bell rang several environments, which counts as another.
     */
    @SerialName("point_of_sale") val pointOfSale: String? = null,
    val environment: String? = null,
) {
    /** Made for [signIn]'s point of sale and environment. */
    fun isFor(signIn: PosSignIn): Boolean = pointOfSale == signIn.pointOfSale.wire && environment == signIn.environment
}

/**
 * What the app knows about the Bell it chose (the acceptance criteria's
 * "Bell status"): one value at a time, for the shift's pace and the Settings
 * card. Times are Unix seconds.
 */
sealed interface WakeUpStatus {
    /** Timer only: no Bell, no status card. */
    data object Off : WakeUpStatus

    /** A Bell is chosen, but the till isn't signed in to a point of sale, which registering needs. */
    data object NoPos : WakeUpStatus

    /** A Bell is chosen, but no UnifiedPush distributor is installed to carry its dings. */
    data object NoDistributor : WakeUpStatus

    /** Several distributors are installed and none is the default: the cashier picks one ([packages]: their package names). */
    data class ChooseDistributor(val packages: List<String>) : WakeUpStatus

    /** Reading the Bell, subscribing with the distributor, or registering. */
    data object Connecting : WakeUpStatus

    /** Registered: dings reach the till, so the timer runs at 30 s. [lastDingAt] is the last ding's time, null for none yet. */
    data class Connected(val lastDingAt: Long?) : WakeUpStatus

    /**
     * The timer found a sale the Bell hadn't rung for (decision 94): the
     * timer is back at 2 s and the till has registered again; the next
     * ding makes it [Connected].
     */
    data class MissingWakeups(val lastDingAt: Long?) : WakeUpStatus

    /**
     * The Bell couldn't be reached, or isn't one: [reason] is short, in
     * the cashier's words ("no connection", "it didn't answer", "it
     * answered 503", "it isn't a Bell", "the UnifiedPush app refused").
     * Retried by itself, with a growing wait.
     */
    data class Unreachable(val reason: String) : WakeUpStatus

    /**
     * The Bell answered with a problem: [reason] in the cashier's words
     * (`docs/bell.md`'s types, decision 95). [retryAt] is when it tries
     * again by itself (`rate-limited`); null when only a re-pick, a
     * sign-in, an app start or a shift start will.
     */
    data class Refused(val reason: String, val retryAt: Long? = null) : WakeUpStatus

    /**
     * The Bell hears [pointOfSale] through another application, [applicationId]:
     * the till must sign in to it through that one (decision 96, 132).
     */
    data class SignInNeeded(val pointOfSale: PointOfSale, val applicationId: String) : WakeUpStatus
}

/** How often the shift's timer checks the point of sale: 30 s only while [WakeUpStatus.Connected], else 2 s (`docs/design.md` §3, §7). */
val WakeUpStatus.interval: Duration
    get() = if (this is WakeUpStatus.Connected) 30.seconds else 2.seconds

/** What Wake-ups keeps between runs, encrypted with the rest of the till's state. */
interface WakeUpStore {
    var choice: WakeUpChoice

    /** The last "My own Bell" address entered, kept through Timer only so the row still shows it (decision 97). */
    var ownBell: BellAddress?

    /** The registration the Bell holds, while it does. */
    var registration: BellRegistration?
}

/**
 * Wake-ups, the Bell side (`docs/design.md` §3): keeps the till registered
 * with the chosen Bell while there is one, and takes the registration back
 * when there isn't. The service paces its timer by [status]; Settings
 * shows it. Implemented over [PushSubscriptions] and the Bell's API; the
 * app wires it in its graph.
 */
interface WakeUps {
    val choice: StateFlow<WakeUpChoice>

    val status: StateFlow<WakeUpStatus>

    /**
     * The cashier chose (Settings): saved at once, then the old
     * registration is taken back and the new Bell registered with, as
     * decision 95 orders it. Choosing what is already chosen does nothing,
     * unless the status is [WakeUpStatus.Refused] or
     * [WakeUpStatus.Unreachable], which tries again.
     */
    fun choose(choice: WakeUpChoice)

    /** The cashier picked a distributor from [WakeUpStatus.ChooseDistributor]: saved, then registration goes on. */
    fun chooseDistributor(packageName: String)

    /**
     * Register again (`POST /v1/register` with the current subscription;
     * the Bell keeps the id): an app start, a shift starting, a new
     * point-of-sale sign-in, a changed location, or a status to retry. Nothing happens
     * for [WakeUpChoice.TimerOnly] or while a registration is under way.
     */
    fun recheck()

    /**
     * A check the shift's *timer* started (not a ding, not the app opening)
     * found [sales] new sales at the point of sale. With a Bell connected and no ding in
     * the 60 s before, that is a wake-up the Bell missed (decision 94):
     * the status becomes [WakeUpStatus.MissingWakeups] and the till
     * registers once again.
     */
    fun timerFound(sales: Int)

    /**
     * Take the registration back (`DELETE`, awaited, 10 s at most) and end
     * the subscription: the cashier signed out of the point of sale or changed
     * it, or the device was disconnected. The choice stays (status [WakeUpStatus.NoPos])
     * unless [forget], which puts it back to Timer only.
     */
    suspend fun unregister(forget: Boolean = false)
}
