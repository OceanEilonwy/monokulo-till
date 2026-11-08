package com.monokulo.till

import android.app.Application
import android.net.Uri
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.monokulo.till.core.ConnectCode
import com.monokulo.till.core.Connection
import com.monokulo.till.core.MonokuloException
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.Problems
import com.monokulo.till.core.Tor
import com.monokulo.till.notify.SaleNotifications
import com.monokulo.till.push.BellAddress
import com.monokulo.till.push.WakeUpChoice
import com.monokulo.till.push.WakeUpStatus
import com.monokulo.till.sales.CloverMerchant
import com.monokulo.till.sales.Pkce
import com.monokulo.till.sales.PosException
import com.monokulo.till.sales.Standing
import com.monokulo.till.sales.SquareAuth
import com.monokulo.till.sales.SquareLocation
import com.monokulo.till.sales.TenderState
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.standing
import com.monokulo.till.sales.live
import com.monokulo.till.ui.CloverTender
import com.monokulo.till.ui.ReceiptFolds
import com.monokulo.till.ui.RefundSheet
import kotlinx.coroutines.CancellationException
import com.monokulo.till.ui.Motion
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Where the app is. Setup follows the design's six onboarding steps. */
sealed interface Screen {
    /** 1: what Till does. */
    data object Welcome : Screen
    /** 2: scan or paste the store's connect code. */
    data object Scan : Screen
    /** 3: connect to the server, then check the store it is. */
    data class Confirm(val code: ConnectCode) : Screen
    data class Connected(val connection: Connection) : Screen
    /** 4: the point of sale, chosen from Square and Clover, and the chosen one's sign-in (Square's location next). */
    data object PointOfSale : Screen
    data class Locations(val locations: List<SquareLocation>) : Screen
    /** 5: notifications, then the bubble over other apps. */
    data object Alerts : Screen
    data object Float : Screen
    /** 6: start the shift. */
    data object Ready : Screen
    /** The shift (ShiftList): this shift's sales, the till's home. */
    data object Till : Screen
    /** One sale: its code while it's live (SaleA), else its page by standing (row 2b). */
    data class Sale(val orderId: String) : Screen
    /** A live sale's code enlarged, for the customer to scan (CodeB). */
    data class Code(val orderId: String) : Screen
    /** What a live sale's customer bought, every line (PanelE, from the sale's order card). */
    data class Order(val orderId: String) : Screen
    data object Settings : Screen
}

/** Clover's payment type in the screens' terms. */
fun TenderState?.forScreen(): CloverTender = when (this) {
    is TenderState.Ready -> CloverTender.Ready
    is TenderState.AddInClover -> CloverTender.AddInClover
    null -> CloverTender.Looking
}

/** The sale a screen shows, if it shows one. */
fun Screen.orderIdOrNull(): String? = when (this) {
    is Screen.Sale -> orderId
    is Screen.Code -> orderId
    is Screen.Order -> orderId
    else -> null
}

class AppViewModel(app: Application) : AndroidViewModel(app) {
    private val graph = (app as TillApp).graph
    private val stores = graph.stores

    private val screenState = MutableStateFlow(home())
    val screen: StateFlow<Screen> = screenState.asStateFlow()

    /**
     * The screen on show was put there by the till (a new sale coming up,
     * a code going down), not by the cashier: it arrives without growing
     * out of anything (M7). Set with every change of screen, before it.
     */
    @Volatile
    var cameUpByItself = false
        private set

    /** Busy with a network call: buttons wait. */
    val busy = MutableStateFlow(false)

    /** What went wrong last, to show. */
    val problem = MutableStateFlow<String?>(null)

    /** Sales whose full screen the cashier left (Back, or a paid one that stepped back itself): they don't come back up on their own. */
    val left = MutableStateFlow(emptySet<String>())

    /** Sales whose refund the POS hasn't answered yet: their Refund buttons wait. */
    val refunding = MutableStateFlow(emptySet<String>())

    /** Where Settings was opened from, to go back to. */
    private var beforeSettings: Screen? = null

