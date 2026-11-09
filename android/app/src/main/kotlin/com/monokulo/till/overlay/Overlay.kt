package com.monokulo.till.overlay

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.PixelFormat
import android.os.Build
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewTreeObserver
import android.view.WindowManager
import android.widget.Button
import android.widget.FrameLayout
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.AndroidUiDispatcher
import androidx.compose.ui.platform.ComposeView
import androidx.core.view.ViewCompat
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat.AccessibilityActionCompat
import androidx.core.view.doOnPreDraw
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.monokulo.till.BuildConfig
import com.monokulo.till.engine.EngineStore
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlin.math.hypot
import kotlin.math.roundToInt
import uniffi.till_app.Action
import uniffi.till_app.Effect
import uniffi.till_screens.BubbleTap
import uniffi.till_screens.BubbleLook
import uniffi.till_screens.PanelFace

/** What the overlay's windows show: the engine's bubble ([uniffi.till_app.BubbleView]), and the panel's page. */
data class OverlayState(
    val look: BubbleLook = BubbleLook.IDLE,
    /** The sale the bubble and the panel are about: the one waiting, or the one that just finished. */
    val sale: PanelSale? = null,
    /** The header's "Front Counter" (or "From Back till"), before "Open the shift". */
    val where: String = "",
    val page: PanelPage = PanelPage.Code,
    /** Whether a payment landing on the bubble buzzes it: Till isn't on screen, where the buzz is the sale screen's. */
    val buzzes: Boolean = true,
    /** The free bubble pulses: the engine's word (a code waits for its money and no panel is up). */
    val pulses: Boolean = false,
    /** What a screen reader says for the bubble. */
    val label: String = "",
    /** What a tap does with the panel down, and with it up, and what a screen reader says for each. */
    val tap: BubbleTap = BubbleTap.OPEN_APP,
    val tapLabel: String = "",
    val tapOpen: BubbleTap = BubbleTap.OPEN_APP,
    val tapOpenLabel: String = "",
)

/**
 * The bubble over other apps and the panel it opens (`docs/design.md` §9
 * "overlay"; the till tweaks' Panel over Square and The order, full
 * screen). Shown during a shift when the cashier allowed "display over
 * other apps" and didn't choose notifications only, and hidden while
 * Till itself is on screen. The bubble opens the panel and, tapped
 * again, lowers it; so do the panel's grabber and a tap beside it. With no
 * sale to show, the bubble opens Till instead. Runs on the main
 * thread, in the till's service.
 */
