package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

/** A till signed in to one point of sale, as [WakeUpsScenarios] runs for it. */
data class TestTill(
    val pointOfSale: PointOfSale,
    val applicationId: String,
    val environment: String,
    /** Another environment of the same point of sale. */
    val otherEnvironment: String,
    /** Another application of the same point of sale and environment: a Bell's. */
    val otherApplicationId: String,
    val locationId: String,
    val token: String,
    /** Why a Bell without the till's environment refuses it, in the cashier's words. */
    val notRungHere: String,
)

/**
 * Wake-ups' registration lifecycle against a recording Bell, distributor and
 * point of sale (A13): every transition of A3, A6, A7, A10 and A11, on virtual
 * time, for a Square till ([BellWakeUpsTest]) and a Clover one
 * ([CloverBellWakeUpsTest]), acceptance T5.4.
 */
@OptIn(ExperimentalCoroutinesApi::class)
abstract class WakeUpsScenarios(private val till: TestTill) {
    private val monokulo = BellAddress.parse("bell.monokulo.com")!!
    private val own = BellAddress.parse("bell.thebeanroom.coffee")!!

    private class FakeBell : BellClient {
        val calls = mutableListOf<String>()
        /** The point of sale each POST registered for. */
        val registeredFor = mutableListOf<PointOfSale>()
        /** The environment each POST registered in. */
        val registeredIn = mutableListOf<String>()
        val infos = mutableMapOf<BellAddress, () -> BellInfo>()
        var register: () -> BellRegistered = { BellRegistered("br_${++ids}") }
        var unregister: suspend () -> Unit = {}
        var ids = 0

        override suspend fun info(bell: BellAddress): BellInfo {
            calls += "GET ${bell.display}"
            return (infos[bell] ?: { throw ConnectException() })()
        }

        override suspend fun register(
            bell: BellAddress,
            pointOfSale: PointOfSale,
            environment: String,
            accessToken: String,
            locationId: String,
            subscription: PushSubscription,
        ): BellRegistered {
            calls += "POST ${bell.display} $accessToken $locationId ${subscription.endpoint}"
            registeredFor += pointOfSale
            registeredIn += environment
            return register()
        }

        override suspend fun unregister(bell: BellAddress, registrationId: String) {
            calls += "DELETE ${bell.display} $registrationId"
            unregister()
        }
    }

    private class FakePush : PushSubscriptions {
        val calls = mutableListOf<String>()
        var endpoint = "https://ntfy.sh/up1"
        /** What subscribing comes to; null leaves it waiting. */
        var answer: (String) -> PushState? = { PushState.Subscribed(PushSubscription(endpoint, "BKey", "auth")) }
        override val state = MutableStateFlow<PushState>(PushState.None)
        override val dings = MutableSharedFlow<Unit>(extraBufferCapacity = 8)

        override fun subscribe(vapidPublicKey: String) {
            calls += "subscribe $vapidPublicKey"
            state.value = PushState.Subscribing
            answer(vapidPublicKey)?.let { state.value = it }
        }

        override fun unsubscribe() {
            calls += "unsubscribe"
            state.value = PushState.None
        }
    }

    private class FakeDistributors : Distributors {
        var resolved: Distributor = Distributor.Found(NTFY)
        val saved = mutableListOf<String>()
        override val changes = MutableSharedFlow<Unit>(extraBufferCapacity = 8)
        override fun resolve() = resolved
        override fun save(packageName: String) {
            saved += packageName
            resolved = Distributor.Found(packageName)
        }
    }

    private class MemoryStore : WakeUpStore {
        override var choice: WakeUpChoice = WakeUpChoice.TimerOnly
        override var ownBell: BellAddress? = null
        override var registration: BellRegistration? = null
    }

    private class FakePos(till: TestTill) : PosSide {
        val pointOfSale = till.pointOfSale
        /** Null: not signed in. */
        var applicationId: String? = till.applicationId
        var environment = till.environment
        val locationId = till.locationId
        var token: String? = till.token
        override val signedIn get() = applicationId?.let { PosSignIn(pointOfSale, it, environment, locationId) }
        override suspend fun accessToken() = token
    }

    private val bell = FakeBell()
    private val push = FakePush()
    private val distributors = FakeDistributors()
    private val store = MemoryStore()
    private val pos = FakePos(till)

    /** What the till posts: its token, its location and the endpoint. */
    private val posted = "${till.token} ${till.locationId}"

    /** A Bell's info with an entry for the till's point of sale only. */
    private fun info(app: String = till.applicationId, environment: String = till.environment, key: String = KEY) =
        { BellInfo(mapOf(till.pointOfSale.wire to mapOf(environment to app)), key) }

    init {
        bell.infos[monokulo] = info()
        bell.infos[own] = info()
    }

    /** The app's process: a new start ends the one before. */
    private var process: Job? = null

    /** A fresh start of the app over [store], on this test's clock. */
    private fun TestScope.start(): BellWakeUps {
        process?.cancel()
        val job = Job(backgroundScope.coroutineContext[Job]).also { process = it }
        return BellWakeUps(
            push = push,
            distributors = distributors,
            bells = bell,
            store = store,
            monokulo = monokulo,
            pos = pos,
            scope = CoroutineScope(backgroundScope.coroutineContext + job),
            now = { testScheduler.currentTime / 1000 },
        ).also { runCurrent() }
    }

