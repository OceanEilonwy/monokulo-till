package com.monokulo.till

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.CompositionLocalProvider
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView
import androidx.core.net.toUri
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import com.monokulo.till.ui.confirm
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.notify.SaleNotifications
import com.monokulo.till.push.WakeUpStatus
import com.monokulo.till.sales.TenderState
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.AlertsScreen
import com.monokulo.till.ui.ConfirmScreen
import com.monokulo.till.ui.ConnectedScreen
import com.monokulo.till.ui.FloatScreen
import com.monokulo.till.ui.LocationsScreen
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.ReadyScreen
import com.monokulo.till.ui.RefundSheet
import com.monokulo.till.ui.ScanScreen
import com.monokulo.till.ui.SettingsActions
import com.monokulo.till.ui.SettingsScreen
import com.monokulo.till.ui.SettingsState
import com.monokulo.till.ui.LocalPointOfSale
import com.monokulo.till.ui.LocalReceiptFolds
import com.monokulo.till.ui.PointOfSaleScreen
import com.monokulo.till.ui.PosSetup
import com.monokulo.till.ui.PosSetupActions
import com.monokulo.till.ui.PosState
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ShiftPlace
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.rememberShiftPlace
import com.monokulo.till.ui.TillActions
import com.monokulo.till.ui.TillStatus
import com.monokulo.till.ui.WakeUpsState
import com.monokulo.till.ui.WelcomeScreen

class MainActivity : ComponentActivity() {
    private val model: AppViewModel by viewModels()

    /** Bumped on every resume: permissions granted in Android's settings show at once. */
    private val resumed = mutableIntStateOf(0)

