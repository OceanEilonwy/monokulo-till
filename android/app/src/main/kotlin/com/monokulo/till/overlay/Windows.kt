package com.monokulo.till.overlay


/**
 * Where an overlay window is in its life: [Up] while it's wanted (arriving
 * or there), [Leaving] while its exit runs, and, for a phone's panel only,
 * [Landed]: collapsed into the docked bubble, waiting for the free bubble
 * to draw in the same place before it goes.
 */
enum class Life { Gone, Up, Leaving, Landed }

/** The overlay's two windows, as the handoff between them sees them (`docs/motion.md` M1). */
data class Windows(
    val usable: Boolean = false,
    /** Whether the panel up, or the one to come, is the tablet's floating one. */
    val wide: Boolean = false,
    val bubble: Life = Life.Gone,
    val panel: Life = Life.Gone,
    /** Whether the panel window has drawn a frame with its docked bubble in place, so the free one can go. */
    val panelDrawn: Boolean = false,
    /**
     * The grabber's speed for the panel's next collapse, progress a second,
     * kept across whatever else happens until that collapse starts; null
     * for none.
     */
    val flung: Float? = null,
)

/** What happened to the windows. */
sealed interface WindowEvent {
    /** What the overlay wants up: the bubble when [usable], the panel too when [open]. */
    data class Want(val usable: Boolean, val open: Boolean, val wide: Boolean) : WindowEvent

    /** The grabber let the panel go at [velocity] (progress a second): the collapse to come starts at it. */
    data class Flung(val velocity: Float) : WindowEvent

    /** The panel window drew its first frame with the docked bubble where the free one is. */
    data object PanelDrawn : WindowEvent

    /** The panel's collapse landed: only the docked bubble is left of it. */
    data object PanelLanded : WindowEvent

    /** The free bubble window drew its first frame. */
    data object BubbleDrawn : WindowEvent

    /** The free bubble's exit landed. */
    data object BubbleLeft : WindowEvent
}

/** What the overlay does to its windows, in order. */
enum class WindowAct {
    /** Add the free bubble, growing in. */
    AddBubble,

    /** Add the free bubble already at rest: it takes over from the docked one, in the same place. */
    AddBubbleAtRest,

    /** The leaving bubble comes back from wherever its exit got to. */
    ShowBubble,
    HideBubble,
    RemoveBubble,

    /** Add the panel window at progress 0; it grows once it has drawn ([WindowEvent.PanelDrawn]). */
    AddPanel,

    /** Add the panel already open: the screen changed under it (decision 89), which isn't the cashier's doing. */
    AddPanelOpen,

    /** The collapsing panel grows again from wherever it got to. */
    OpenPanel,
    ClosePanel,
    RemovePanel,
}

/** The windows after an event and what to do to them; [closeVelocity] is the speed a [WindowAct.ClosePanel] starts at. */
data class WindowStep(val windows: Windows, val acts: List<WindowAct>, val closeVelocity: Float? = null)

/**
 * The order the overlay's windows come and go in, so there is never a
 * frame with no bubble and never two bubbles in different places
 * (`docs/motion.md` M1, decision 157). On a phone the panel docks the
 * bubble: its window goes up first with the docked bubble where the free
 * one is, and the free one goes once the panel has drawn; lowered, the
 * panel collapses back into the docked bubble, which slides back to where
 * the free one will be, the free one comes back there at rest, and the
 * panel goes once it has drawn. On a tablet the bubble stays and the panel
 * comes and goes beside it. A panel of the other kind (a fold opened, split
 * screen) is swapped without motion. Pure, so the order is tested on the JVM.
 */
