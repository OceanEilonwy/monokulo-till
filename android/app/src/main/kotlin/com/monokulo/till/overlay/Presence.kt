package com.monokulo.till.overlay

import androidx.compose.animation.core.Animatable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.Preset
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * How far an overlay window's surface is in, 0 (not there) to 1 (at rest),
 * for the panel growing out of the bubble and the bubble appearing
 * (`docs/motion.md` M1, M4). One [Animatable] per window, read only in
 * `graphicsLayer {}` and `offset {}`, animated in the window's own
 * composition (its frame clock) with `motion.surface`: [enter] and [exit]
 * retarget from the current value and velocity, so a tap mid-grow reverses
 * it, and an exit's landing is reported once and only if nothing
 * interrupted it. The overlay calls in before the window has composed, so
 * the latest call waits for [Attach].
 */
@Stable
class Presence(initial: Float = 0f) {
    private val animatable = Animatable(initial, Preset.FRACTION_THRESHOLD).apply { updateBounds(0f, 1f) }
    private var scope: CoroutineScope? = null
    private var motion: Motion = Motion.Full
    private var pending: (() -> Unit)? = null

    /** The progress; a state read, so only in a draw or layout lambda. */
    val value: Float get() = animatable.value

    /** Whether it is on its way in or out, neither gone nor at rest; a state read, so only in a draw or layout lambda. */
    val moving: Boolean get() = animatable.value > 0f && animatable.value < 1f

    /** Where it is going, 1 (in) or 0 (out); a state read, for a coroutine waiting on a turn, never in composition. */
    val heading: Float get() = animatable.targetValue

    /** Grows in from wherever it is. */
    fun enter() = go { animatable.animateTo(1f, motion.surface.fraction) }

    /**
     * Goes out from wherever it is, at [velocity] (progress a second; null
     * keeps the current one), and calls [landed] once it's out, unless an
     * [enter] or a [snap] came first. Out is [landsAt] or below: what is
     * drawn there is drawn as at 0, so the window can go on that frame
     * instead of after the spring's invisible tail. The landing is reported
     * mid-frame, from inside the spring's frame callback, so the overlay
     * removes the panel's window from within that window's own frame: that
     * is intended (what it draws there is drawn as at 0, decision 227) and
     * tested (`PresenceOnDeviceTest`, `OverlayFramesTest`); moved to a post,
     * it would draw one more frame of a window that should be gone.
     */
    fun exit(velocity: Float? = null, landsAt: Float = 0f, landed: () -> Unit = {}) = go {
        var out = animatable.value <= landsAt
        if (out) landed()
        animatable.animateTo(0f, motion.surface.fraction, initialVelocity = velocity ?: animatable.velocity) {
            if (!out && value <= landsAt) {
                out = true
                landed()
            }
        }
        if (!out) landed()
    }

    /** Jumps to [to]: the system moved things, not the cashier. */
    fun snap(to: Float) = go { animatable.snapTo(to) }

    private fun go(run: suspend () -> Unit) {
        val scope = scope
        if (scope == null) {
            pending = { this.scope?.launch { run() } }
        } else {
            // A new animateTo takes over from the running one at its velocity; the old one's landing never comes.
            scope.launch { run() }
        }
    }

    /** Ties this to the window's composition: its frame clock, and the motion it asked for. */
    @Composable
    fun Attach() {
        val scope = rememberCoroutineScope()
        val m = LocalMotion.current
        SideEffect {
            motion = m
            if (this.scope !== scope) {
                this.scope = scope
                pending?.invoke()
                pending = null
            }
        }
    }
}

/**
 * A panel window's motion (`docs/motion.md` M1): its [raise], where the
 * free bubble is on screen ([bubbleAt], the overlay's), and where the panel
 * placed its docked bubble and its card, so the docked bubble starts where
 * the free one was and the card grows out of it. The places are set once
 * per layout and read only in `graphicsLayer {}` and `offset {}`.
 */
@Stable
class PanelMotion(initial: Float, private val bubble: State<Offset?>) {
    val raise = Presence(initial)

    /** The free bubble's centre on screen: where the docked one comes from and goes back to. */
    val bubbleAt: Offset? get() = bubble.value

    /** The docked bubble's centre on screen, once placed. */
    var dock by mutableStateOf<Offset?>(null)

    /** The panel's card (or, on a tablet, the order in its place) on screen, once placed: where a phone's order grows from. */
    var card by mutableStateOf<Rect?>(null)

    /** A phone's order on screen, filling the window, once placed. */
    var window by mutableStateOf<Rect?>(null)

    /** Whether the panel shows no docked bubble (the order fills a phone's window), so the free one can go at once. */
    var undocked = false

    /**
     * The docked bubble's centre on screen at [raise], sliding from the free
     * bubble to the dock ([Panel.docked]); the dock alone (or the free bubble
     * alone) while only one is known, null before either is. Read in the
     * layer only.
     */
    fun bubbleNow(raise: Float): Offset? {
        val dock = dock
        val from = bubbleAt
        return if (dock != null && from != null) Panel.docked(dock, from, raise) else dock ?: from
    }

    /** Whether the panel has placed what the free bubble hands over to. */
    val placed: Boolean get() = undocked || dock != null
}