    /** The sign-in under way went through a Bell's application, from Settings: back there once it's done. */
    private var bellSignIn = false

    /** Step 4 was opened from Settings ("Change the point of sale"): back there once the new sign-in is done. */
    private var fromSettings = false

    /** Clover's Monero payment type at the merchant, as the sign-in card shows it (decision 129); null while not known. */
    val tenderState = MutableStateFlow<TenderState?>(null)

    /** The refund sheet over the sale on screen, at a point of sale the till can't refund through (decision 130). */
    val refundSheet = MutableStateFlow<RefundSheet?>(null)

    /** The sale pages' order card folds, kept while a sale's order or code is over its page and through a turn. */
    val receiptFolds = ReceiptFolds()

    /** The sales that were live at the last look, to tell a sale paid on screen and a sale new since. */
    private var liveBefore = emptySet<String>()

    /**
     * The screen the stage has landed on and rests on, as the stage reports it ([landed]); null while a change runs.
     * A new sale rises out of the shift only while the shift is at rest: one arriving under a Back still landing goes
     * into the tray under the moving card (decision 266). The first screen is at rest before anything moves.
     */
    private var restingOn: Screen? = screenState.value
    private var stepBack: Job? = null

    /** A payment landed while the app is on screen: the app gives the one confirming buzz ([PaidBuzz]). */
    val paidBuzz = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    val runner get() = graph.runner.state
    val bubbleWanted get() = graph.bubbleWanted

    val connection: Connection? get() = stores.connection

    /** The store as last read, this session only (decision 50). */
    val store get() = graph.store.now

    /** Settings › Wake-ups: what the cashier chose, and where the chosen Bell stands. */
    val wakeUpChoice get() = graph.wakeUps.choice
    val wakeUpStatus get() = graph.wakeUps.status

    /** The last "My own Bell" address used, kept through Timer only. */
    val ownBell: BellAddress? get() = stores.wakeUps.ownBell

    /** The project's Bell, as the graph parsed it from `BuildConfig.BELL_URL` (decision 98). */
    val monokuloBell: BellAddress get() = graph.monokuloBell

    init {
        // On app start: the store's name and base currency, read live.
        graph.store.read(stores.connection)
        viewModelScope.launch { graph.runner.state.collect { follow(it.till.sales) } }
        // Not watching: the shift's sales still list, to open and refund.
        if (!stores.shiftOn) graph.runner.showSaved()
        // Signed in to Clover: what its card and Settings say about the payment type. An app start
        // re-finds the Monero tender in Clover, or adds it if it's missing; a refusal is shown and
        // only Look again asks once more, never a timer (decision 129). A look that fails (Clover
        // refusing, unreachable or answering something unreadable) leaves the card as it was until
        // Look again; cancellation still ends it.
        if (stores.pointOfSale == PointOfSale.Clover) stores.cloverMerchant?.let { merchant ->
            viewModelScope.launch {
                tenderState.value = try {
                    graph.cloverSource(merchant).ensureTender()
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    null
                }
            }
        }
    }

    /** The sales changed: bring up a new one, move off a code that's down, and step back from one paid on screen. */
    private fun follow(sales: List<TillSale>) {
        val next = TillNav.follow(screenState.value, sales, left.value, liveBefore, restingOn == Screen.Till)
        if (next != screenState.value) {
            cameUpByItself = true
            restingOn = null
            screenState.value = next
        }
        if (TillNav.paymentLanded(sales, liveBefore) && PaidBuzz.by(graph.appVisible.value) == PaidBuzz.Surface.App) paidBuzz.tryEmit(Unit)
        TillNav.paidOnScreen(next, sales, liveBefore)?.let { paid ->
            stepBack?.cancel()
            stepBack = viewModelScope.launch {
                delay(Motion.Dwell.PAID_MS)
                if (screenState.value == Screen.Sale(paid)) leave(paid)
            }
        }
        liveBefore = sales.filter { it.live }.map { it.orderId }.toSet()
    }

