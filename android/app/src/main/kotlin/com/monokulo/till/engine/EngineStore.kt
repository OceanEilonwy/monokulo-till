package com.monokulo.till.engine

import android.util.Log
import com.monokulo.till.Way
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.Executors
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.till_app.Action
import uniffi.till_app.BubbleView
import uniffi.till_app.RefundSheetView
import uniffi.till_app.Effect
import uniffi.till_app.Engine
import uniffi.till_app.EngineListener
import uniffi.till_app.Screen
import uniffi.till_app.Update
import uniffi.till_screens.CloseView
import uniffi.till_screens.NotificationView
import uniffi.till_screens.OnboardingView
import uniffi.till_screens.SettingsView
import uniffi.till_screens.ShiftView
import uniffi.till_screens.KeypadView

/** Busy with a network call (buttons wait), and what went wrong last; and of those, what no screen's own view says. */
data class Work(val busy: Boolean = false, val problem: String? = null, val shownBusy: Boolean = false, val shownProblem: String? = null)

/** How the screen came ([Update.Nav]), where Back would go from it, and whether the system's Back is the app's there. */
data class Nav(val byItself: Boolean = false, val backTarget: Screen? = null, val backEnabled: Boolean = false, val way: Way = Way.None)

/** The close sheet over a sale, while one is asked. */
data class CloseSheet(val orderId: String?, val view: CloseView?)

/**
 * What the engine last published for each slice of the screen, as Tern's
 * `Store` holds it: one [StateFlow] per [Update], fed by [Listener]. It holds
 * no state of its own; Compose draws these and sends [Action]s back.
 */
object EngineStore {
    private val mutableScreen = MutableStateFlow<Screen?>(null)
    private val mutableWork = MutableStateFlow(Work())
    private val mutableService = MutableStateFlow(false)
    private val mutableOnboarding = MutableStateFlow<OnboardingView?>(null)
    private val mutableSettings = MutableStateFlow<SettingsView?>(null)
    private val mutableNav = MutableStateFlow(Nav())
    private val mutableShift = MutableStateFlow<ShiftView?>(null)
    private val mutableKeypad = MutableStateFlow<KeypadView?>(null)
    private val mutableSale = MutableStateFlow<Update.Sale?>(null)
    private val mutableClose = MutableStateFlow(CloseSheet(null, null))
    private val mutableRefund = MutableStateFlow<RefundSheetView?>(null)
    private val mutableBubble = MutableStateFlow<BubbleView?>(null)
    private val mutableNotification = MutableStateFlow<NotificationView?>(null)
    private val mutableEffects = MutableSharedFlow<Effect>(extraBufferCapacity = 16, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    val screen: StateFlow<Screen?> = mutableScreen.asStateFlow()
    val work: StateFlow<Work> = mutableWork.asStateFlow()
    /** The till runs: Android keeps its foreground service meanwhile. */
    val service: StateFlow<Boolean> = mutableService.asStateFlow()
    val onboarding: StateFlow<OnboardingView?> = mutableOnboarding.asStateFlow()
    val settings: StateFlow<SettingsView?> = mutableSettings.asStateFlow()
    val nav: StateFlow<Nav> = mutableNav.asStateFlow()
    val shift: StateFlow<ShiftView?> = mutableShift.asStateFlow()
    /** Till's keypad, while it's the till's home. */
    val keypad: StateFlow<KeypadView?> = mutableKeypad.asStateFlow()
    /** The sale on screen, as each of its screens draws it; kept while it leaves. */
    val sale: StateFlow<Update.Sale?> = mutableSale.asStateFlow()
    val close: StateFlow<CloseSheet> = mutableClose.asStateFlow()
    val refund: StateFlow<RefundSheetView?> = mutableRefund.asStateFlow()
    val bubble: StateFlow<BubbleView?> = mutableBubble.asStateFlow()
    val notification: StateFlow<NotificationView?> = mutableNotification.asStateFlow()
    /** What only the platform can do; dropped when nothing collects them in time. */
    val effects: SharedFlow<Effect> = mutableEffects.asSharedFlow()

    @Volatile
    private var engine: Engine? = null
    private val early = ConcurrentLinkedQueue<Action>()

    /** Sends [action] to the engine, or keeps it until the engine has started. Never blocks. */
    fun send(action: Action) {
        val ready = engine
        if (ready != null) ready.send(action) else early.add(action)
    }

    /**
     * The app's lifecycle for the engine's ledger (docs/decisions/core.md
     * 151): Till left the screen, or came back. The engine decides what to
     * close and reopen; `Engine.suspend` blocks until it has, so both run in
     * order on one thread of their own, never the main one. Android has no
     * second process writing the ledger, so it never calls `ledgerChanged`.
     */
    fun appStopped() = lifecycle.execute { engine?.`suspend`() }

    fun appStarted() = lifecycle.execute { engine?.resume() }

    private val lifecycle = Executors.newSingleThreadExecutor { Thread(it, "till-lifecycle") }

    /** The engine has started: what was sent before goes to it now, in order. */
    fun attach(ready: Engine) {
        engine = ready
        while (true) ready.send(early.poll() ?: break)
    }

    internal fun update(update: Update) {
        when (update) {
            // Android shows neither the core's version nor a separate Wake-ups screen (it draws them inside Settings).
            is Update.Core, is Update.WakeUps -> Unit
            is Update.Screen -> mutableScreen.value = update.screen
            is Update.Work -> mutableWork.value = Work(update.busy, update.problem, update.shownBusy, update.shownProblem)
            is Update.Service -> mutableService.value = update.running
            is Update.Onboarding -> mutableOnboarding.value = update.view
            is Update.Settings -> mutableSettings.value = update.view
            is Update.Nav -> mutableNav.value = Nav(update.byItself, update.backTarget, update.backEnabled, Way(update.back, update.steps))
            is Update.Shift -> mutableShift.value = update.view
            is Update.Keypad -> mutableKeypad.value = update.view
            is Update.Sale -> mutableSale.value = update
            is Update.CloseSheet -> mutableClose.value = CloseSheet(update.orderId, update.view)
            is Update.RefundSheet -> mutableRefund.value = update.view
            is Update.Bubble -> mutableBubble.value = update.view
            is Update.Notification -> mutableNotification.value = update.view
            // The Live Activity is iPhone only; Android never gets one.
            is Update.LiveActivity -> Unit
        }
    }

    internal fun effect(effect: Effect) {
        mutableEffects.tryEmit(effect)
    }
}

/** Where the engine's task publishes: straight into [EngineStore]. */
object Listener : EngineListener {
    override fun onUpdate(update: Update) = EngineStore.update(update)

    override fun onEffect(effect: Effect) = EngineStore.effect(effect)

    override fun onLog(message: String) {
        Log.w("Till", message)
    }
}
