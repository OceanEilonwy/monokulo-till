package com.monokulo.till

import uniffi.till_app.Screen
import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Surface
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.PrimaryButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.core.app.NotificationManagerCompat
import androidx.core.net.toUri
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.monokulo.till.engine.CloseSheet
import com.monokulo.till.engine.EngineStore
import com.monokulo.till.notify.SaleNotifications
import com.monokulo.till.engine.Nav
import com.monokulo.till.engine.Work
import com.monokulo.till.ui.AlertsScreen
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.ConfirmScreen
import com.monokulo.till.ui.ConnectedScreen
import com.monokulo.till.ui.CounterScreen
import com.monokulo.till.ui.FloatScreen
import com.monokulo.till.ui.LocalPointOfSale
import com.monokulo.till.ui.LocalReceiptFolds
import com.monokulo.till.ui.LocationsScreen
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.PointOfSaleScreen
import com.monokulo.till.ui.PosSetupActions
import com.monokulo.till.ui.ReadyScreen
import com.monokulo.till.ui.ShowSalesScreen
import com.monokulo.till.ui.SaleFace
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ScanScreen
import com.monokulo.till.ui.SettingsActions
import com.monokulo.till.ui.SettingsScreen
import com.monokulo.till.ui.ShiftPlace
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.TillActions
import com.monokulo.till.ui.WorkBar
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.WelcomeScreen
import com.monokulo.till.ui.FobConnectedScreen
import com.monokulo.till.ui.FobRequestScreen
import com.monokulo.till.ui.PaymentsModeScreen
import com.monokulo.till.ui.StepperActions
import com.monokulo.till.ui.LightServerScreen
import com.monokulo.till.ui.ThisTillActions
import com.monokulo.till.ui.ThisTillScreen
import com.monokulo.till.ui.PricesScreen
import com.monokulo.till.ui.KeypadScreen
import uniffi.till_screens.KeypadView
import com.monokulo.till.ui.WalletCodeScreen
import com.monokulo.till.ui.WalletEntryScreen
import com.monokulo.till.ui.WalletKeysScreen
import com.monokulo.till.ui.WalletSeedScreen
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import uniffi.till_screens.WalletWay
import com.monokulo.till.ui.confirm
import com.monokulo.till.ui.mk
import com.monokulo.till.ui.pos
import com.monokulo.till.ui.rememberShiftPlace
import kotlinx.coroutines.launch
import uniffi.till_app.Action
import uniffi.till_app.Effect
import uniffi.till_app.RefundSheetView
import uniffi.till_app.Update
import uniffi.till_screens.OnboardingPage
import uniffi.till_screens.OnboardingView
import uniffi.till_screens.SettingsView
import uniffi.till_screens.ShiftView
import uniffi.till_screens.PosMark

/**
 * The app's one activity: it draws what the engine publishes (through
 * [AppViewModel]'s [com.monokulo.till.engine.EngineStore]) on the stage of
 * [Nav.kt][TillStage], follows the engine's screen, tells it when the stage
 * comes to rest ([Action.Landed], decision 55) and sends Back and every tap
 * as an [Action]. It does what only Android can: the browser for a sign-in,
 * the clipboard, the buzz, the permissions, the camera.
 */
class MainActivity : ComponentActivity() {
    private val model: AppViewModel by viewModels()

    /** Bumped on every resume and window-mode change: the till's actions follow split screen. */
    private val resumed = mutableIntStateOf(0)