    /** Reads the store again: its name, wallet and this device's name can change at any time. */
    fun readStore() = graph.store.read(stores.connection)

    /** Settings, with the store read again as it opens. */
    fun openSettings() {
        readStore()
        retryWakeUps()
        beforeSettings = screenState.value.takeIf { it is Screen.Till || it is Screen.Sale || it is Screen.Code || it is Screen.Order }
        go(Screen.Settings)
    }

    /**
     * The app came forward, or Settings opened: a Bell that couldn't be reached is tried
     * again now rather than at the end of its wait, since what was in the way (a UnifiedPush
     * app just installed and not yet opened, say) may have been seen to (decision 123).
     * Nothing else is retried here: a refusal keeps decision 95's rules.
     */
    fun retryWakeUps() {
        if (graph.wakeUps.status.value is WakeUpStatus.Unreachable) graph.wakeUps.recheck()
    }

    /** Back from Settings: to the sale or the shift it was opened from. */
    fun closeSettings() {
        val before = beforeSettings
        beforeSettings = null
        if (before == null || !stores.setupDone) return goHome()
        // What arrived meanwhile waits in the tray: no sale comes up, only a code or order that's down gives way.
        go(TillNav.follow(before, graph.runner.state.value.till.sales, left.value, liveBefore, restingOnShift = false))
    }
    val location: SquareLocation? get() = stores.location
    val cloverMerchant: CloverMerchant? get() = stores.cloverMerchant

    /** The point of sale chosen, null until the cashier taps one (or with a till that runs without one). */
    val pointOfSale: PointOfSale? get() = stores.pointOfSale

    /** Signed in to the chosen point of sale. */
    val posSignedIn: Boolean
        get() = when (stores.pointOfSale) {
            PointOfSale.Square -> graph.squareAuth().signedIn
            PointOfSale.Clover -> graph.cloverAuth().signedIn
            null -> false
        }

    /** Where the till watches, by name: the Square location, or the Clover merchant. */
    val placeName: String? get() = stores.placeName

    val shiftOn: Boolean get() = stores.shiftOn
    val shiftStartedAt: Long? get() = stores.shiftStartedAt
    val debug: Boolean get() = BuildConfig.DEBUG
    val deviceName: String get() = graph.identity.deviceName
    val tenderName: String
        get() = when (stores.pointOfSale) {
            PointOfSale.Clover -> graph.cloverConfig().tenderLabel
            PointOfSale.Square, null -> graph.squareConfig().tenderName
        }

    val setupDone: Boolean get() = stores.setupDone

    /** Debug builds: the stand-ins' addresses (acceptance T6.5). */
    val baseUrls: Map<PointOfSale, String?>
        get() = mapOf(PointOfSale.Square to stores.squareBaseUrl, PointOfSale.Clover to stores.cloverBaseUrl)

    fun setBaseUrl(pos: PointOfSale, url: String) = when (pos) {
        PointOfSale.Square -> stores.squareBaseUrl = url
        PointOfSale.Clover -> stores.cloverBaseUrl = url
    }

    fun overlayAllowed(): Boolean = Settings.canDrawOverlays(getApplication())

    fun notificationsAllowed(): Boolean = SaleNotifications(getApplication()).allowed()

    private fun home(): Screen = when {
        stores.connection == null -> Screen.Welcome
        stores.setupDone -> Screen.Till
        // Not yet at a Square location or a Clover merchant.
        stores.placeName == null && !stores.withoutPos -> Screen.PointOfSale
        else -> Screen.Alerts
    }

    fun go(screen: Screen) {
        problem.value = null
        cameUpByItself = false
        // Going to the screen already up changes nothing the stage would land from.
        if (screen != screenState.value) restingOn = null
        screenState.value = screen
    }

    /** The stage has landed on [screen] and rests there (`FollowScreen`'s report): a new sale may rise out of the shift now. */
    fun landed(screen: Screen) {
        if (screen == screenState.value) restingOn = screen
    }

    fun goHome() = go(home())