    private val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        resumed.intValue++
        model.permissionsMayHaveChanged()
        if (model.screen.value == Screen.Alerts) model.go(Screen.Float)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Only a fresh launch acts on its intent: a rotation or theme change recreates the activity with the same
        // intent, and replaying it would reopen a sale, the shift or onboarding over whatever the cashier was doing.
        // Reopening from Recents replays the original intent too, so that is skipped as well.
        if (savedInstanceState == null && intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY == 0) handle(intent)
        setContent {
            TillTheme {
                val screenState = model.screen.collectAsStateWithLifecycle()
                val screen by screenState
                // Held as states and read only by the screens that need them (behind derivedStateOf), so a runner
                // update that changes nothing a screen shows recomposes nothing here (performance rule 10).
                val flows = Flows(
                    busy = model.busy.collectAsStateWithLifecycle(),
                    problem = model.problem.collectAsStateWithLifecycle(),
                    runner = model.runner.collectAsStateWithLifecycle(),
                    refunding = model.refunding.collectAsStateWithLifecycle(),
                    bubbleWanted = model.bubbleWanted.collectAsStateWithLifecycle(),
                    store = model.store.collectAsStateWithLifecycle(),
                    tenderState = model.tenderState.collectAsStateWithLifecycle(),
                    refundSheet = model.refundSheet.collectAsStateWithLifecycle(),
                    screen = screenState,
                )
                // Read so that coming back from Android's settings recomposes the setup screens that ask it.
                val tick = resumed.intValue
                // Re-read on every bump of `resumed`, which onMultiWindowModeChanged makes, so the till's actions follow it.
                val multiWindow = isInMultiWindowMode
                val pos = model.pointOfSale
                val runnerState = flows.runner
                val source by remember { derivedStateOf { runnerState.value.source } }
                val actions = remember(multiWindow, pos, source) { tillActions(multiWindow, pos, source) }
                PaidBuzz()
                val stage = remember { SeekableTransitionState(screen) }
                // Android's back (decision 77: everywhere but the first step, the shift and Square's sign-in). From a
                // sale's screens and Settings the gesture previews the way back; nothing changes before it commits.
                val scrub = remember { BackScrub() }
                // Step 4 has no Back in setup (as Square's had none), but goes back to Settings when opened from there.
                val stepFour = screen == Screen.PointOfSale && !model.setupDone
                FollowScreen(stage, scrub, screenState, onLanded = model::landed)
                // A second Back while the shift is still arriving lands on the shift, never leaves the app (decision 260 c):
                // Back stays the stage's until it rests there. Read as a derived state, so the root recomposes as a change
                // starts and as it lands, never a frame.
                val arriving by remember { derivedStateOf { stage.currentState != stage.targetState } }
                StageBack(
                    enabled = (screen != Screen.Welcome && screen != Screen.Till && !stepFour) || (screen == Screen.Till && arriving),
                    state = stage, scrub = scrub, current = { model.screen.value }, target = model::backTarget, onCommit = model::back,
                )
                // Where the shift's list was when a row was tapped: the shift is not composed while a sale is up, so a
                // Back finds it here (decision 296).
                val shiftPlace = rememberShiftPlace()
                CompositionLocalProvider(LocalPointOfSale provides pos, LocalReceiptFolds provides model.receiptFolds) {
                    TillStage(stage, cameUpByItself = { model.cameUpByItself }, scrub) { s ->
                        when (s) {
                            Screen.Till, is Screen.Sale, is Screen.Code, is Screen.Order -> TillScreens(s, flows, actions, shiftPlace)
                            else -> SetupScreens(s, flows, tick)
                        }
                    }
                }
            }
        }
    }

    /** One confirming buzz when a payment lands with the app on screen (`docs/motion.md` §6); collected only while it is. */
    @Composable
    private fun PaidBuzz() {
        val view = LocalView.current
        val lifecycle = LocalLifecycleOwner.current.lifecycle
        LaunchedEffect(view, lifecycle) {
            lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { model.paidBuzz.collect { view.confirm() } }
        }
    }

    /** The view model's flows as states, collected once at the root and read below it. */
    @Stable
    private class Flows(
        val busy: State<Boolean>,
        val problem: State<String?>,
        val runner: State<RunnerState>,
        val refunding: State<Set<String>>,
        val bubbleWanted: State<Boolean>,
        val store: State<StoreNow>,
        val tenderState: State<TenderState?>,
        val refundSheet: State<RefundSheet?>,
        /** The model's screen: the one up, or being gone to. */
        val screen: State<Screen>,
    )

    /**
     * The shift and a sale's screens. Each reads only its own part of the
     * runner, so a poll that changes nothing it shows recomposes nothing.
     */
    @Composable
    private fun TillScreens(s: Screen, flows: Flows, actions: TillActions, shiftPlace: ShiftPlace) {
        val id = s.orderIdOrNull()
        val runner = flows.runner
        val shown by remember(id) { derivedStateOf { id?.let { id -> runner.value.till.sales.firstOrNull { it.orderId == id } } } }
        val disconnected by remember { derivedStateOf { runner.value.disconnected } }
        // Being left, a screen keeps the sale as it was when the change began: closed, paid or gone under it, it
        // still leaves with what it showed (decision 242).
        val sale = keptWhileLeaving(s, flows.screen) { shown }.value
        when {
            // A sale not in the list (yet: the till is still loading its shift) shows the shift until it is;
            // a disconnected till shows why, whatever was on screen.
            sale == null || disconnected -> Shift(flows, actions, shiftPlace)
            s is Screen.Code -> CodeScreen(sale, onBack = { model.back(s) })
            s is Screen.Order -> OrderPage(sale, "Back to the sale", onBack = { model.back(s) }, onShowCode = { model.back(s) })
            else -> Sale(sale, flows, actions)
        }
    }

    @Composable
    private fun Shift(flows: Flows, actions: TillActions, place: ShiftPlace) {
        val runner = flows.runner
        val store = flows.store
        val status by remember { derivedStateOf { tillStatus(runner.value, store.value) } }
        val sales by remember { derivedStateOf { runner.value.till.sales } }
        ShiftScreen(status, sales, actions, place = place)
    }

    @Composable
    private fun Sale(sale: TillSale, flows: Flows, actions: TillActions) {
        val id = sale.orderId
        val runner = flows.runner
        val refundingSet = flows.refunding
        val refunding by remember(id) { derivedStateOf { id in refundingSet.value } }
        val refundProblem by remember(id) { derivedStateOf { runner.value.till.refundProblems[id] } }
        val refundSheet by flows.refundSheet
        SaleScreen(sale, model.placeName, refunding, refundProblem, refundSheet, actions)
    }

    /** Onboarding and Settings: few, and never on screen while a sale is, so they read the flows whole. */
    @Composable
    private fun SetupScreens(s: Screen, flows: Flows, @Suppress("UNUSED_PARAMETER") resumed: Int) {
        val busy by flows.busy
        val problem by flows.problem
        val runner by flows.runner
        val bubbleWanted by flows.bubbleWanted
        val store by flows.store
        val tenderState by flows.tenderState
        when (s) {
            Screen.Welcome -> WelcomeScreen(onStart = { model.go(Screen.Scan) })
            Screen.Scan -> ScanScreen(onCode = model::onCode, onBack = model::back)
            is Screen.Confirm -> ConfirmScreen(s.code, model.deviceName, busy, problem, { model.connect(s.code) }, model::back)
            is Screen.Connected -> ConnectedScreen(
                s.connection, store,
                onNext = { model.go(Screen.PointOfSale) }, onRetry = model::readStore, onWrong = model::disconnect,
            )
            Screen.PointOfSale -> PointOfSaleScreen(
                PosSetup(
                    chosen = model.pointOfSale,
                    location = model.location,
                    merchant = model.cloverMerchant?.name,
                    signedIn = model.posSignedIn,
                    tender = tenderState.forScreen(),
                    busy = busy,
                    problem = problem ?: runner.till.problem,
                    testSale = runner.till.sales.firstOrNull { it.found != null },
                    setup = !model.setupDone,
                ),
                PosSetupActions(
                    onChoose = model::choosePointOfSale,
                    onSignIn = ::signIn,
                    onChangeLocation = model::changeLocation,
                    onLookAgain = model::lookAgain,
                    onSkip = model::withoutPos,
                    onContinue = {
                        if (model.setupDone) {
                            model.back()
                        } else {
                            // The test sale has done its job: the till opens on its idle screen, the sale in its list.
                            flows.runner.value.till.sales.forEach { model.leave(it.orderId) }
                            model.go(Screen.Alerts)
                        }
                    },
                ),
            )
            is Screen.Locations -> LocationsScreen(s.locations, model::chooseLocation, model::back)
            Screen.Alerts -> AlertsScreen(
                place = model.placeName,
                currency = model.location?.currency ?: store.store?.baseCurrency ?: "USD",
                allowed = model.notificationsAllowed(),
                onAllow = ::allowNotifications,
                onSkip = { model.go(Screen.Float) },
                onBack = model::back,
            )
            Screen.Float -> FloatScreen(
                pos = model.pointOfSale,
                allowed = model.overlayAllowed(),
                onOpenSettings = { model.setBubble(true); openOverlaySettings() },
                onNotificationsOnly = { model.setBubble(false); model.go(Screen.Ready) },
                onContinue = { model.setBubble(true); model.go(Screen.Ready) },
                onBack = model::back,
            )
            Screen.Ready -> ReadyScreen(
                pos = model.pointOfSale,
                store = store.store?.name.orEmpty(),
                location = model.placeName,
                alerts = model.notificationsAllowed(),
                bubble = bubbleWanted && model.overlayAllowed(),
                onStart = model::startShift,
            )
            Screen.Settings -> {
                val torRunning by produceState<Boolean?>(null, store) { value = model.torRunning() }
                val wakeUpChoice by model.wakeUpChoice.collectAsStateWithLifecycle()
                val wakeUpStatus by model.wakeUpStatus.collectAsStateWithLifecycle()
                // PackageManager is asked for the UnifiedPush apps' names only when the status changes.
                val distributorNames = remember(wakeUpStatus) { distributorNames(wakeUpStatus) }
                SettingsScreen(
                    SettingsState(
                        connection = model.connection,
                        store = store,
                        pos = PosState(
                            chosen = model.pointOfSale,
                            signedIn = model.posSignedIn,
                            location = model.location,
                            merchant = model.cloverMerchant?.name,
                            tenderName = model.tenderName,
                            tender = tenderState.forScreen(),
                        ),
                        live = runner.live,
                        shiftOn = model.shiftOn,
                        alerts = model.notificationsAllowed(),
                        bubbleWanted = bubbleWanted,
                        bubbleAllowed = model.overlayAllowed(),
                        debug = model.debug,
                        baseUrls = model.baseUrls,
                        torRunning = torRunning,
                        wakeUps = WakeUpsState(
                            choice = wakeUpChoice,
                            status = wakeUpStatus,
                            monokulo = model.monokuloBell,
                            ownBell = model.ownBell,
                            distributorNames = distributorNames,
                            allowHttp = BuildConfig.DEBUG,
                        ),
                        problem = problem,
                    ),
                    SettingsActions(
                        onShift = { model.setShift(it); model.go(Screen.Settings) },
                        onAlerts = ::openNotificationSettings,
                        onBubble = { on ->
                            model.setBubble(on)
                            if (on && !model.overlayAllowed()) openOverlaySettings()
                        },
                        onChoosePos = model::choosePointOfSale,
                        onSignIn = ::signIn,
                        onChangeLocation = model::changeLocation,
                        onSignOut = model::signOutOfPos,
                        onChangePos = model::openPointOfSale,
                        onLookAgain = model::lookAgain,
                        onDisconnect = model::disconnect,
                        onBaseUrl = model::setBaseUrl,
                        onCopyAddress = { SaleNotifications.copyAddress(this, it) },
                        onTor = model::setTor,
                        onWakeUps = model::chooseWakeUps,
                        onOwnBell = model::useOwnBell,
                        onDistributor = model::chooseDistributor,
                        onGetDistributor = { openInTab(NTFY) },
                        onBellSignIn = { pos, applicationId -> model.signInUrl(pos, applicationId)?.let(::openInTab) },
                        onBack = model::closeSettings,
                    ),
                )
            }
            Screen.Till, is Screen.Sale, is Screen.Code, is Screen.Order -> Unit
        }
    }

    private fun tillStatus(runner: RunnerState, store: StoreNow) = TillStatus(
        store = store.store?.name,
        location = model.placeName,
        running = runner.running,
        live = runner.live,
        disconnected = runner.disconnected,
        problem = runner.till.problem ?: runner.streamProblem,
        signInAgain = runner.till.signInAgain,
        startedAt = model.shiftStartedAt,
    )

    private fun tillActions(multiWindow: Boolean, pos: PointOfSale?, source: PointOfSale) = TillActions(
        onSettings = model::openSettings,
        onOpen = model::open,
        onBack = model::leave,
        onEnlarge = model::enlarge,
        onShowOrder = model::showOrder,
        onClose = model::closeSale,
        onRefund = { model.refund(it) },
        onReopen = model::reopen,
        onCopy = { SaleNotifications.copyAddress(this, it.address) },
        onCopyXmr = { SaleNotifications.copyXmr(this, it.xmr) },
        // Square's app is on this phone; a Clover point of sale is a device of its own (decision 135).
        onBackToPos = if (multiWindow || pos != PointOfSale.Square) null else ::backToSquare,
        onRefundDone = model::refundDone,
        onRefundSheetDismissed = model::refundSheetDismissed,
        onSignIn = ::signIn,
        onForget = model::forget,
        onStartShift = { model.setShift(true) },
        source = source,
    )

    override fun onStart() {
        super.onStart()
        model.setVisible(true)
        if (model.connection != null && model.shiftOn) model.restartIfStopped()
        model.wake()
        model.retryWakeUps()
    }

    override fun onResume() {
        super.onResume()
        resumed.intValue++
        model.permissionsMayHaveChanged()
    }

    override fun onMultiWindowModeChanged(isInMultiWindowMode: Boolean, newConfig: android.content.res.Configuration) {
        super.onMultiWindowModeChanged(isInMultiWindowMode, newConfig)
        resumed.intValue++
    }

    override fun onStop() {
        model.setVisible(false)
        super.onStop()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handle(intent)
    }

    private fun handle(intent: Intent?) {
        intent?.getStringExtra(EXTRA_ORDER)?.let(model::open)
        if (intent?.getBooleanExtra(EXTRA_SHIFT, false) == true) model.openShift()
        when (intent?.action) {
            Intent.ACTION_VIEW -> {
                val uri = intent.data ?: return
                posRedirect(uri)?.let { model.onPosRedirect(it, uri) }
            }
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.let { text ->
                text.split(Regex("\\s+")).firstOrNull { model.onCode(it) }
            }
        }
    }

    /** The point of sale whose sign-in [uri] returns from, by its return address; null for anything else. */
    private fun posRedirect(uri: Uri): PointOfSale? = PointOfSale.entries.firstOrNull { pos ->
        val expected = when (pos) {
            PointOfSale.Square -> BuildConfig.SQUARE_RETURN_URI
            PointOfSale.Clover -> BuildConfig.CLOVER_RETURN_URI
        }.toUri()
        uri.scheme == expected.scheme && uri.host == expected.host && uri.path.orEmpty() == expected.path.orEmpty()
    }

    /** The chosen point of sale's sign-in, in a Custom Tab. */
    private fun signIn() {
        model.signInUrl()?.let(::openInTab)
    }

    private fun openInTab(url: String) {
        CustomTabsIntent.Builder().build().launchUrl(this, url.toUri())
    }

    /** The UnifiedPush apps' names, for the rows to pick one from; a package Android can't name shows as itself. */
    private fun distributorNames(status: WakeUpStatus): Map<String, String> {
        if (status !is WakeUpStatus.ChooseDistributor) return emptyMap()
        return status.packages.associateWith { pkg ->
            runCatching { packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString() }.getOrDefault(pkg)
        }
    }

    private fun allowNotifications() {
        if (Build.VERSION.SDK_INT >= 33 && !model.notificationsAllowed()) {
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            model.go(Screen.Float)
        }
    }

    private fun openOverlaySettings() {
        runCatching { startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, "package:$packageName".toUri())) }
    }

    private fun openNotificationSettings() {
        runCatching {
            startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, packageName))
        }
    }

    /** Square Point of Sale, if it's installed; else Till steps back to whatever was behind it. */
    private fun backToSquare() {
        val square = packageManager.getLaunchIntentForPackage(SQUARE_POS)
        if (square != null) startActivity(square) else moveTaskToBack(true)
    }

    companion object {
        /** A sale to show full screen: its code, or its page. */
        const val EXTRA_ORDER = "com.monokulo.till.ORDER"

        /** Open on the shift (the panel's "Open the shift"), whatever sale is live. */
        const val EXTRA_SHIFT = "com.monokulo.till.SHIFT"

        /** Where to get ntfy, a UnifiedPush app that carries the Bell's wake-ups (decision 91). */
        const val NTFY = "https://ntfy.sh/"

        /** Square Point of Sale's package. */
        const val SQUARE_POS = "com.squareup"
    }
}