class Overlay(
    private val context: Context,
    private val scope: CoroutineScope,
    /** Opens Till on the shift (the panel's header row). */
    private val onOpenShift: () -> Unit,
    /** Brings Till to the front as it was (the bubble tapped with no sale to show). */
    private val onOpenApp: () -> Unit,
    /** Opens Till full screen on a sale. */
    private val onFullScreen: (String) -> Unit,
) {
    /** Where the free bubble rests: the engine's to keep (docs/decisions/core.md 73), told by [Action.MoveBubble]. */
    private val place = BubblePlace()
    private val wm = context.getSystemService(WindowManager::class.java)
    private val owner = WindowOwner()
    private val state = MutableStateFlow(OverlayState())
    private val panelOpen = MutableStateFlow(false)
    private val page = MutableStateFlow<PanelPage>(PanelPage.Code)
    private var bubble: BubbleFrame? = null
    private var bubbleIn: Presence? = null
    private var panel: View? = null
    private var panelMotion: PanelMotion? = null
    /** The windows as the handoff between them sees them (M1). */
    private var windows = Windows()
    /** The free bubble's centre on screen, where the docked bubble comes from and goes back to. */
    private val bubbleAt = mutableStateOf<Offset?>(null)
    /** The last usable and open shown, shown again when the configuration changes. */
    private var showing: Pair<Boolean, Boolean>? = null
    private var job: Job? = null
    private var focus: String? = null
    /** Whether the overlay was usable at the last look, to log each change. */
    private var wasUsable: Boolean? = null
    /** What the engine was last told of the panel ([Action.PanelShown]): its one-second timer and the pulse follow it. */
    private var panelShown = false

    /** Whether the person allowed Till to draw over other apps. */
    fun allowed(): Boolean = Settings.canDrawOverlays(context)

    fun start() {
        owner.start()
        job = scope.launch {
            val panel = combine(panelOpen, page) { open, page -> open to page }
            // The paid moment's collapse (M9): the engine says when, and keeps the bubble green until it lands.
            launch { EngineStore.effects.collect { if (it is Effect.LowerPanel) lower() } }
            combine(EngineStore.bubble, panel) { view, (open, shown) ->
                val previous = focus
                val sale = view?.let(PanelSale::of)
                focus = view?.orderId
                if (view != null) place.follow(view.left, view.y)
                // The whole order is about the sale whose code is up: another sale, or this one ending, brings the code back.
                if (focus != previous || sale?.face != PanelFace.CODE || sale.order == null) page.value = PanelPage.Code
                // Green through the paid moment, its collapse included: the engine's (docs/decisions/core.md 95).
                val look = view?.look ?: BubbleLook.IDLE
                state.value = OverlayState(
                    look, sale, sale?.where.orEmpty(), if (sale?.face == PanelFace.CODE) shown else PanelPage.Code,
                    buzzes = view?.buzzes ?: true,
                    pulses = view?.pulses ?: false,
                    label = view?.label.orEmpty(),
                    tap = view?.tap ?: BubbleTap.OPEN_APP,
                    tapLabel = view?.tapLabel.orEmpty(),
                    tapOpen = view?.tapOpen ?: BubbleTap.OPEN_APP,
                    tapOpenLabel = view?.tapOpenLabel.orEmpty(),
                )
                val allowed = allowed()
                // The engine's word (the cashier wants it, the till runs, Till isn't on screen), and Android's still.
                val usable = view?.shown == true && allowed
                if (BuildConfig.DEBUG && usable != wasUsable) {
                    // Each change of whether the overlay is up, with what changed it, for a lower nobody asked for.
                    Log.d(TAG, "usable=$usable shown=${view?.shown} allowed=$allowed")
                }
                wasUsable = usable
                usable to (open && sale != null)
            }.collect { (usable, open) -> show(usable, open) }
        }
    }

    fun stop() {
        job?.cancel()
        // The service is going: no exits to wait for.
        bubble?.let { runCatching { wm.removeViewImmediate(it) } }
        panel?.let { runCatching { wm.removeViewImmediate(it) } }
        bubble = null
        bubbleIn = null
        panel = null
        panelMotion = null
        windows = Windows()
        tellPanelShown()
        owner.stop()
    }

    /** Tells the engine whether a panel is up over other apps, once each time that changes. */
    private fun tellPanelShown() {
        val up = panel != null
        if (up == panelShown) return
        panelShown = up
        EngineStore.send(Action.PanelShown(up))
    }

    /** The notification's "Show code": opens the panel; false when the overlay can't be shown. */
    fun showCode(): Boolean {
        if (EngineStore.bubble.value?.shown != true || !allowed()) return false
        panelOpen.value = true
        return true
    }

    /** The screen changed under the windows (a fold opened, split screen): the panel's kind is judged again. */
    fun configurationChanged() {
        // Where the free bubble was is somewhere else on a turned or unfolded screen: the panel collapses into its dock.
        if (bubble == null) bubbleAt.value = null
        showing?.let { (usable, open) -> show(usable, open) }
    }

    /** Whether the panel floats: by the screen's smallest width, the same whichever way a phone is held. */
    private fun wide(): Boolean = Panel.wide(context.resources.configuration.smallestScreenWidthDp)

    private fun show(usable: Boolean, open: Boolean) {
        showing = usable to open
        on(WindowEvent.Want(usable, open, wide()))
        if (!usable) lower()
    }

    /** Moves the windows on by [event], in the handoff's order. */
    private fun on(event: WindowEvent) {
        val step = Handoff.next(windows, event)
        windows = step.windows
        for (act in step.acts) {
            when (act) {
                WindowAct.AddBubble -> addBubble(atRest = false)
                WindowAct.AddBubbleAtRest -> addBubble(atRest = true)
                WindowAct.ShowBubble -> bubbleIn?.enter()
                WindowAct.HideBubble -> bubbleIn?.exit { on(WindowEvent.BubbleLeft) }
                WindowAct.RemoveBubble -> removeBubble()
                WindowAct.AddPanel -> addPanel(windows.wide, open = false)
                WindowAct.AddPanelOpen -> addPanel(windows.wide, open = true)
                WindowAct.OpenPanel -> panelMotion?.raise?.enter()
                // The collapse lands, then the panel's window goes: told the panel is down, the engine fades a paid green.
                WindowAct.ClosePanel -> panelMotion?.raise?.exit(step.closeVelocity, landsAt = Panel.IN_BUBBLE) { on(WindowEvent.PanelLanded) }
                WindowAct.RemovePanel -> removePanel()
            }
        }
        tellPanelShown()
    }

    /** Lowers the panel at the grabber's [velocity] (progress a second). */
    private fun fling(velocity: Float) {
        on(WindowEvent.Flung(velocity))
        lower()
    }

    /** A tap on the bubble, free or docked: the panel up or down on the sale, or Till when there is none. */
    private fun tapBubble() {
        val s = state.value
        when (if (panelOpen.value) s.tapOpen else s.tap) {
            BubbleTap.OPEN_APP -> {
                // Never left wanting a panel that can't show: the next tap after a sale arrives opens it.
                panelOpen.value = false
                // Once on screen, Till's own visibility takes the overlay down (the engine's BubbleView.shown).
                onOpenApp()
            }
            BubbleTap.OPEN_PANEL -> panelOpen.value = true
            BubbleTap.LOWER_PANEL -> lower()
        }
    }

    /** Lowers the panel to the bubble, its code face up for next time. */
    private fun lower() {
        panelOpen.value = false
        page.value = PanelPage.Code
    }

    private fun compose(content: @androidx.compose.runtime.Composable () -> Unit) = ComposeView(context).apply {
        setViewTreeLifecycleOwner(owner)
        setViewTreeSavedStateRegistryOwner(owner)
        setContent {
            TillTheme { content() }
        }
    }

    private fun overlayParams(width: Int, height: Int, touchThrough: Boolean) = WindowManager.LayoutParams(
        width,
        height,
        WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
            (if (touchThrough) WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL else 0),
        PixelFormat.TRANSLUCENT,
    )

    /** Adds the free bubble's window: growing in, or [atRest] where the docked bubble just landed. */
    private fun addBubble(atRest: Boolean) {
        if (bubble != null) return
        val prefs = place
        val params = overlayParams(WindowManager.LayoutParams.WRAP_CONTENT, WindowManager.LayoutParams.WRAP_CONTENT, touchThrough = true).apply {
            gravity = Gravity.TOP or if (prefs.bubbleOnLeft) Gravity.START else Gravity.END
            x = 0
            y = prefs.bubbleY ?: (context.resources.displayMetrics.heightPixels / 3)
        }
        val presence = Presence(if (atRest) 1f else 0f)
        // On a tablet the bubble stays up beside the floating panel: a second tap lowers it.
        lateinit var frame: BubbleFrame
        frame = BubbleFrame(context, wm, params, scope, onTap = ::tapBubble) { left, y ->
            val moved = prefs.bubbleOnLeft != left
            prefs.moved(left, y)
            // Dragged to the other edge with a tablet's panel up: the panel steps aside for it, or back.
            if (moved && windows.wide) panel?.let(::placeFloating)
            frame.doOnPreDraw { locate(frame) }
        }
        frame.setViewTreeLifecycleOwner(owner)
        frame.setViewTreeSavedStateRegistryOwner(owner)
        frame.addView(
            compose {
                presence.Attach()
                val s by state.collectAsStateWithLifecycle()
                // On a tablet the bubble stays beside the open panel: nothing to call for while it's up.
                val open by panelOpen.collectAsStateWithLifecycle()
                val m = LocalMotion.current
                // The bubble no longer shows the step: its label says it (§12.5).
                val label = s.label
                val tapLabel = if (open) s.tapOpenLabel else s.tapLabel
                SideEffect {
                    frame.describe(label, tapLabel)
                    frame.motion = m
                }
                // Appearing and going, the circle scales and fades inside its window (M4).
                Box(
                    Modifier.graphicsLayer {
                        val p = presence.value
                        alpha = p
                        compositingStrategy = CompositingStrategy.ModulateAlpha
                        scaleX = 0.6f + 0.4f * p
                        scaleY = scaleX
                    },
                ) { BubbleFace(s.look, pulses = s.pulses, confirms = s.buzzes, presence = presence, label = label) }
            }.apply { importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS },
        )
        runCatching { wm.addView(frame, params) }.onSuccess {
            bubble = frame
            bubbleIn = presence
            if (!atRest) presence.enter()
            frame.doOnPreDraw {
                locate(frame)
                frame.post { if (bubble === frame) on(WindowEvent.BubbleDrawn) }
            }
        }
    }

    /** Notes where the free bubble's centre is on screen. */
    private fun locate(frame: View) {
        val at = IntArray(2).also(frame::getLocationOnScreen)
        bubbleAt.value = Offset(at[0] + frame.width / 2f, at[1] + frame.height / 2f)
    }

    private fun removeBubble() {
        bubble?.let { runCatching { wm.removeView(it) } }
        bubble = null
        bubbleIn = null
    }

    /** Adds the panel's window at progress 0 (it grows once drawn), or already [open] when its kind was swapped. */
    private fun addPanel(wide: Boolean, open: Boolean) {
        if (panel != null) return
        val density = context.resources.displayMetrics.density
        val params = if (wide) {
            overlayParams(WindowManager.LayoutParams.WRAP_CONTENT, WindowManager.LayoutParams.WRAP_CONTENT, touchThrough = true).apply {
                gravity = Gravity.TOP or Gravity.END
                // Beside the bubble when it's on this edge, so the panel grows from the corner next to it, never over it.
                x = (Panel.floatingInsetDp(bubbleOnEnd = !place.bubbleOnLeft) * density).toInt()
                y = (8 * density).toInt()
            }
        } else {
            overlayParams(WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT, touchThrough = false).apply {
                // Edge to edge, under the status and navigation bars and round a cutout, as the app's screens are:
                // the panel and The order pad themselves with the safe drawing insets.
                if (Build.VERSION.SDK_INT >= 30) {
                    fitInsetsTypes = 0
                    layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                } else if (Build.VERSION.SDK_INT >= 28) {
                    layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                }
            }
        }
        // One set of actions for the window's life, outside its content: a new state hands the panel the same ones.
        val actions = PanelActions(
            onLower = ::lower,
            onFlung = ::fling,
            onOpenShift = { lower(); onOpenShift() },
            onOrder = { page.value = PanelPage.Order },
            onBackToCode = { page.value = PanelPage.Code },
            onCopy = { EngineStore.send(Action.CopyAddress(it.orderId)) },
            onCopyXmr = { EngineStore.send(Action.CopyAmount(it.orderId)) },
            onFullScreen = { lower(); onFullScreen(it.orderId) },
            onBubble = ::tapBubble,
        )
        val motion = PanelMotion(if (open) 1f else 0f, bubbleAt)
        val view = compose {
            motion.raise.Attach()
            val s by state.collectAsStateWithLifecycle()
            val sale = s.sale ?: return@compose
            if (wide) FloatingPanel(sale, s.where, s.page, actions, motion) else PhonePanel(sale, s.where, s.look, s.page, actions, motion, s.buzzes)
        }
        runCatching { wm.addView(view, params) }.onSuccess {
            panel = view
            panelMotion = motion
            tellPanelShown()
            // The free bubble goes once a frame shows the docked one where it is, never before (amendment 6).
            view.afterPlaced({ wide || motion.placed }) { if (panel === view) on(WindowEvent.PanelDrawn) }
        }
    }

    /** Puts a floating panel window beside the bubble's edge, or off it. */
    private fun placeFloating(view: View) {
        val params = view.layoutParams as? WindowManager.LayoutParams ?: return
        params.x = (Panel.floatingInsetDp(bubbleOnEnd = !place.bubbleOnLeft) * context.resources.displayMetrics.density).toInt()
        runCatching { wm.updateViewLayout(view, params) }
    }

    private fun removePanel() {
        panel?.let { runCatching { wm.removeView(it) } }
        panel = null
        panelMotion = null
    }
}