    /** One step back: in setup, from a sale to the shift, from its code or order to the sale; home from elsewhere. */
    fun back() = when (val screen = screenState.value) {
        is Screen.Sale -> leave(screen.orderId)
        is Screen.Code -> go(Screen.Sale(screen.orderId))
        is Screen.Order -> go(Screen.Sale(screen.orderId))
        Screen.Settings -> closeSettings()
        Screen.Scan -> go(Screen.Welcome)
        is Screen.Confirm -> go(Screen.Scan)
        is Screen.Locations -> go(Screen.PointOfSale)
        Screen.PointOfSale -> if (stores.setupDone) closePointOfSale() else goHome()
        Screen.Alerts -> go(Screen.PointOfSale)
        Screen.Float -> go(Screen.Alerts)
        Screen.Ready -> go(Screen.Float)
        else -> goHome()
    }

    /**
     * Where [back] would go from a sale's screens or Settings, without going
     * there: what a back gesture previews before it commits. Null where
     * Back shows no preview.
     */
    fun backTarget(): Screen? = when (val screen = screenState.value) {
        is Screen.Sale -> Screen.Till
        is Screen.Code -> Screen.Sale(screen.orderId)
        is Screen.Order -> Screen.Sale(screen.orderId)
        Screen.Settings -> beforeSettings?.takeIf { stores.setupDone }
            ?.let { TillNav.follow(it, graph.runner.state.value.till.sales, left.value, liveBefore, restingOnShift = false) } ?: home()
        else -> null
    }

    /**
     * Back from [from] only if it's still on screen: the enlarged code goes
     * back by itself once its sale isn't live, which the sales changing may
     * already have done ([TillNav]), and a second Back would leave the sale.
     */
    fun back(from: Screen) {
        if (screenState.value == from) back()
    }

    /** A scanned or pasted code; false when it isn't one. */
    fun onCode(text: String): Boolean {
        val code = ConnectCode.parse(text, allowCleartext = BuildConfig.DEBUG) ?: return false
        go(Screen.Confirm(code))
        return true
    }

    fun connect(code: ConnectCode) = work {
        try {
            val redeemed = graph.monokulo.connect(code)
            // Over Tor when the store has an onion address and Orbot is running (decision 55).
            val connection = redeemed.copy(tor = redeemed.onionApi != null && withContext(Dispatchers.IO) { Tor.running() })
            // A registration left from an earlier store goes before the stores forget its id.
            graph.wakeUps.unregister(forget = true)
            stores.forgetAll()
            stores.connection = connection
            graph.store.forget()
            graph.store.read(connection)
            go(Screen.Connected(connection))
        } catch (e: MonokuloException) {
            problem.value = Problems.say(e)
        }
    }

    /** Step 4 from Settings ("Change the point of sale", acceptance T1.2): back to Settings once done. */
    fun openPointOfSale() {
        fromSettings = true
        go(Screen.PointOfSale)
    }

    /** Back from step 4 opened from Settings. */
    private fun closePointOfSale() {
        fromSettings = false
        go(Screen.Settings)
    }

    /**
     * A point of sale tapped, in step 4 or Settings: saved at once; choosing the one chosen
     * changes nothing; choosing another keeps the shift's sales named by the old point of sale,
     * takes the Bell's registration back when the old one is still signed in, and signs out of
     * the old one whether or not its sign-in had already ended, before the new one's sign-in
     * ([PosSwitch], acceptance D6, decision 138).
     */
    fun choosePointOfSale(pos: PointOfSale) = work {
        val from = stores.pointOfSale
        val changed = PosSwitch.choose(switchSide, from, signedIn = from != null && posSignedIn, to = pos)
        if (!changed) return@work
        stores.withoutPos = false
        tenderState.value = null
        restart()
        // Settings' rows (not signed in) stay in Settings, its sign-in row beneath them; step 4 shows the new card.
        if (screenState.value != Screen.Settings) go(Screen.PointOfSale)
    }

