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
import androidx.compose.foundation.layout.fillMaxSize
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
import com.monokulo.till.engine.Nav
import com.monokulo.till.ui.AlertsScreen
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.ConfirmScreen
import com.monokulo.till.ui.ConnectedScreen
import com.monokulo.till.ui.FloatScreen
import com.monokulo.till.ui.LocalPointOfSale
import com.monokulo.till.ui.LocalReceiptFolds
import com.monokulo.till.ui.LocationsScreen
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.PointOfSaleScreen
import com.monokulo.till.ui.PosSetupActions
import com.monokulo.till.ui.ReadyScreen
import com.monokulo.till.ui.SaleFace
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ScanScreen
import com.monokulo.till.ui.SettingsActions
import com.monokulo.till.ui.SettingsScreen
import com.monokulo.till.ui.ShiftPlace
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.TillActions
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.WelcomeScreen
import com.monokulo.till.ui.confirm
import com.monokulo.till.ui.mk
import com.monokulo.till.ui.pos
import com.monokulo.till.ui.rememberShiftPlace
import kotlinx.coroutines.launch
import com.monokulo.till.notify.SaleNotifications
import uniffi.till_app.Action
import uniffi.till_app.CopyKind
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
                    sale = store.sale.collectAsStateWithLifecycle(),
                    close = store.close.collectAsStateWithLifecycle(),
                    refund = store.refund.collectAsStateWithLifecycle(),
                )
                // The engine says where the app is as it starts; until then, the ground.
                val first = screenState.value ?: return@TillTheme Box(Modifier.fillMaxSize().background(mk.ground))
                Stage(first, flows)
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
        val stage = remember { SeekableTransitionState(first) }
        // Android's back where the engine says it is the app's (decision 77; docs/decisions/core.md 98). From a sale's
        // screens and Settings the gesture previews the way back the engine names; nothing changes before it commits.
        val scrub = remember { BackScrub() }
        val backEnabled by remember { derivedStateOf { flows.nav.value.backEnabled } }
        FollowScreen(stage, scrub, screenState, onLanded = { send(Action.Landed(it)) })
        // A second Back while the shift is still arriving lands on the shift, never leaves the app (decision 260 c):
        // Back stays the stage's until it rests there. Read as a derived state, so the root recomposes as a change
        // starts and as it lands, never a frame.
        val arriving by remember { derivedStateOf { stage.currentState != stage.targetState } }
        StageBack(
            enabled = backEnabled || (screen == Screen.Till && arriving),
            state = stage, scrub = scrub, current = { screenState.value }, target = { flows.nav.value.backTarget },
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
                    Screen.Till, is Screen.Sale, is Screen.Code, is Screen.Order -> TillScreens(s, flows, actions, shiftPlace)
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
        when {
            // A sale not drawn (yet) shows the shift until it is.
            face == null -> if (shift != null) ShiftScreen(shift, actions, shiftPlace, close?.takeIf { s == Screen.Till })
            s is Screen.Code -> CodeScreen(face) { send(Action.Back) }
            s is Screen.Order -> OrderPage(face, "Back to the sale", onBack = { send(Action.Back) }, onShowCode = { send(Action.Back) })
            else -> SaleScreen(face, actions, close?.takeIf { it.orderId == id }?.view, flows.refund.value.takeIf { target })
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
                ),
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
        }
    }

    private val settingsActions by lazy {
        SettingsActions(
            send = ::send,
            onAlerts = ::openNotificationSettings,
            onBubble = { on ->
                send(Action.SetBubble(on))
                if (on && !Settings.canDrawOverlays(this)) openOverlaySettings()
            },
            onCopyAddress = ::copy,
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
            // The overlay collapses its own panel; Till on screen has none up.
            is Effect.LowerPanel -> Unit
            // TillService.follow probes for Orbot whether or not this activity has started collecting yet.
            is Effect.ProbeTor -> Unit
        }
    }

    override fun onStart() {
        super.onStart()
        send(Action.Opened)
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

    private fun copy(text: String) = SaleNotifications.copyText(this, text, CopyKind.ADDRESS)

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
     * Back to the point of sale the engine named: Square Point of Sale, if it's installed. On Clover, Till runs on the
     * Clover device itself, over the Register it was opened from, so Till steps back to it, as it does when Square
     * isn't installed (docs/decisions/core.md 94).
     */
    private fun backToPos(pos: PosMark) {
        val app = when (pos) {
            PosMark.SQUARE -> packageManager.getLaunchIntentForPackage(SQUARE_POS)
            PosMark.CLOVER -> null
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
}