/**
 * Calls [then] after the first frame drawn once [placed] holds: a pre-draw
 * listener that sees it, asks for one more frame (the one drawn with what
 * was placed), and removes itself at that frame's pre-draw. Frames, not a
 * delay: under reduced motion nothing else would ask for the second one.
 */
private fun View.afterPlaced(placed: () -> Boolean, then: () -> Unit) {
    val listener = object : ViewTreeObserver.OnPreDrawListener {
        private var seen = false

        override fun onPreDraw(): Boolean {
            if (seen) {
                viewTreeObserver.removeOnPreDrawListener(this)
                post(then)
            } else if (placed()) {
                seen = true
                invalidate()
            }
            return true
        }
    }
    viewTreeObserver.addOnPreDrawListener(listener)
}

/**
 * Where the free bubble rests, its edge and height, as the engine last
 * published it (`BubbleView.left` and `y`): the engine keeps it, under the
 * Kotlin stores' `bubble.left` and `bubble.y`, and is told of a move once
 * the drag lets go ([Action.MoveBubble]); meanwhile the place just let go
 * of stands, so the next frame doesn't jump back.
 */
private class BubblePlace {
    var bubbleOnLeft: Boolean = false
        private set
    var bubbleY: Int? = null
        private set

    /** The engine's place, as it publishes it. */
    fun follow(left: Boolean, y: Int?) {
        bubbleOnLeft = left
        bubbleY = y
    }