    /** What changing the point of sale touches: the Bell, the old sign-in, the choice. */
    private val switchSide = object : PosSwitchSide {
        override suspend fun takeBellRegistrationBack() = graph.wakeUps.unregister()
        override fun signOut(pos: PointOfSale) = signOutHere(pos)
        override fun forgetCursor(pos: PointOfSale) = stores.forgetCursor(pos)
        override fun choose(pos: PointOfSale) {
            stores.pointOfSale = pos
        }
    }

    /**
     * Forgets the sign-in to [pos] on this device: its tokens, the application they came through
     * when it was a Bell's (decision 146), its location or merchant, and the remembered tender id
     * (acceptance T1.4); a change of point of sale forgets its cursor too.
     */
    private fun signOutHere(pos: PointOfSale) {
        when (pos) {
            PointOfSale.Square -> {
                graph.squareAuth().signOut()
                stores.squareApplicationId = null
                stores.location = null
            }
            PointOfSale.Clover -> {
                graph.cloverAuth().signOut()
                stores.cloverApplicationId = null
                stores.cloverMerchant?.let { stores.tenders.save(it.id, null) }
                stores.cloverMerchant = null
                tenderState.value = null
            }
        }
    }

    /**
     * The chosen point of sale's sign-in page, for this attempt: through [applicationId] when
     * that's a Bell's (Settings › Wake-ups' "Sign in to …", decision 96), else through the
     * application the till signs in through now. The application is kept with the attempt,
     * and becomes the till's only once the sign-in finishes, so the working sign-in stands
     * until then. Null with no point of sale chosen.
     */
    fun signInUrl(pos: PointOfSale? = stores.pointOfSale, applicationId: String? = null): String? {
        val pkce = Pkce.create()
        return when (pos) {
            PointOfSale.Square -> {
                val through = applicationId?.takeIf { it != graph.squareConfig().applicationId }
                stores.pendingSignIn = PendingSignIn(pos, pkce, through)
                squareAuth(through).authorizeUrl(pkce)
            }
            PointOfSale.Clover -> {
                val through = applicationId?.takeIf { it != graph.cloverConfig().applicationId }
                stores.pendingSignIn = PendingSignIn(pos, pkce, through)
                cloverAuth(through).authorizeUrl(pkce)
            }
            null -> null
        }
    }

    /** Square's sign-in through [applicationId], or through the till's own application when null. */
    private fun squareAuth(applicationId: String?): SquareAuth =
        if (applicationId == null) graph.squareAuth() else SquareAuth(graph.http, graph.squareConfig().copy(applicationId = applicationId), stores.tokens)

    /** Clover's sign-in through [applicationId], or through the till's own application when null. */
    private fun cloverAuth(applicationId: String?) =
        if (applicationId == null) graph.cloverAuth() else graph.cloverAuth(graph.cloverConfig().copy(applicationId = applicationId))

    /**
     * A point of sale's redirect back into the app: [pos] is the one whose return address it
     * came to. A redirect with no sign-in of that point of sale pending is dropped (decision 127).
     */
    fun onPosRedirect(pos: PointOfSale, uri: Uri) = work {
        val pending = stores.pendingSignIn?.takeIf { it.pointOfSale == pos } ?: run {
            problem.value = "That ${pos.displayName} sign-in wasn't started here. Try again."
            return@work
        }
        try {
            when (pos) {
                PointOfSale.Square -> finishSquare(pending, uri)
                PointOfSale.Clover -> finishClover(pending, uri)
            }
        } catch (e: PosException) {
            // A failed sign-in after a good one (the locations, the merchant) leaves no Bell sign-in pending.
            bellSignIn = false
            // go() clears the problem, so the point of sale's words come after it.
            go(if (pending.applicationId != null && stores.setupDone) Screen.Settings else Screen.PointOfSale)
            problem.value = e.message
        }
    }