    /** Every status [flow] takes from now on. */
    private fun TestScope.statuses(flow: StateFlow<WakeUpStatus>): List<WakeUpStatus> {
        val seen = mutableListOf<WakeUpStatus>()
        backgroundScope.launch { flow.collect { seen += it } }
        runCurrent()
        return seen
    }

    private fun TestScope.connected(choice: WakeUpChoice = WakeUpChoice.MonokuloBell): BellWakeUps = start().also {
        it.choose(choice)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), it.status.value)
        bell.calls.clear()
        push.calls.clear()
    }

    private fun TestScope.wait(duration: Duration) {
        advanceTimeBy(duration)
        runCurrent()
    }

    @Test
    fun `the pace is 30 s while connected and 2 s for every other status`() {
        val statuses = listOf(
            WakeUpStatus.Off, WakeUpStatus.NoPos, WakeUpStatus.NoDistributor, WakeUpStatus.ChooseDistributor(listOf("a", "b")),
            WakeUpStatus.Connecting, WakeUpStatus.MissingWakeups(5), WakeUpStatus.Unreachable("no connection"),
            WakeUpStatus.Refused("x", 5), WakeUpStatus.SignInNeeded(till.pointOfSale, "sq0idp-x"),
        )
        statuses.forEach { assertEquals(2.seconds, it.interval, "$it") }
        assertEquals(30.seconds, WakeUpStatus.Connected(null).interval)
        assertEquals(30.seconds, WakeUpStatus.Connected(5).interval)
    }

    @Test
    fun `timer only, a fresh install, asks nothing of anyone (A2_11)`() = runTest {
        val wakeUps = start()
        wait(30.minutes)
        assertEquals(WakeUpChoice.TimerOnly, wakeUps.choice.value)
        assertEquals(WakeUpStatus.Off, wakeUps.status.value)
        assertEquals(emptyList(), bell.calls)
        assertEquals(emptyList(), push.calls)
        wakeUps.recheck()
        wakeUps.timerFound(3)
        runCurrent()
        assertEquals(emptyList(), bell.calls)
    }

    @Test
    fun `picking the Bell saves it, says Connecting at once, then reads, subscribes and registers (A3_13, A3_14)`() = runTest {
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        assertEquals(WakeUpChoice.MonokuloBell, store.choice)
        assertEquals(WakeUpChoice.MonokuloBell, wakeUps.choice.value)
        assertEquals(WakeUpStatus.Connecting, wakeUps.status.value)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com", "POST bell.monokulo.com $posted https://ntfy.sh/up1"), bell.calls)
        assertEquals(listOf("subscribe $KEY"), push.calls)
        val registration = assertNotNull(store.registration)
        assertEquals("br_1", registration.registrationId)
        assertEquals(monokulo, registration.bell)
        assertEquals(KEY, registration.vapidPublicKey)
        assertEquals(NTFY, registration.distributor)
        assertEquals("https://ntfy.sh/up1", registration.subscription.endpoint)
    }

    @Test
    fun `picking My own Bell keeps its address for the row`() = runTest {
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.OwnBell(own))
        runCurrent()
        assertEquals(own, store.ownBell)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals("GET bell.thebeanroom.coffee", bell.calls.first())
    }

    @Test
    fun `picking the Bell already chosen does nothing (A3_18)`() = runTest {
        val wakeUps = connected()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(emptyList(), bell.calls)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `a Bell that can't be reached is Unreachable, retried 1, 2, 4 minutes on to 15, and connects when it's back (A11_1, A4_20)`() = runTest {
        bell.infos.remove(monokulo)
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Unreachable("no connection"), wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls)
        assertEquals(emptyList(), push.calls, "nothing subscribed for a Bell not read")
        val gaps = listOf(1, 2, 4, 8, 15, 15).map { it.minutes }
        gaps.forEachIndexed { i, gap ->
            wait(gap - 1.seconds)
            assertEquals(i + 1, bell.calls.size, "not before ${gap}")
            wait(1.seconds)
            assertEquals(i + 2, bell.calls.size, "after $gap")
        }
        bell.infos[monokulo] = info()
        wait(15.minutes)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertTrue(bell.calls.last().startsWith("POST"))
    }

    @Test
    fun `an unreachable Bell's reasons, in the cashier's words`() {
        assertEquals("no connection", BellWakeUps.unreachableReason(ConnectException()))
        assertEquals("no connection", BellWakeUps.unreachableReason(IOException("no route")))
        assertEquals("it didn't answer", BellWakeUps.unreachableReason(SocketTimeoutException()))
        assertEquals("it didn't answer", BellWakeUps.unreachableReason(java.io.InterruptedIOException("timeout")))
        assertEquals("it answered 503", BellWakeUps.unreachableReason(BellException(503, null, null, null)))
        assertEquals("it isn't a Bell", BellWakeUps.unreachableReason(NotABellException()))
    }

    @Test
    fun `an answer that isn't a Bell's is Unreachable and nothing is posted (A11_7)`() = runTest {
        bell.infos[own] = { throw NotABellException() }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.OwnBell(own))
        runCurrent()
        assertEquals(WakeUpStatus.Unreachable("it isn't a Bell"), wakeUps.status.value)
        assertEquals(listOf("GET bell.thebeanroom.coffee"), bell.calls)
    }

    @Test
    fun `a Bell without the till's environment is refused for good in words naming it, with no sign-in and no post (A3_14)`() = runTest {
        bell.infos[monokulo] = info(app = till.otherApplicationId, environment = till.otherEnvironment)
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Refused(till.notRungHere), wakeUps.status.value)
        assertEquals(WakeUpStatus.Refused(PosWords.notRung(till.pointOfSale, till.environment)), wakeUps.status.value)
        wait(60.minutes)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls, "no retry on its own")
        assertEquals(emptyList(), push.calls)
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(2, bell.calls.size, "re-picking a refused Bell tries again")
    }

    @Test
    fun `a till signed in elsewhere is refused by a Bell for this environment alone, naming the till's`() = runTest {
        pos.environment = till.otherEnvironment
        val wakeUps = BellWakeUps(push, distributors, bell, store, monokulo, pos, backgroundScope, { 0 })
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Refused(PosWords.notRung(till.pointOfSale, till.otherEnvironment)), wakeUps.status.value)
    }

    @Test
    fun `a Bell ringing both environments registers the till in its own, through its own application (decision 305)`() = runTest {
        bell.infos[monokulo] = {
            BellInfo(mapOf(till.pointOfSale.wire to mapOf(till.otherEnvironment to till.otherApplicationId, till.environment to till.applicationId)), KEY)
        }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(listOf(till.environment), bell.registeredIn)
        assertEquals(till.pointOfSale.wire, store.registration?.pointOfSale)
        assertEquals(till.environment, store.registration?.environment)
    }

    @Test
    fun `a Bell ringing both environments asks for a sign-in through the till's environment's application, not the other's`() = runTest {
        val bellsOwn = till.otherApplicationId
        bell.infos[monokulo] = {
            BellInfo(mapOf(till.pointOfSale.wire to mapOf(till.otherEnvironment to "another-environments-app", till.environment to bellsOwn)), KEY)
        }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.SignInNeeded(till.pointOfSale, bellsOwn), wakeUps.status.value)
        assertFalse(bell.calls.any { it.startsWith("POST") })
    }

    @Test
    fun `a till moved to the other environment takes its registration back and registers again in the new one (decision 306)`() = runTest {
        bell.infos[monokulo] = {
            BellInfo(mapOf(till.pointOfSale.wire to mapOf(till.otherEnvironment to till.otherApplicationId, till.environment to till.applicationId)), KEY)
        }
        val wakeUps = connected()
        pos.environment = till.otherEnvironment
        pos.applicationId = till.otherApplicationId
        wakeUps.recheck()
        runCurrent()
        assertEquals(
            listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1", "POST bell.monokulo.com $posted https://ntfy.sh/up1"),
            bell.calls,
        )
        assertEquals(listOf(till.environment, till.otherEnvironment), bell.registeredIn)
        assertEquals(till.otherEnvironment, store.registration?.environment)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `a registration kept from before registrations named their environment is taken back and made again at the next start (decision 306)`() = runTest {
        store.choice = WakeUpChoice.MonokuloBell
        store.registration = BellRegistration(monokulo, "br_old", KEY, PushSubscription("https://ntfy.sh/up1", "BKey", "auth"), NTFY)
        val wakeUps = start()
        assertEquals(
            listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_old", "POST bell.monokulo.com $posted https://ntfy.sh/up1"),
            bell.calls,
        )
        assertEquals(till.environment, store.registration?.environment)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        wakeUps.recheck()
        runCurrent()
        assertEquals(1, bell.calls.count { it.startsWith("DELETE") }, "one made for the till's environment is kept: ${bell.calls}")
    }

    @Test
    fun `a Bell that doesn't run the till's point of sale is refused for good, before its environment or application (T5_2)`() = runTest {
        val other = PointOfSale.entries.single { it != till.pointOfSale }
        bell.infos[monokulo] = { BellInfo(mapOf(other.wire to mapOf("elsewhere" to "someone-else")), KEY) }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Refused("it doesn't ring ${till.pointOfSale.displayName} tills"), wakeUps.status.value)
        wait(60.minutes)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls, "no retry on its own")
        assertEquals(emptyList(), push.calls)
    }

    @Test
    fun `the environment is checked before the application (T5_2)`() = runTest {
        bell.infos[monokulo] = info(app = till.otherApplicationId, environment = till.otherEnvironment)
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertTrue(wakeUps.status.value is WakeUpStatus.Refused, "${wakeUps.status.value}")
    }

    @Test
    fun `an environment this build doesn't know is passed over, beside the till's or alone`() = runTest {
        bell.infos[monokulo] = { BellInfo(mapOf(till.pointOfSale.wire to mapOf("staging" to "staging-app", till.environment to till.applicationId)), KEY) }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        bell.infos[own] = info(environment = "staging")
        wakeUps.choose(WakeUpChoice.OwnBell(own))
        runCurrent()
        assertEquals(WakeUpStatus.Refused(PosWords.notRung(till.pointOfSale, till.environment)), wakeUps.status.value)
    }

    @Test
    fun `a Bell for both points of sale registers the till for its own, with its own location`() = runTest {
        val other = PointOfSale.entries.single { it != till.pointOfSale }
        bell.infos[monokulo] = {
            BellInfo(
                mapOf(
                    other.wire to mapOf("elsewhere" to "someone-else"),
                    till.pointOfSale.wire to mapOf(till.environment to till.applicationId),
                ),
                KEY,
            )
        }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(listOf(till.pointOfSale), bell.registeredFor)
        assertEquals("POST bell.monokulo.com $posted https://ntfy.sh/up1", bell.calls.last())
    }

    @Test
    fun `a Bell on another application asks for a sign-in through it, and registers nothing until then (A6_26)`() = runTest {
        bell.infos[monokulo] = info(app = till.otherApplicationId)
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.SignInNeeded(till.pointOfSale, till.otherApplicationId), wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls)
        assertEquals(emptyList(), push.calls, "no distributor registration")
        wait(60.minutes)
        assertEquals(1, bell.calls.size, "no retry on its own")
        // The sign-in through the Bell's application finished; the app asks again.
        pos.applicationId = till.otherApplicationId
        wakeUps.recheck()
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `without a sign-in the Bell waits for one (A3_14)`() = runTest {
        pos.applicationId = null
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.NoPos, wakeUps.status.value)
        assertEquals(WakeUpChoice.MonokuloBell, store.choice)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls)
    }

    @Test
    fun `a sign-in the point of sale ended at the token is NoPos, and takes back what was registered`() = runTest {
        val wakeUps = connected()
        pos.token = null
        wakeUps.recheck()
        runCurrent()
        assertEquals(WakeUpStatus.NoPos, wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1"), bell.calls)
        assertNull(store.registration)
    }

    @Test
    fun `no distributor is NoDistributor with the choice kept, and one installed connects by itself (A7_29, A7_30)`() = runTest {
        distributors.resolved = Distributor.None
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.NoDistributor, wakeUps.status.value)
        assertEquals(WakeUpChoice.MonokuloBell, store.choice)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls, "no POST")
        distributors.resolved = Distributor.Found(NTFY)
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `several distributors and no default, the cashier picks, and it is saved (A7_31)`() = runTest {
        distributors.resolved = Distributor.Choose(listOf("io.heckel.ntfy", "org.unifiedpush.distributor.sunup"))
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.ChooseDistributor(listOf("io.heckel.ntfy", "org.unifiedpush.distributor.sunup")), wakeUps.status.value)
        wakeUps.chooseDistributor("org.unifiedpush.distributor.sunup")
        assertEquals(WakeUpStatus.Connecting, wakeUps.status.value)
        runCurrent()
        assertEquals(listOf("org.unifiedpush.distributor.sunup"), distributors.saved)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `the distributor failing is Unreachable with its reason, retried`() = runTest {
        push.answer = { PushState.Failed(PushFailure.Other) }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.PUSH_REFUSED), wakeUps.status.value)
        push.answer = { PushState.Failed(PushFailure.NoKeys) }
        wait(1.minutes)
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.PUSH_NO_KEYS), wakeUps.status.value)
        push.answer = { null }
        wait(2.minutes + BellWakeUps.SUBSCRIBE_WAIT)
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.PUSH_SILENT), wakeUps.status.value)
        assertFalse(bell.calls.any { it.startsWith("POST") })
        assertEquals(BellWakeUps.PUSH_OFFLINE, (BellWakeUps.pushFailedStatus(PushFailure.Network) as WakeUpStatus.Unreachable).reason)
        assertEquals(WakeUpStatus.NoDistributor, BellWakeUps.pushFailedStatus(PushFailure.NoDistributor))
    }

    @Test
    fun `each problem type is its refusal, and only rate-limited retries by itself (A1_7, A11_5, A11_6)`() {
        fun problem(status: Int, type: String?, detail: String? = null, retryAfter: Long? = null, at: PointOfSale? = PointOfSale.Square) =
            BellWakeUps.problemStatus(BellException(status, type, detail, retryAfter), at, now = 1000)
        assertEquals(WakeUpStatus.Refused("Square didn't confirm this location. Sign in to Square again."), problem(401, "square-refused"))
        assertEquals(
            WakeUpStatus.Refused("Clover didn't confirm this merchant. Sign in to Clover again."),
            problem(401, "clover-refused", at = PointOfSale.Clover),
        )
        assertEquals(WakeUpStatus.Refused("this merchant already has 32 tills registered"), problem(409, "too-many-registrations", at = PointOfSale.Clover))
        assertEquals(
            WakeUpStatus.Refused("this device is registered under another Clover account"),
            problem(409, "endpoint-registered", at = PointOfSale.Clover),
        )
        assertEquals(WakeUpStatus.Unreachable("it answered 503"), problem(503, "clover-unavailable", at = PointOfSale.Clover))
        // Another point of sale's slug is a refusal like any other, in the Bell's words.
        assertEquals(WakeUpStatus.Refused("Refused by Clover"), problem(401, "clover-refused", "Refused by Clover"))
        assertEquals(WakeUpStatus.Refused("it answered 401"), problem(401, "square-refused", at = null))
        assertEquals(WakeUpStatus.Refused("this location already has 32 tills registered"), problem(409, "too-many-registrations"))
        assertEquals(WakeUpStatus.Refused("this device is registered under another Square account"), problem(409, "endpoint-registered"))
        assertEquals(
            WakeUpStatus.Refused("too many registrations from this network: trying again in about a minute", retryAt = 1042),
            problem(429, "rate-limited", retryAfter = 42),
        )
        assertEquals(
            WakeUpStatus.Refused("too many registrations from this network: trying again in about 10 minutes", retryAt = 1600),
            problem(429, "rate-limited", retryAfter = 600),
        )
        assertEquals(listOf("about a minute", "about a minute", "about 2 minutes", "about 15 minutes"), listOf(1L, 90L, 91L, 900L).map(BellWakeUps::about))
        assertEquals(WakeUpStatus.Refused("subscription.keys.p256dh isn't a P-256 point"), problem(400, "invalid-request", "subscription.keys.p256dh isn't a P-256 point"))
        assertEquals(WakeUpStatus.Refused("it answered 413"), problem(413, "payload-too-large"))
        assertEquals(WakeUpStatus.Unreachable("it answered 503"), problem(503, "square-unavailable", "Square didn't answer"))
        assertEquals(WakeUpStatus.Unreachable("it answered 500"), problem(500, "internal"))
        assertEquals(WakeUpStatus.Unreachable("it answered 502"), problem(502, null))
    }

    @Test
    fun `a refusal waits for a re-pick, a sign-in or a start, but rate-limited comes back after Retry-After`() = runTest {
        bell.register = { throw BellException(409, "endpoint-registered", null, null) }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Refused("this device is registered under another ${till.pointOfSale.displayName} account"), wakeUps.status.value)
        wait(60.minutes)
        assertEquals(2, bell.calls.size)
        bell.register = { throw BellException(429, "rate-limited", null, 30) }
        wakeUps.recheck()
        runCurrent()
        val refused = wakeUps.status.value as WakeUpStatus.Refused
        assertEquals(testScheduler.currentTime / 1000 + 30, refused.retryAt)
        bell.register = { BellRegistered("br_9") }
        wait(29.seconds)
        assertEquals(4, bell.calls.size)
        wait(1.seconds)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals("br_9", store.registration?.registrationId)
    }

    @Test
    fun `switching Bells takes the old registration back before registering with the new (A3_17)`() = runTest {
        val wakeUps = connected()
        wakeUps.choose(WakeUpChoice.OwnBell(own))
        runCurrent()
        assertEquals(
            listOf("DELETE bell.monokulo.com br_1", "GET bell.thebeanroom.coffee", "POST bell.thebeanroom.coffee $posted https://ntfy.sh/up1"),
            bell.calls,
        )
        assertEquals(own, store.registration?.bell)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `timer only takes the registration back once, ends the subscription and keeps My own Bell's address (A10_1)`() = runTest {
        val wakeUps = connected(WakeUpChoice.OwnBell(own))
        wakeUps.choose(WakeUpChoice.TimerOnly)
        assertEquals(WakeUpStatus.Off, wakeUps.status.value)
        runCurrent()
        assertEquals(listOf("DELETE bell.thebeanroom.coffee br_1"), bell.calls)
        assertEquals(listOf("unsubscribe"), push.calls)
        assertNull(store.registration)
        assertEquals(WakeUpChoice.TimerOnly, store.choice)
        assertEquals(own, store.ownBell)
        wakeUps.recheck()
        runCurrent()
        assertEquals(1, bell.calls.size)
    }

    @Test
    fun `signing out takes the registration back and keeps the choice, awaited (A10_2)`() = runTest {
        val wakeUps = connected()
        wakeUps.unregister()
        assertEquals(listOf("DELETE bell.monokulo.com br_1"), bell.calls)
        assertEquals(listOf("unsubscribe"), push.calls)
        assertNull(store.registration)
        assertEquals(WakeUpChoice.MonokuloBell, store.choice)
        assertEquals(WakeUpStatus.NoPos, wakeUps.status.value)
        // Signed in again (same application): registers with no re-pick.
        wakeUps.recheck()
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `a registration taken back takes its Last wake-up with it, so the next one starts at None yet`() = runTest {
        val wakeUps = connected()
        push.dings.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(testScheduler.currentTime / 1000), wakeUps.status.value)
        // The point of sale changed (or the till signed out): the registration goes, then comes back for the new sign-in.
        wakeUps.unregister()
        wakeUps.recheck()
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `disconnecting takes the registration back and forgets the choice (A10_3)`() = runTest {
        val wakeUps = connected()
        wakeUps.unregister(forget = true)
        assertEquals(listOf("DELETE bell.monokulo.com br_1"), bell.calls)
        assertEquals(WakeUpChoice.TimerOnly, store.choice)
        assertEquals(WakeUpChoice.TimerOnly, wakeUps.choice.value)
        assertEquals(WakeUpStatus.Off, wakeUps.status.value)
        wait(30.minutes)
        assertEquals(1, bell.calls.size, "and nothing after")
    }

    @Test
    fun `a Bell that can't take the registration back is tried once and dropped here (A10_7)`() = runTest {
        val wakeUps = connected()
        bell.unregister = { throw ConnectException() }
        wakeUps.unregister()
        assertEquals(listOf("DELETE bell.monokulo.com br_1"), bell.calls)
        assertNull(store.registration)
    }

    @Test
    fun `a Bell that never answers the DELETE holds the sign-out 10 s at most (A10_7)`() = runTest {
        val wakeUps = connected()
        bell.unregister = { awaitCancellation() }
        val started = testScheduler.currentTime
        wakeUps.unregister()
        assertEquals(10_000L, testScheduler.currentTime - started)
        assertNull(store.registration)
        assertEquals(WakeUpStatus.NoPos, wakeUps.status.value)
    }

    @Test
    fun `unregistering stops a registration under way, and takes back what it got`() = runTest {
        val wakeUps = start()
        val held = CompletableDeferred<Unit>()
        bell.register = { BellRegistered("br_1").also { held.complete(Unit) } }
        push.answer = { null }
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Connecting, wakeUps.status.value, "waiting on the distributor")
        wakeUps.unregister(forget = true)
        assertFalse(bell.calls.any { it.startsWith("POST") })
        assertEquals(WakeUpStatus.Off, wakeUps.status.value)
        assertFalse(held.isCompleted)
    }

    @Test
    fun `every app start registers again with the same subscription (A4_21, A3_19)`() = runTest {
        connected()
        val second = start()
        assertEquals(WakeUpChoice.MonokuloBell, second.choice.value)
        assertEquals(listOf("GET bell.monokulo.com", "POST bell.monokulo.com $posted https://ntfy.sh/up1"), bell.calls)
        assertEquals(listOf("subscribe $KEY"), push.calls, "the connector asked again, as its docs say")
        assertEquals(WakeUpStatus.Connected(null), second.status.value)
    }

    @Test
    fun `a shift starting registers again without leaving Connected, so the pace holds (A4_21)`() = runTest {
        val wakeUps = connected()
        val seen = statuses(wakeUps.status)
        wakeUps.recheck()
        runCurrent()
        assertEquals(listOf("GET bell.monokulo.com", "POST bell.monokulo.com $posted https://ntfy.sh/up1"), bell.calls)
        assertEquals(emptyList(), push.calls, "the subscription this run made is still good")
        assertTrue(seen.all { it is WakeUpStatus.Connected }, "$seen")
    }

    @Test
    fun `an app start with timer only finishes taking back a registration left behind`() = runTest {
        store.registration = BellRegistration(monokulo, "br_old", KEY, PushSubscription("https://ntfy.sh/up0", "BKey", "auth"), NTFY)
        val wakeUps = start()
        assertEquals(listOf("DELETE bell.monokulo.com br_old"), bell.calls)
        assertNull(store.registration)
        assertEquals(WakeUpStatus.Off, wakeUps.status.value)
    }

    @Test
    fun `a new endpoint from the distributor deletes the old registration, then registers the new (A7_32)`() = runTest {
        val wakeUps = connected()
        push.state.value = PushState.Subscribed(PushSubscription("https://ntfy.sh/up2", "BKey2", "auth2"))
        runCurrent()
        assertEquals(
            listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1", "POST bell.monokulo.com $posted https://ntfy.sh/up2"),
            bell.calls,
        )
        assertEquals("https://ntfy.sh/up2", store.registration?.subscription?.endpoint)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `an endpoint changed while the app was away is caught at the next start`() = runTest {
        connected()
        push.endpoint = "https://ntfy.sh/up2"
        start()
        assertEquals(
            listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1", "POST bell.monokulo.com $posted https://ntfy.sh/up2"),
            bell.calls,
        )
    }

    @Test
    fun `the distributor ending the subscription takes the registration back and is NoDistributor (A7_32)`() = runTest {
        val wakeUps = connected()
        push.state.value = PushState.Failed(PushFailure.Lost)
        runCurrent()
        assertEquals(listOf("DELETE bell.monokulo.com br_1"), bell.calls)
        assertNull(store.registration)
        assertEquals(WakeUpStatus.NoDistributor, wakeUps.status.value)
    }

    @Test
    fun `the distributor uninstalled takes the registration back, and one installed again connects (A7_32, A7_30)`() = runTest {
        val wakeUps = connected()
        distributors.resolved = Distributor.None
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.NoDistributor, wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1"), bell.calls)
        assertEquals(listOf("unsubscribe"), push.calls)
        distributors.resolved = Distributor.Found(NTFY)
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `the distributor uninstalled with two others left takes the registration back before the cashier picks (decision 108)`() = runTest {
        val wakeUps = connected()
        val left = listOf("org.unifiedpush.distributor.nextpush", "org.unifiedpush.distributor.sunup")
        distributors.resolved = Distributor.Choose(left)
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1"), bell.calls)
        assertNull(store.registration)
        assertEquals(WakeUpStatus.ChooseDistributor(left), wakeUps.status.value)
    }

    /** The used distributor uninstalled with [NEXTPUSH] left: the change of apps, and the connector's end of the subscription in [endFirst] order. */
    private suspend fun TestScope.uninstalledWithAnotherLeft(endFirst: Boolean) {
        val wakeUps = connected()
        val seen = statuses(wakeUps.status)
        distributors.resolved = Distributor.Found(NEXTPUSH)
        if (endFirst) push.state.value = PushState.Failed(PushFailure.Lost)
        distributors.changes.emit(Unit)
        if (!endFirst) push.state.value = PushState.Failed(PushFailure.Lost)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertFalse(WakeUpStatus.NoDistributor in seen, "$seen")
        assertNotNull(store.registration)
        assertEquals(PushState.Subscribed(PushSubscription(push.endpoint, "BKey", "auth")), push.state.value)
        assertTrue(bell.calls.last().startsWith("POST bell.monokulo.com"), "${bell.calls}")
        wait(30.minutes)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `the distributor uninstalled with another left connects through it when the connector's end comes after (decision 118)`() = runTest {
        uninstalledWithAnotherLeft(endFirst = false)
    }

    @Test
    fun `the distributor uninstalled with another left connects through it when the connector's end comes first (decision 118)`() = runTest {
        uninstalledWithAnotherLeft(endFirst = true)
    }

    @Test
    fun `the distributor uninstalled with two left and the connector's end arriving asks the cashier, never NoDistributor (decision 118)`() = runTest {
        val wakeUps = connected()
        val left = listOf(NEXTPUSH, "org.unifiedpush.distributor.sunup")
        val seen = statuses(wakeUps.status)
        distributors.resolved = Distributor.Choose(left)
        distributors.changes.emit(Unit)
        push.state.value = PushState.Failed(PushFailure.Lost)
        runCurrent()
        assertEquals(WakeUpStatus.ChooseDistributor(left), wakeUps.status.value)
        assertFalse(WakeUpStatus.NoDistributor in seen, "$seen")
        assertNull(store.registration)
    }

    @Test
    fun `a distributor installed after the last one went keeps its own end of the subscription, read as the cashier's (decision 118)`() = runTest {
        val wakeUps = connected()
        distributors.resolved = Distributor.None
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.NoDistributor, wakeUps.status.value)
        distributors.resolved = Distributor.Found(NEXTPUSH)
        push.endpoint = "https://push.example/np1"
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(NEXTPUSH, store.registration?.distributor)
        bell.calls.clear()
        push.state.value = PushState.Failed(PushFailure.Lost)
        runCurrent()
        assertEquals(WakeUpStatus.NoDistributor, wakeUps.status.value)
        assertEquals(listOf("DELETE bell.monokulo.com br_2"), bell.calls)
        wait(30.minutes)
        assertFalse(bell.calls.any { it.startsWith("POST") }, "${bell.calls}")
    }

    @Test
    fun `a distributor uninstalled while the app was away is known at the start from the registration (decision 122)`() = runTest {
        connected()
        assertEquals(NTFY, store.registration?.distributor)
        distributors.resolved = Distributor.Found(NEXTPUSH)
        push.endpoint = "https://push.example/np1"
        val wakeUps = start()
        val seen = statuses(wakeUps.status)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(NEXTPUSH, store.registration?.distributor)
        bell.calls.clear()
        // The connector's end of the old distributor's token, broadcast by the start's look.
        push.state.value = PushState.Failed(PushFailure.Lost)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertFalse(WakeUpStatus.NoDistributor in seen, "$seen")
        assertEquals(
            listOf("DELETE bell.monokulo.com br_2", "GET bell.monokulo.com", "POST bell.monokulo.com $posted https://push.example/np1"),
            bell.calls,
        )
        assertEquals(NEXTPUSH, store.registration?.distributor)
    }

    @Test
    fun `a pass failing on this device for any reason leaves the till running and retries (decision 120)`() = runTest {
        bell.infos[monokulo] = { throw IllegalStateException("the store couldn't be read") }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.TILL_FAILED), wakeUps.status.value)
        assertTrue(process!!.isActive)
        bell.infos[monokulo] = info()
        wait(1.minutes)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        bell.infos[own] = { throw IllegalStateException() }
        wakeUps.choose(WakeUpChoice.OwnBell(own))
        runCurrent()
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.TILL_FAILED), wakeUps.status.value)
        bell.infos[own] = info()
        wakeUps.choose(WakeUpChoice.OwnBell(own))
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `an app installed that isn't the distributor changes nothing`() = runTest {
        connected()
        distributors.changes.emit(Unit)
        runCurrent()
        assertEquals(emptyList(), bell.calls)
    }

    @Test
    fun `a rotated VAPID key subscribes again for it, then registers the new subscription (A9_8)`() = runTest {
        val wakeUps = connected()
        bell.infos[monokulo] = info(key = "BNewKey")
        push.endpoint = "https://ntfy.sh/up2"
        wakeUps.recheck()
        runCurrent()
        assertEquals(listOf("unsubscribe", "subscribe BNewKey"), push.calls)
        assertEquals(
            listOf("GET bell.monokulo.com", "DELETE bell.monokulo.com br_1", "POST bell.monokulo.com $posted https://ntfy.sh/up2"),
            bell.calls,
        )
        assertEquals("BNewKey", store.registration?.vapidPublicKey)
    }

    @Test
    fun `a rotated key is caught at an app start too`() = runTest {
        connected()
        bell.infos[monokulo] = info(key = "BNewKey")
        start()
        assertEquals(listOf("unsubscribe", "subscribe BNewKey"), push.calls)
    }

    @Test
    fun `a timer-found sale with no ding in 60 s is a missed wake-up, 2 s and one registration again (A11_3)`() = runTest {
        val wakeUps = connected()
        wait(61.seconds)
        wakeUps.timerFound(1)
        assertEquals(WakeUpStatus.MissingWakeups(null), wakeUps.status.value)
        assertEquals(2.seconds, wakeUps.status.value.interval)
        runCurrent()
        assertEquals(listOf("GET bell.monokulo.com", "POST bell.monokulo.com $posted https://ntfy.sh/up1"), bell.calls)
        assertEquals(WakeUpStatus.MissingWakeups(null), wakeUps.status.value, "registering again proves nothing")
        wakeUps.timerFound(2)
        runCurrent()
        assertEquals(2, bell.calls.size, "once")
        wait(5.seconds)
        push.dings.emit(Unit)
        runCurrent()
        val at = testScheduler.currentTime / 1000
        assertEquals(WakeUpStatus.Connected(at), wakeUps.status.value)
        assertEquals(30.seconds, wakeUps.status.value.interval)
    }

    @Test
    fun `a ding then a timer-found sale stays Connected (A11_3)`() = runTest {
        val wakeUps = connected()
        wait(5.minutes)
        push.dings.emit(Unit)
        runCurrent()
        val at = testScheduler.currentTime / 1000
        assertEquals(WakeUpStatus.Connected(at), wakeUps.status.value, "Last wake-up")
        wait(30.seconds)
        wakeUps.timerFound(1)
        runCurrent()
        assertEquals(WakeUpStatus.Connected(at), wakeUps.status.value)
        assertEquals(emptyList(), bell.calls)
        wait(31.seconds)
        wakeUps.timerFound(1)
        assertEquals(WakeUpStatus.MissingWakeups(at), wakeUps.status.value, "61 s after the ding")
    }

    @Test
    fun `a timer check that finds nothing, or one just after connecting, proves nothing`() = runTest {
        val wakeUps = connected()
        wait(30.seconds)
        wakeUps.timerFound(1)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value, "a sale from before the Bell was connected")
        wait(10.minutes)
        wakeUps.timerFound(0)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
    }

    @Test
    fun `a timer-found sale while not connected changes nothing`() = runTest {
        bell.infos.remove(monokulo)
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        wait(10.seconds)
        wakeUps.timerFound(3)
        assertEquals(WakeUpStatus.Unreachable("no connection"), wakeUps.status.value)
    }

    @Test
    fun `an endpoint the distributor gives after the pass stopped waiting registers at once (decision 123)`() = runTest {
        push.answer = { null }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        wait(BellWakeUps.SUBSCRIBE_WAIT)
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.PUSH_SILENT), wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com"), bell.calls)
        // The distributor, opened at last, answers the registration it was sent.
        push.state.value = PushState.Subscribed(PushSubscription(push.endpoint, "BKey", "auth"))
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(listOf("GET bell.monokulo.com", "GET bell.monokulo.com", "POST bell.monokulo.com $posted https://ntfy.sh/up1"), bell.calls)
        assertEquals(listOf("subscribe $KEY"), push.calls, "the subscription held is the one registered")
        // The wait that was set is gone with it: nothing more is asked of the Bell.
        wait(20.minutes)
        assertEquals(3, bell.calls.size)
    }

    @Test
    fun `an unreachable Bell is tried again on recheck, before its wait is out (decision 123)`() = runTest {
        push.answer = { null }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        wait(BellWakeUps.SUBSCRIBE_WAIT)
        assertEquals(WakeUpStatus.Unreachable(BellWakeUps.PUSH_SILENT), wakeUps.status.value)
        push.answer = { PushState.Subscribed(PushSubscription(push.endpoint, "BKey", "auth")) }
        wait(5.seconds)
        wakeUps.recheck()
        runCurrent()
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value)
        assertEquals(1, bell.calls.count { it.startsWith("POST") })
    }

    @Test
    fun `a ding while not connected changes no status`() = runTest {
        pos.applicationId = null
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        push.dings.emit(Unit)
        runCurrent()
        assertEquals(WakeUpStatus.NoPos, wakeUps.status.value)
    }

    @Test
    fun `the Bell's refusals and outages for the till's point of sale are its words (T5_4)`() = runTest {
        bell.register = { throw BellException(401, "${till.pointOfSale.wire}-refused", null, null) }
        val wakeUps = start()
        wakeUps.choose(WakeUpChoice.MonokuloBell)
        runCurrent()
        assertEquals(WakeUpStatus.Refused(PosWords.refused(till.pointOfSale)), wakeUps.status.value)
        bell.register = { throw BellException(503, "${till.pointOfSale.wire}-unavailable", null, null) }
        wakeUps.recheck()
        runCurrent()
        assertEquals(WakeUpStatus.Unreachable("it answered 503"), wakeUps.status.value)
        bell.register = { BellRegistered("br_7") }
        wait(1.minutes)
        assertEquals(WakeUpStatus.Connected(null), wakeUps.status.value, "an outage is retried by itself")
    }

    companion object {
        val SQUARE = TestTill(
            pointOfSale = PointOfSale.Square,
            applicationId = "sandbox-sq0idb-dev",
            environment = "sandbox",
            otherEnvironment = "production",
            otherApplicationId = "sandbox-sq0idb-other",
            locationId = "L1",
            token = "EAAAtoken",
            notRungHere = "it doesn't ring Square sandbox tills",
        )
        val CLOVER = TestTill(
            pointOfSale = PointOfSale.Clover,
            applicationId = "MONOKULARBELL",
            environment = "north_america",
            otherEnvironment = "europe",
            otherApplicationId = "OTHERCLOVERAP",
            locationId = "MCLOVERDEV001",
            token = "clv-access-token",
            notRungHere = "it doesn't ring Clover tills in North America",
        )
        const val KEY = "BOS-key"
        const val NTFY = "io.heckel.ntfy"
        const val NEXTPUSH = "org.unifiedpush.distributor.nextpush"
    }
}

/** Every scenario for a till signed in to Square. */
class BellWakeUpsTest : WakeUpsScenarios(SQUARE)

/** Every scenario for a till signed in to Clover, its merchant as the location (acceptance T5.4). */
class CloverBellWakeUpsTest : WakeUpsScenarios(CLOVER)