object Handoff {
    fun next(w: Windows, event: WindowEvent): WindowStep {
        val acts = mutableListOf<WindowAct>()
        var bubble = w.bubble
        var panel = w.panel
        var drawn = w.panelDrawn
        var flung = w.flung
        var closeVelocity: Float? = null
        fun ensureBubble() {
            when (bubble) {
                Life.Gone -> acts += WindowAct.AddBubble
                Life.Leaving -> acts += WindowAct.ShowBubble
                else -> Unit
            }
            bubble = Life.Up
        }
        fun removeBubble() {
            if (bubble != Life.Gone) acts += WindowAct.RemoveBubble
            bubble = Life.Gone
        }
        return when (event) {
            is WindowEvent.Want -> {
                val wide = event.wide
                val swap = panel != Life.Gone && w.wide != wide
                if (swap) {
                    acts += WindowAct.RemovePanel
                    panel = Life.Gone
                    drawn = false
                }
                if (event.usable && event.open) {
                    when (panel) {
                        Life.Gone -> {
                            acts += if (swap) WindowAct.AddPanelOpen else WindowAct.AddPanel
                            drawn = false
                        }
                        Life.Leaving, Life.Landed -> acts += WindowAct.OpenPanel
                        Life.Up -> Unit
                    }
                    panel = Life.Up
                    // Raised again: a fling from before is spent.
                    flung = null
                    // A tablet keeps its bubble; a phone's goes once the panel has drawn its docked one.
                    if (wide) ensureBubble() else if (drawn) removeBubble()
                } else {
                    if (panel == Life.Up) {
                        acts += WindowAct.ClosePanel
                        panel = Life.Leaving
                        // The collapse takes the grabber's speed, however many frames came between the fling and now.
                        closeVelocity = flung
                        flung = null
                    }
                    if (!event.usable) {
                        if (bubble == Life.Up) {
                            acts += WindowAct.HideBubble
                            bubble = Life.Leaving
                        }
                    } else if (wide || panel == Life.Gone) {
                        // A phone's collapsing panel brings the bubble back itself, where it lands.
                        ensureBubble()
                    }
                }
                WindowStep(Windows(event.usable, wide, bubble, panel, drawn, flung), acts, closeVelocity)
            }
            is WindowEvent.Flung -> WindowStep(w.copy(flung = if (w.panel == Life.Up) event.velocity else null), acts)
            WindowEvent.PanelDrawn -> {
                drawn = true
                // The grow starts from the handoff, so the docked bubble leaves from exactly where the free one was.
                if (panel == Life.Up) {
                    if (!w.wide) removeBubble()
                    acts += WindowAct.OpenPanel
                }
                WindowStep(w.copy(bubble = bubble, panelDrawn = drawn), acts)
            }
            WindowEvent.PanelLanded -> {
                if (panel == Life.Leaving) {
                    if (!w.wide && w.usable && bubble == Life.Gone) {
                        acts += WindowAct.AddBubbleAtRest
                        bubble = Life.Up
                        panel = Life.Landed
                    } else {
                        acts += WindowAct.RemovePanel
                        panel = Life.Gone
                        drawn = false
                    }
                }
                WindowStep(w.copy(bubble = bubble, panel = panel, panelDrawn = drawn), acts)
            }
            WindowEvent.BubbleDrawn -> {
                if (panel == Life.Landed) {
                    acts += WindowAct.RemovePanel
                    panel = Life.Gone
                    drawn = false
                }
                WindowStep(w.copy(panel = panel, panelDrawn = drawn), acts)
            }
            WindowEvent.BubbleLeft -> {
                if (bubble == Life.Leaving) removeBubble()
                WindowStep(w.copy(bubble = bubble), acts)
            }
        }
    }
}
/** Where a let-go bubble goes: the nearer side, [x] the window's left edge for a window [width] wide. */
data class EdgeSnap(val left: Boolean, val x: Int)

object Edge {
    /** The side nearer the bubble's centre, and the left edge that puts the window flush against it. */
    fun snap(x: Int, width: Int, screenWidth: Int): EdgeSnap {
        val left = x + width / 2 < screenWidth / 2
        return EdgeSnap(left, if (left) 0 else screenWidth - width)
    }
}
/**
 * Where a paid moment is (`docs/motion.md` M9, decision 158): [Held] for
 * [com.monokulo.till.ui.Motion.Dwell.PAID_MS] after a payment lands, the bubble
 * green and the panel showing it paid; then [Lowering] while the panel
 * collapses into the green bubble, which fades to lit or idle only once
 * the collapse has landed. With another sale waiting the panel moves on to
 * it instead and the moment ends there.
 */
enum class PaidMoment {
    None, Held, Lowering;

    /** Whether the bubble is green and the panel holds the paid sale. */
    val paid: Boolean get() = this != None
}

enum class PaidEvent {
    /** A payment landed. */
    Paid,

    /** [com.monokulo.till.ui.Motion.Dwell.PAID_MS] went by. */
    DwellEnded,

    /** The panel started to collapse: Done, the grabber, a tap beside it, or the dwell. */
    Lowered,

    /** The panel's collapse landed, or the panel went without one. */
    ExitLanded,
}

data class PaidStep(val moment: PaidMoment, val lower: Boolean = false)

object PaidMoments {
    /** The next moment after [event], with the panel up or not and another sale waiting or not. */
    fun next(moment: PaidMoment, event: PaidEvent, panelUp: Boolean, nextWaiting: Boolean): PaidStep = when (event) {
        PaidEvent.Paid -> PaidStep(PaidMoment.Held)
        PaidEvent.DwellEnded -> when {
            moment != PaidMoment.Held -> PaidStep(moment)
            panelUp && !nextWaiting -> PaidStep(PaidMoment.Lowering, lower = true)
            else -> PaidStep(PaidMoment.None)
        }
        PaidEvent.Lowered -> PaidStep(if (moment == PaidMoment.Held) PaidMoment.Lowering else moment)
        PaidEvent.ExitLanded -> PaidStep(if (moment == PaidMoment.Lowering) PaidMoment.None else moment)
    }
}