    private suspend fun finishSquare(pending: PendingSignIn, uri: Uri) {
        squareAuth(pending.applicationId)
            .finish(pending.pkce, uri.getQueryParameter("state"), uri.getQueryParameter("code"), uri.getQueryParameter("error"))
        stores.pendingSignIn = null
        stores.pointOfSale = PointOfSale.Square
        if (pending.applicationId != null) {
            // Signed in through the Bell's application: the till's from now on (decision 96).
            stores.squareApplicationId = pending.applicationId
            bellSignIn = true
        }
        val locations = graph.squareApi().locations()
        when {
            locations.size == 1 -> chooseLocation(locations.single())
            else -> {
                // The running till still asks Square as the old application: it starts again as the new one.
                if (pending.applicationId != null) restart()
                go(Screen.Locations(locations))
            }
        }
    }

    /**
     * Clover's redirect: the tokens for the redirect's merchant, the merchant read, then the
     * Monero payment type found or added (decision 129); the merchant is the till's place,
     * with no locations to pick (decision 127).
     */
    private suspend fun finishClover(pending: PendingSignIn, uri: Uri) {
        val tokens = cloverAuth(pending.applicationId).finish(
            pending.pkce,
            merchantId = uri.getQueryParameter("merchant_id"),
            code = uri.getQueryParameter("code"),
            state = uri.getQueryParameter("state"),
            error = uri.getQueryParameter("error"),
        )
        stores.pendingSignIn = null
        if (pending.applicationId != null) {
            stores.cloverApplicationId = pending.applicationId
            bellSignIn = true
        }
        val merchant = graph.cloverApi().merchant(tokens.merchantId)
        stores.cloverMerchant = merchant
        stores.pointOfSale = PointOfSale.Clover
        stores.location = null
        stores.withoutPos = false
        tenderState.value = graph.cloverSource(merchant).ensureTender()
        signedIn()
    }

    /** "Look again" on Clover's card or in Settings: the Monero payment type, found or added now. */
    fun lookAgain() = work {
        val merchant = stores.cloverMerchant ?: return@work
        try {
            tenderState.value = graph.cloverSource(merchant).ensureTender()
            // A sale rung up with the tender just found is the till's from now.
            restart()
        } catch (e: PosException) {
            problem.value = e.message
        }
    }

    /** The Square locations, to change this till's. */
    fun changeLocation() = work {
        try {
            go(Screen.Locations(graph.squareApi().locations()))
        } catch (e: PosException) {
            problem.value = e.message
        }
    }

    fun chooseLocation(location: SquareLocation) {
        stores.location = location
        stores.withoutPos = false
        signedIn()
    }

    /**
     * Signed in at a Square location or a Clover merchant: the Bell rings this till for the place
     * it registers, so it registers again for this one; the till starts watching now, so setup
     * can show the test sale; and the cashier goes back where the sign-in started.
     */
    private fun signedIn() {
        graph.wakeUps.recheck()
        restart()
        val forBell = bellSignIn
        val settings = fromSettings
        bellSignIn = false
        fromSettings = false
        go(
            when {
                (forBell || settings) && stores.setupDone -> Screen.Settings
                stores.setupDone -> Screen.Till
                else -> Screen.PointOfSale
            },
        )
    }

    /** "Not now: only show other tills' sales". */
    fun withoutPos() {
        stores.withoutPos = true
        restart()
        go(if (stores.setupDone) Screen.Till else Screen.Alerts)
    }

    fun setBubble(on: Boolean) = graph.setBubble(on)

    /** Setup's last step: the shift starts, and the till takes over. */
    fun startShift() {
        stores.setupDone = true
        setShift(true)
        go(Screen.Till)
    }

    /**
     * Watching on or off. Turning it off brings every live code down;
     * the shift's sales stay listed. Turning it on after it was off
     * starts a new shift: "since" is now and the last one's sales go,
     * but for those that still need the cashier (decisions 64, 86).
     * The first shift starts when the till first runs, in setup.
     */
    fun setShift(on: Boolean) {
        if (on && !stores.shiftOn) {
            stores.shift.save(ShiftList.carried(stores.shift.load()))
            stores.shiftStartedAt = System.currentTimeMillis() / 1000
        }
        if (!on && stores.shiftOn) graph.runner.closeLive()
        stores.shiftOn = on
        if (on) TillService.start(getApplication()) else TillService.stop(getApplication())
    }