    /** A drag let go against the [left] edge (else the right) at height [y]: the engine keeps it. */
    fun moved(left: Boolean, y: Int) {
        follow(left, y)
        EngineStore.send(Action.MoveBubble(left, y))
    }
}

/** The lifecycle and saved state Compose needs to run in a window that has no activity. */
private class WindowOwner : SavedStateRegistryOwner {
    private val registry = LifecycleRegistry(this)
    private val saved = SavedStateRegistryController.create(this)
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry get() = saved.savedStateRegistry

    fun start() {
        if (registry.currentState != Lifecycle.State.INITIALIZED) return
        saved.performAttach()
        saved.performRestore(null)
        registry.currentState = Lifecycle.State.RESUMED
    }

    fun stop() {
        if (registry.currentState == Lifecycle.State.INITIALIZED) return
        registry.currentState = Lifecycle.State.DESTROYED
    }
}

/**
 * The bubble's window: a tap opens the panel (or lowers it again), a drag
 * moves it, and letting go snaps it to the nearer side. Touches are
 * handled here, in screen coordinates, because the window moves under the
 * finger. TalkBack hears one button, named for the bubble's state, its
 * double-tap named for what the tap does.
 */
@SuppressLint("ViewConstructor")
private class BubbleFrame(
    context: Context,
    private val wm: WindowManager,
    private val params: WindowManager.LayoutParams,
    /** The overlay's scope: the snap runs in it on the UI dispatcher, whose frame clock it springs by. */
    private val scope: CoroutineScope,
    private val onTap: () -> Unit,
    private val onMoved: (left: Boolean, y: Int) -> Unit,
) : FrameLayout(context) {
    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private var downX = 0f
    private var downY = 0f
    private var startX = 0
    private var startY = 0
    private var dragging = false
    private var velocity: VelocityTracker? = null
    private var snapping: Job? = null
    /** Which side the window's x is measured from. */
    private var fromLeft = params.gravity and Gravity.HORIZONTAL_GRAVITY_MASK == Gravity.START

    /** The motion the bubble's window asked for (its composition sets it). */
    var motion: Motion = Motion.Full

    init {
        isClickable = true
        isFocusable = true
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
        accessibilityLiveRegion = ACCESSIBILITY_LIVE_REGION_POLITE
    }

    private var tapLabel: String? = null

    fun describe(label: String, tap: String) {
        if (contentDescription != label) contentDescription = label
        if (tapLabel != tap) {
            tapLabel = tap
            // A label only: the click itself stays performClick's.
            ViewCompat.replaceAccessibilityAction(this, AccessibilityActionCompat.ACTION_CLICK, tap, null)
        }
    }

    override fun getAccessibilityClassName(): CharSequence = Button::class.java.name

    private fun screenWidth(): Int =
        if (Build.VERSION.SDK_INT >= 30) wm.currentWindowMetrics.bounds.width() else resources.displayMetrics.widthPixels

    private fun screenHeight(): Int =
        if (Build.VERSION.SDK_INT >= 30) wm.currentWindowMetrics.bounds.height() else resources.displayMetrics.heightPixels

    /** The window's left edge in screen coordinates, whichever side it's docked to. */
    private fun absoluteX(): Int = if (fromLeft) params.x else screenWidth() - width - params.x

    override fun onInterceptTouchEvent(ev: MotionEvent) = true

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                // The finger has it: a snap on its way stops where it is.
                snapping?.cancel()
                snapping = null
                downX = ev.rawX
                downY = ev.rawY
                startX = absoluteX()
                startY = params.y
                dragging = false
                velocity?.recycle()
                velocity = VelocityTracker.obtain().also { it.addMovement(ev) }
            }
            MotionEvent.ACTION_MOVE -> {
                velocity?.addMovement(ev)
                val dx = ev.rawX - downX
                val dy = ev.rawY - downY
                if (!dragging && hypot(dx, dy) > slop) {
                    dragging = true
                    params.gravity = Gravity.TOP or Gravity.START
                    fromLeft = true
                }
                if (dragging) {
                    params.x = (startX + dx).toInt().coerceIn(0, screenWidth() - width)
                    params.y = (startY + dy).toInt().coerceIn(0, screenHeight() - height)
                    runCatching { wm.updateViewLayout(this, params) }
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val tracker = velocity
                tracker?.addMovement(ev)
                tracker?.computeCurrentVelocity(1000)
                val vx = tracker?.xVelocity ?: 0f
                tracker?.recycle()
                velocity = null
                when {
                    dragging -> snap(vx)
                    ev.actionMasked == MotionEvent.ACTION_UP -> performClick()
                }
            }
        }
        return true
    }

    override fun performClick(): Boolean {
        super.performClick()
        onTap()
        return true
    }

    override fun onDetachedFromWindow() {
        // The window went mid-snap (the panel took the bubble's place): nothing left to move.
        snapping?.cancel()
        snapping = null
        velocity?.recycle()
        velocity = null
        super.onDetachedFromWindow()
    }

    /**
     * Slides to the nearer side on `motion.flick`, starting at the speed the
     * finger let go at (`docs/motion.md` M3): the window's x moves once a
     * frame, by the UI dispatcher's frame clock, until the spring lands; with
     * motion off it's at the edge at once.
     */
    private fun snap(vx: Float) {
        dragging = false
        val to = Edge.snap(params.x, width, screenWidth())
        val x = Animatable(params.x.toFloat())
        snapping = scope.launch(AndroidUiDispatcher.Main) {
            x.animateTo(to.x.toFloat(), motion.flick.px, initialVelocity = vx) {
                params.x = value.roundToInt()
                runCatching { wm.updateViewLayout(this@BubbleFrame, params) }
            }
            params.gravity = Gravity.TOP or if (to.left) Gravity.START else Gravity.END
            params.x = 0
            fromLeft = to.left
            runCatching { wm.updateViewLayout(this@BubbleFrame, params) }
            onMoved(to.left, params.y)
        }
    }
}

/** The overlay's log tag, for the debug build's line on each change of whether it's up. */
private const val TAG = "TillOverlay"