    private val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        permissionsMayHaveChanged()
        if (model.store.screen.value == Screen.Alerts) send(Action.Next)
    }

    private fun send(action: Action) = model.send(action)

    /** Fob on this device, answering C1's request (Fob's decision 423): its result's data is the answer. */
    private val fob = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        result.data?.dataString?.let { send(Action.FobAnswer(it)) }
    }

    /** Fob's package on this device, the release before a debug build, when one is installed and trusted. */
    private fun fobPackage(): String? = FobApp.packages.firstOrNull { FobApp.trusted(this, it) }

    /** Opens Fob with the request, for a result (decision 318). */
    private fun openFob(request: String) {
        val pkg = fobPackage() ?: return
        runCatching { fob.launch(Intent(FobApp.ACTION_CONNECT, request.toUri()).setPackage(pkg)) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Only a fresh launch acts on its intent: a rotation or theme change recreates the activity with the same
        // intent, and replaying it would reopen a sale, the shift or onboarding over whatever the cashier was doing.
        // Reopening from Recents replays the original intent too, so that is skipped as well.
        if (savedInstanceState == null && intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY == 0) handle(intent)
        // What only the platform can do, while the activity is on screen.
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { model.store.effects.collect(::effect) }
        }
        setContent {
            TillTheme {
                val store = model.store
                val screenState = store.screen.collectAsStateWithLifecycle()
                // Held as states and read only by the screens that need them, so an update that changes nothing a
                // screen shows recomposes nothing here (performance rule 10).
                val flows = Flows(
                    screen = screenState,
                    nav = store.nav.collectAsStateWithLifecycle(),
                    onboarding = store.onboarding.collectAsStateWithLifecycle(),
                    settings = store.settings.collectAsStateWithLifecycle(),
                    shift = store.shift.collectAsStateWithLifecycle(),
                    keypad = store.keypad.collectAsStateWithLifecycle(),
                    sale = store.sale.collectAsStateWithLifecycle(),
                    close = store.close.collectAsStateWithLifecycle(),
                    refund = store.refund.collectAsStateWithLifecycle(),
                )
                // The engine says where the app is as it starts; until then, the ground.
                val work = store.work.collectAsStateWithLifecycle()
                Box(Modifier.fillMaxSize()) {
                    val first = screenState.value
                    if (first == null) Box(Modifier.fillMaxSize().background(mk.ground)) else Stage(first, flows)
                    // What the engine is busy with and what went wrong, where no screen's view says it (core.md 109).
                    WorkOverlay(work)
                }
            }
        }
    }

    @Composable
    private fun Stage(first: Screen, flows: Flows) {
        val screenState = remember(flows) { derivedStateOf { flows.screen.value ?: first } }
        val screen by screenState
        // Re-read on every bump of `resumed`, which onMultiWindowModeChanged makes, so the till's actions follow it.
        @Suppress("UNUSED_VARIABLE") val tick = resumed.intValue
        val multiWindow = isInMultiWindowMode
        val actions = remember(multiWindow) { TillActions(::send, onBackToPos = if (multiWindow) null else ::backToPos) }
        // A keypad till on a wide window (the expanded width class) shows the sales and the keypad side by side
        // (core.md decision 257): the shift, the keypad and a sale's screens are then all one place on the stage,
        // Screen.Till, which draws the two panes and follows the engine's screen in the right one itself.
        val wide = LocalConfiguration.current.screenWidthDp >= SPLIT_WIDTH_DP
        val splitState = remember(flows, wide) { derivedStateOf { wide && flows.shift.value?.newSale != null } }
        val stageScreen = remember(screenState, splitState) { derivedStateOf { screenState.value.onStage(splitState.value) } }
        val stage = remember { SeekableTransitionState(first.onStage(splitState.value)) }
        // Android's back where the engine says it is the app's (decision 77; docs/decisions/core.md 98). From a sale's
        // screens and Settings the gesture previews the way back the engine names; nothing changes before it commits.
        val scrub = remember { BackScrub() }
        val backEnabled by remember { derivedStateOf { flows.nav.value.backEnabled } }
        FollowScreen(stage, scrub, stageScreen)
        // The one landing report (decision 269), for the engine's own screen: the stage at rest where that screen is
        // drawn, once each time the screen changes, also between the panes of the wide layout, where the stage stays.
        LaunchedEffect(stage, stageScreen) {
            snapshotFlow {
                val at = screenState.value
                at.takeIf { stage.currentState == stage.targetState && stage.currentState == stageScreen.value }
            }.collect { at -> at?.let { send(Action.Landed(it)) } }
        }
        // A second Back while the shift is still arriving lands on the shift, never leaves the app (decision 260 c):
        // Back stays the stage's until it rests there. Read as a derived state, so the root recomposes as a change
        // starts and as it lands, never a frame.
        val arriving by remember { derivedStateOf { stage.currentState != stage.targetState } }
        StageBack(
            enabled = backEnabled || (screen == Screen.Till && arriving),
            // Between the wide layout's panes there's nothing on the stage to preview: Back just goes.
            state = stage, scrub = scrub, current = { stageScreen.value },
            target = { flows.nav.value.backTarget?.onStage(splitState.value)?.takeIf { it != stageScreen.value } },
            onCommit = { send(Action.Back) },
        )
        // Where the shift's list was when a row was tapped: the shift is not composed while a sale is up, so a
        // Back finds it here (decision 296).
        val shiftPlace = rememberShiftPlace()
        // The till's point of sale names its marks: the shift's top row carries it (docs/decisions/core.md 104).
        val pos by remember { derivedStateOf { flows.shift.value?.top?.mark } }
        CompositionLocalProvider(LocalPointOfSale provides pos, LocalReceiptFolds provides model.receiptFolds) {
            TillStage(stage, cameUpByItself = { flows.nav.value.byItself }, scrub, way = { flows.nav.value.way }) { s ->
                when (s) {
                    Screen.Till -> if (splitState.value) SplitTill(flows, actions, shiftPlace) else TillScreens(s, flows, actions, shiftPlace)
                    is Screen.Sale, is Screen.Code, is Screen.Order -> TillScreens(s, flows, actions, shiftPlace)
                    Screen.Keypad -> flows.keypad.value?.let { KeypadScreen(it, send = ::send) }
                    else -> SetupScreens(s, flows)
                }
            }
        }
    }

    /** The engine's flows as states, collected once at the root and read below it. */
    @Stable
    private class Flows(
        val screen: State<Screen?>,
        val nav: State<Nav>,
        val onboarding: State<OnboardingView?>,
        val settings: State<SettingsView?>,
        val shift: State<ShiftView?>,
        val keypad: State<KeypadView?>,
        val sale: State<Update.Sale?>,
        val close: State<CloseSheet>,
        val refund: State<RefundSheetView?>,
    )

    /**
     * The shift and a sale's screens. A sale's screens draw the engine's
     * [Update.Sale] for their own sale; being left, a screen keeps the one it
     * had when the change began, so it leaves with what it showed (decision
     * 242). The sheets are drawn only by the screen the stage goes to.
     */
    @Composable
    private fun TillScreens(s: Screen, flows: Flows, actions: TillActions, shiftPlace: ShiftPlace) {
        val id = s.orderIdOrNull()
        val kept = remember(id) { Kept() }
        val face = flows.sale.value?.takeIf { it.orderId == id }?.let { update -> kept.face(update) } ?: kept.last
        val target = flows.screen.value == s
        val close = flows.close.value.takeIf { target }
        val shift = flows.shift.value
        // A sale charged on the keypad comes straight up in the keypad's place: the shift is never shown between them,
        // even for the frame or two before the sale's own update is in. Read once, as the screen is first composed.
        val stage = LocalStage.current
        val charged = remember(id) { id != null && stage?.from() == Screen.Keypad }
        when {
            face == null && charged -> Box(Modifier.fillMaxSize().background(mk.ground))
            // A sale not drawn (yet) shows the shift until it is.
            face == null -> if (shift != null) ShiftScreen(shift, actions, shiftPlace, close?.takeIf { s == Screen.Till })
            s is Screen.Code -> CodeScreen(face) { send(Action.Back) }
            s is Screen.Order -> OrderPage(face, onBack = { send(Action.Back) }, onShowCode = { send(Action.Back) })
            else -> SaleScreen(face, actions, close?.takeIf { it.orderId == id }?.view, flows.refund.value.takeIf { target })
        }
    }

    /**
     * A keypad till on a wide window (core.md decision 257): the shift's list on the left, Settings in its top row and
     * no New sale of its own; on the right the keypad, with nothing to close, or the sale the engine has open, drawn by
     * its own screen, with New sale under it. The sale stays in the list beside it.
     */
    @Composable
    private fun SplitTill(flows: Flows, actions: TillActions, shiftPlace: ShiftPlace) {
        val shift = flows.shift.value ?: return
        // What the right pane shows, kept while the stage leaves for Settings, so it leaves with what it showed.
        val shown = remember { arrayOf<Screen>(Screen.Till) }
        flows.screen.value?.takeIf { it.onStage(true) == Screen.Till }?.let { shown[0] = it }
        val screen = shown[0]
        val id = screen.orderIdOrNull()
        val kept = remember(id) { Kept() }
        val face = id?.let { flows.sale.value?.takeIf { it.orderId == id }?.let { update -> kept.face(update) } ?: kept.last }
        // The swiped row's sheet: the sale's own while it's open on the right, else the list's.
        val close = flows.close.value.takeIf { flows.screen.value == screen }
        val forSale = close != null && id != null && close.orderId == id
        Row(Modifier.fillMaxSize().background(mk.ground)) {
            Box(Modifier.width(SPLIT_LIST_WIDTH).fillMaxHeight().consumeWindowInsets(WindowInsets.safeDrawing.only(WindowInsetsSides.End))) {
                ShiftScreen(shift, actions, shiftPlace, close.takeIf { !forSale }, newSale = false)
            }
            VerticalDivider(color = mk.divider)
            Box(Modifier.weight(1f).fillMaxHeight().consumeWindowInsets(WindowInsets.safeDrawing.only(WindowInsetsSides.Start))) {
                if (face == null) {
                    flows.keypad.value?.let { KeypadScreen(it, closable = false, send = ::send) }
                } else {
                    // The sale's screen as it is full screen, off the stage: its surfaces stay in place.
                    Column(Modifier.fillMaxSize()) {
                        Box(Modifier.weight(1f).consumeWindowInsets(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom))) {
                            CompositionLocalProvider(LocalStage provides null) {
                                when (screen) {
                                    is Screen.Code -> CodeScreen(face) { send(Action.Back) }
                                    is Screen.Order -> OrderPage(face, onBack = { send(Action.Back) }, onShowCode = { send(Action.Back) })
                                    else -> SaleScreen(face, actions, close?.takeIf { forSale }?.view, flows.refund.value)
                                }
                            }
                        }
                        Surface(Modifier.fillMaxWidth(), color = mk.ground, contentColor = mk.ink) {
                            PrimaryButton(
                                shift.newSale?.label ?: "New sale", { send(Action.NewSale) },
                                Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal))
                                    .padding(16.dp).fillMaxWidth(),
                                height = 56.dp,
                            )
                        }
                    }
                }
            }
        }
    }

    /** The last face a sale's screen drew, and the update it came from, so the same update makes the same face. */
    private class Kept {
        var last: SaleFace? = null
        private var from: Update.Sale? = null

        fun face(update: Update.Sale): SaleFace {
            if (update !== from) {
                from = update
                last = SaleFace.of(update)
            }
            return last!!
        }
    }

    /**
     * Onboarding and Settings: each step draws its own page of the engine's
     * onboarding view, kept while the stage leaves it, as the view moves on.
     */
    @Composable
    private fun SetupScreens(s: Screen, flows: Flows) {
        if (s == Screen.Settings) {
            val settings = flows.settings.value ?: return
            SettingsScreen(settings, settingsActions)
            return
        }
        val kept = remember(s) { arrayOfNulls<OnboardingView>(1) }
        val current = flows.onboarding.value?.takeIf { it.page.isFor(s) }
        if (current != null) kept[0] = current
        val view = current ?: kept[0] ?: return
        val step = view.stepLabel
        when (val page = view.page) {
            is OnboardingPage.Welcome -> WelcomeScreen(page, step) { send(Action.Next) }
            is OnboardingPage.Scan -> ScanScreen(
                page, step,
                onPaste = { send(Action.PasteConnectLink(it)) },
                onScan = { send(Action.ScanConnectCode(it)) },
                onBack = { send(Action.Back) },
            )
            is OnboardingPage.Confirm -> ConfirmScreen(page, step, onConnect = { send(Action.Connect) }, onCancel = { send(Action.Back) })
            is OnboardingPage.Connected -> ConnectedScreen(
                page, step,
                onNext = { send(Action.Next) }, onRetry = { send(Action.ReadStore) }, onWrong = { send(Action.Disconnect) },
            )
            is OnboardingPage.PointOfSale -> PointOfSaleScreen(
                page.view, step,
                PosSetupActions(
                    onChoose = { send(Action.ChoosePointOfSale(it.pos())) },
                    onNext = { send(Action.Next) },
                    onChangeLocation = { send(Action.ChangeLocation) },
                    onLookAgain = { send(Action.LookAgain) },
                    onSkip = { send(Action.Skip) },
                    onBack = { send(Action.Back) },
                    onCurrency = { send(Action.ChooseKeypadCurrency(it)) },
                    onSearchCurrency = { send(Action.SearchKeypadCurrency(it)) },
                ),
                heading = view.heading, lead = view.lead,
            )
            is OnboardingPage.Locations -> LocationsScreen(page, step, onPick = { send(Action.ChooseLocation(it)) }, onBack = { send(Action.Back) })
            is OnboardingPage.Alerts -> AlertsScreen(page, step, onAllow = ::allowNotifications, onSkip = { send(Action.Skip) }, onBack = { send(Action.Back) })
            is OnboardingPage.Float -> FloatScreen(
                page,
                onOpenSettings = { send(Action.SetBubble(true)); openOverlaySettings() },
                onNotificationsOnly = { send(Action.Skip) },
                onContinue = { send(Action.Next) },
                onBack = { send(Action.Back) },
            )
            is OnboardingPage.Ready -> ReadyScreen(
                page, bubble = flows.settings.value?.bubble?.on == true,
                onStart = { send(Action.StartShift) },
            )
            is OnboardingPage.Counter -> CounterScreen(
                page.view,
                onPick = { send(Action.SetAlertFor(it)) },
                onNext = { send(Action.Next) },
                onBack = { send(Action.Back) },
            )
            is OnboardingPage.ShowSales -> ShowSalesScreen(
                page.view,
                onPick = { send(Action.ChooseShowSales(it)) },
                onNext = { send(Action.Next) },
                onBack = { send(Action.Back) },
            )
            is OnboardingPage.PaymentsMode -> PaymentsModeScreen(
                page.view, view.heading, view.lead, step,
                onChoose = { send(Action.ChoosePaymentsMode(it)) }, onNext = { send(Action.Next) }, onBack = { send(Action.Back) },
            )
            is OnboardingPage.WalletEntry -> WalletEntryScreen(
                page.view, view.heading, view.lead, step,
                onConnect = { send(Action.ConnectFob) }, onWay = { send(Action.ChooseWalletWay(it)) },
                onGetFob = { send(Action.GetFob) }, onBack = { send(Action.Back) },
            )
            is OnboardingPage.FobRequest -> {
                val request = page.view.qr
                val here = remember { fobPackage() != null }
                // Fob on this device opens by itself, once for each request; the page stays for Fob on another phone.
                var opened by rememberSaveable { mutableStateOf<String?>(null) }
                LaunchedEffect(request) {
                    if (here && opened != request) {
                        opened = request
                        openFob(request)
                    }
                }
                FobRequestScreen(
                    page.view, view.heading, view.lead, step, fobHere = here,
                    onOpenFob = { openFob(request) }, onAnswer = { send(Action.FobAnswer(it)) }, onBack = { send(Action.Back) },
                )
            }
            is OnboardingPage.FobConnected -> FobConnectedScreen(
                page.view, view.heading, view.lead, step, stepperActions,
                onNext = { send(Action.Next) }, onOtherWallet = { send(Action.UseAnotherWallet) },
            )
            is OnboardingPage.WalletCode -> WalletCodeScreen(
                page.view, view.heading, view.lead, step,
                onScan = { send(Action.ScanWalletCode(it)) }, onPaste = { send(Action.PasteWalletCode(it)) }, onBack = { send(Action.Back) },
            )
            is OnboardingPage.WalletKeys -> WalletKeysScreen(
                page.view, view.heading, view.lead, step,
                onEdit = { network, address, viewKey -> send(Action.EditWalletKeys(network, address, viewKey)) },
                onNext = { send(Action.Next) }, onBack = { send(Action.Back) },
            )
            is OnboardingPage.WalletSeed -> WalletSeedScreen(
                page.view, view.heading, view.lead, step,
                onLength = { send(Action.ChooseSeedLength(it)) },
                onScanInstead = { send(Action.ChooseWalletWay(WalletWay.SCAN_CODE)) },
                onEnter = { send(Action.EnterSeed(it)) },
                onBack = { send(Action.Back) },
            )
            is OnboardingPage.LightServer -> LightServerScreen(
                page.view, view.heading, view.lead, step,
                onEdit = { url, selfSigned -> send(Action.EditLightServer(url, selfSigned)) },
                onNext = { send(Action.Next) }, onBack = { send(Action.Back) },
            )
            is OnboardingPage.ThisTill -> ThisTillScreen(
                page.view, view.heading, view.lead, step,
                ThisTillActions(
                    stepperActions,
                    onNumber = { send(Action.StepTillNumber(it)) },
                    onNode = { send(Action.SetNode(it)) },
                    onNext = { send(Action.Next) },
                    onBack = { send(Action.Back) },
                    send = { send(it) },
                ),
            )
            is OnboardingPage.Prices -> PricesScreen(
                page.view, view.heading, view.lead, step,
                onChoose = { send(Action.ChoosePriceProvider(it)) }, onNext = { send(Action.Next) }, onBack = { send(Action.Back) },
            )
        }
    }

    private val stepperActions by lazy {
        StepperActions(onPaidAfter = { send(Action.StepPaidAfter(it)) }, onExpiry = { send(Action.StepCodeExpiry(it)) })
    }

    private val settingsActions by lazy {
        SettingsActions(
            send = ::send,
            onAlerts = ::openNotificationSettings,
            onBubble = { on ->
                send(Action.SetBubble(on))
                if (on && !Settings.canDrawOverlays(this)) openOverlaySettings()
            },
            onCopyAddress = { send(Action.CopyWalletAddress) },
            onGetDistributor = { openInTab(NTFY) },
            distributorName = { pkg ->
                runCatching { packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString() }.getOrDefault(pkg)
            },
        )
    }

    /** One confirming buzz, the clipboard, the browser: what only the platform does. */
    private fun effect(effect: Effect) {
        when (effect) {
            is Effect.OpenUrl -> openInTab(effect.url)
            // A payment landed with the app on screen (`docs/motion.md` §6).
            is Effect.Haptic -> window.decorView.confirm()
            // TillService.follow copies, and posts the notifications, whether or not Till is on screen.
            is Effect.CopyText, is Effect.Notify, is Effect.ClearNotification -> Unit
            // Erased or disconnected: here too, since the service may already have stopped (docs/decisions/core.md 264).
            is Effect.ClearNotifications -> SaleNotifications(this).clearAll()
            // The overlay collapses its own panel; Till on screen has none up.
            is Effect.LowerPanel -> Unit
            // TillService.follow probes for Orbot whether or not this activity has started collecting yet.
            is Effect.ProbeTor -> Unit
        }
    }

    override fun onStart() {
        super.onStart()
        send(Action.Opened)
        EngineStore.appStarted()
        (application as TillApp).clockMayHaveChanged()
    }

    override fun onResume() {
        super.onResume()
        resumed.intValue++
        permissionsMayHaveChanged()
    }

    override fun onMultiWindowModeChanged(isInMultiWindowMode: Boolean, newConfig: android.content.res.Configuration) {
        super.onMultiWindowModeChanged(isInMultiWindowMode, newConfig)
        resumed.intValue++
    }

    override fun onStop() {
        send(Action.Backgrounded)
        EngineStore.appStopped()
        super.onStop()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handle(intent)
    }

    private fun handle(intent: Intent?) {
        intent?.getStringExtra(EXTRA_ORDER)?.let { send(Action.OpenSale(it)) }
        if (intent?.getBooleanExtra(EXTRA_SHIFT, false) == true) send(Action.OpenShift)
        when (intent?.action) {
            // A point of sale's sign-in coming back, on its redirect or the build's return link (the setups'
            // `returnUri`): the engine says which, and ignores anything else.
            Intent.ACTION_VIEW -> intent.data?.let { send(Action.SignInReturned(it.toString())) }
            // Shared text, whole: the engine finds the connect link in it (docs/decisions/core.md 103).
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.let { send(Action.SharedText(it)) }
        }
    }

    private fun permissionsMayHaveChanged() = (application as TillApp).permissionsMayHaveChanged()


    private fun openInTab(url: String) {
        runCatching { CustomTabsIntent.Builder().build().launchUrl(this, url.toUri()) }
    }

    private fun allowNotifications() {
        if (Build.VERSION.SDK_INT >= 33 && !NotificationManagerCompat.from(this).areNotificationsEnabled()) {
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            send(Action.Next)
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

    /**
     * Back to the point of sale the engine named: Square Point of Sale, if it's installed, else Till steps back to
     * whatever was under it. Clover runs on its own device, with Till on a separate phone beside it, so there is no
     * Clover app to open and the engine never offers the button there (docs/decisions/core.md 94).
     */
    private fun backToPos(pos: PosMark) {
        val app = when (pos) {
            PosMark.SQUARE -> packageManager.getLaunchIntentForPackage(SQUARE_POS)
            PosMark.CLOVER, PosMark.TILL -> return
        }
        if (app != null) startActivity(app) else moveTaskToBack(true)
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

/** The engine's [Work] as [WorkBar] shows it, read here alone so a change recomposes nothing else. */
@Composable
private fun WorkOverlay(work: State<Work>) = WorkBar(work.value.shownBusy, work.value.shownProblem)

/** The expanded width class, from which a keypad till shows its sales and keypad side by side (core.md decision 257). */
private const val SPLIT_WIDTH_DP = 840

/** The wide layout's list column. */
private val SPLIT_LIST_WIDTH = 440.dp

/**
 * Where [this] screen is drawn on the stage: with [split] (a keypad till on a wide window), the shift, the keypad and a
 * sale's screens are all the two panes of [Screen.Till]; otherwise each screen is its own.
 */
private fun Screen.onStage(split: Boolean): Screen = when {
    split && (this == Screen.Till || this == Screen.Keypad || this is Screen.Sale || this is Screen.Code || this is Screen.Order) -> Screen.Till
    else -> this
}

/** Whether [page] is the step [screen] shows. */
private fun OnboardingPage.isFor(screen: Screen): Boolean = when (this) {
    is OnboardingPage.Welcome -> screen == Screen.Welcome
    is OnboardingPage.Scan -> screen == Screen.Scan
    is OnboardingPage.Confirm -> screen == Screen.Confirm
    is OnboardingPage.Connected -> screen == Screen.Connected
    is OnboardingPage.PointOfSale -> screen == Screen.PointOfSale
    is OnboardingPage.Locations -> screen == Screen.Locations
    is OnboardingPage.Alerts -> screen == Screen.Alerts
    is OnboardingPage.Float -> screen == Screen.Float
    is OnboardingPage.Ready -> screen == Screen.Ready
    is OnboardingPage.Counter -> screen == Screen.Counter
    is OnboardingPage.ShowSales -> screen == Screen.ShowSales
    is OnboardingPage.PaymentsMode -> screen == Screen.PaymentsMode
    is OnboardingPage.WalletEntry -> screen == Screen.WalletEntry
    is OnboardingPage.FobRequest -> screen == Screen.FobRequest
    is OnboardingPage.FobConnected -> screen == Screen.FobConnected
    is OnboardingPage.WalletCode -> screen == Screen.WalletCode
    is OnboardingPage.WalletKeys -> screen == Screen.WalletKeys
    is OnboardingPage.WalletSeed -> screen == Screen.WalletSeed
    is OnboardingPage.ThisTill -> screen == Screen.ThisTill
    is OnboardingPage.Prices -> screen == Screen.Prices
    is OnboardingPage.LightServer -> screen == Screen.LightServer
}