    /** Starts (or restarts) the till's service when a shift is on. */
    fun restart() {
        TillService.stop(getApplication())
        if (stores.shiftOn && stores.connection != null) TillService.start(getApplication())
    }

    /** Starts the till's service if a shift is on and it isn't running (the app was reopened). */
    fun restartIfStopped() {
        if (!graph.runner.state.value.running && stores.shiftOn && stores.connection != null) {
            TillService.start(getApplication())
        }
    }

    /** Reach the store over Tor, at its onion address, or not (decision 55). */
    fun setTor(on: Boolean) {
        val connection = stores.connection ?: return
        val onion = connection.onionApi ?: graph.store.now.value.onionApi ?: return
        stores.connection = connection.copy(onionApi = onion, tor = on)
        graph.store.read(stores.connection)
        restart()
        go(Screen.Settings)
    }

    /** Whether Orbot's proxy answers: Settings says so beside the Tor switch. */
    suspend fun torRunning(): Boolean = withContext(Dispatchers.IO) { Tor.running() }

    /**
     * Signs out of the point of sale. The Bell's registration is taken back
     * first, awaited, while what it needs is still here; the Wake-ups choice
     * and the point of sale chosen stay, so signing in again registers again
     * (acceptance A10.2).
     */
    fun signOutOfPos() = work {
        val pos = stores.pointOfSale ?: return@work
        PosSwitch.signOut(switchSide, pos)
        restart()
        go(Screen.PointOfSale)
    }

    fun disconnect() = work {
        val connection = stores.connection
        // The Bell forgets this till before the store does (acceptance A10.3).
        graph.wakeUps.unregister(forget = true)
        if (connection != null) {
            // The store may already have disconnected it: forget it either way.
            runCatching { graph.monokulo.disconnect(connection) }
        }
        forgetHere()
    }

    /** The store disconnected this device: forget it and start again, the Bell's registration taken back first. */
    fun forget() = work {
        graph.wakeUps.unregister(forget = true)
        forgetHere()
    }

    /** Forgets the store and everything about this till, once nothing outside it needs what's forgotten. */
    private fun forgetHere() {
        TillService.stop(getApplication())
        graph.squareAuth().signOut()
        graph.cloverAuth().signOut()
        tenderState.value = null
        refundSheet.value = null
        stores.forgetAll()
        graph.store.forget()
        left.value = emptySet()
        beforeSettings = null
        bellSignIn = false
        go(Screen.Welcome)
    }

    /** Settings › Wake-ups' choice. */
    fun chooseWakeUps(choice: WakeUpChoice) = graph.wakeUps.choose(choice)

    /** "Use this Bell": the address as typed, which Settings only offers once it parses. */
    fun useOwnBell(text: String) {
        val bell = BellAddress.parse(text, allowHttp = BuildConfig.DEBUG) ?: return
        graph.wakeUps.choose(WakeUpChoice.OwnBell(bell))
    }

    /** The UnifiedPush app to deliver wake-ups through, picked from several. */
    fun chooseDistributor(packageName: String) = graph.wakeUps.chooseDistributor(packageName)

    /**
     * Refunds a sale's point-of-sale sale ([close] it first: the close sheet's "No"); its button
     * waits for the point of sale's answer. A sale the till leaves to the cashier (`Till.refund`:
     * Clover, decision 130) is marked to refund there, and the refund sheet opens once it stands
     * so, whichever point of sale took it.
     */
    fun refund(orderId: String, close: Boolean = false) {
        if (orderId in refunding.value) return
        val sale = graph.runner.state.value.till.sales.firstOrNull { it.orderId == orderId } ?: return
        val found = sale.found ?: return
        // Rung up at a point of sale this till has changed from: refunded there, and only a sale
        // already told so opens the sheet, for the cashier's word (decisions 138, 141).
        if (found.pointOfSale != stores.pointOfSale && sale.standing != Standing.RefundInPos) return
        val job = graph.runner.refund(orderId, close) ?: return
        refunding.update { it + orderId }
        job.invokeOnCompletion {
            refunding.update { it - orderId }
            if (graph.runner.standing(orderId) == Standing.RefundInPos) {
                val pos = found.pointOfSale
                val source = graph.runner.state.value.source
                refundSheet.value = RefundSheet(orderId, pos, asks = pos == source && Closing.refundsThere(pos, source))
            }
        }
    }

    /** The refund sheet's "I've refunded it": the point of sale asked now; the sheet goes once it shows the refund. */
    fun refundDone(orderId: String) {
        val sheet = refundSheet.value?.takeIf { it.orderId == orderId && !it.checking } ?: return
        refundSheet.value = sheet.copy(checking = true, notSeen = false)
        viewModelScope.launch {
            val seen = graph.runner.confirmRefund(orderId)
            refundSheet.update { current ->
                when {
                    current?.orderId != orderId -> current
                    seen -> null
                    else -> current.copy(checking = false, notSeen = true)
                }
            }
        }
    }

    /** "Not yet", Back or the scrim: the sale stays pending, under Needs action. */
    fun refundSheetDismissed() {
        refundSheet.value = null
    }

    /** The close sheet's answer for the sale on screen ([Closing]). */
    fun closeSale(orderId: String, choice: CloseChoice) {
        if (!Closing.applies(graph.runner.state.value.till.sales.firstOrNull { it.orderId == orderId })) return
        val outcome = Closing.outcome(choice)
        when {
            outcome.refund -> refund(orderId, close = outcome.close)
            outcome.close -> graph.runner.close(orderId)
        }
        if (outcome.then == CloseOutcome.Then.Shift) leave(orderId)
    }

    /** "Show the code again" on a closed sale: its code is back up, on screen and on the bubble. */
    fun reopen(orderId: String) {
        graph.runner.reopen(orderId)
        open(orderId)
    }

    /**
     * The cashier left a sale's full screen for the shift (it stays live, on
     * the bubble and in the shift's tray), or a paid one stepped back itself:
     * the one way Back and the dwell both go. It always lands on the shift;
     * a sale waiting meanwhile is in the tray, not brought up (decision 266).
     */
    fun leave(orderId: String) {
        // Left by the cashier during a paid moment, the step back has nothing left to do.
        stepBack?.cancel()
        left.update { it + orderId }
        if (screenState.value.orderIdOrNull() == orderId) go(Screen.Till)
    }

    /** Shows a sale full screen (from the shift, a notification or the bubble's panel): its code, or its page. */
    fun open(orderId: String) {
        left.update { it - orderId }
        stepBack?.cancel()
        if (stores.setupDone) go(Screen.Sale(orderId))
    }

    /** The enlarged code, from the sale's code. */
    fun enlarge(orderId: String) = go(Screen.Code(orderId))

    /** Every line of the sale's order, from its order card. */
    fun showOrder(orderId: String) = go(Screen.Order(orderId))

    /** The shift (from the panel's "Open the shift"): every sale's full screen is left for now. */
    fun openShift() {
        left.update { it + graph.runner.state.value.till.sales.map { s -> s.orderId } }
        if (stores.setupDone) go(Screen.Till)
    }

    fun wake() = graph.runner.wake()

    fun setVisible(visible: Boolean) {
        graph.appVisible.value = visible
    }

    /** Back from Android's settings or a permission prompt: let the service look again. */
    fun permissionsMayHaveChanged() {
        graph.permissions.value++
    }

    private fun work(block: suspend () -> Unit) {
        if (busy.value) return
        busy.value = true
        problem.value = null
        viewModelScope.launch {
            try {
                block()
            } catch (e: Exception) {
                problem.value = e.message ?: "Something went wrong."
            } finally {
                busy.value = false
            }
        }
    }
}
