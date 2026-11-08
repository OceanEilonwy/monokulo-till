package com.monokulo.till.ui

import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.monokulo.till.SaleText
import com.monokulo.till.core.SalePhase
import com.monokulo.till.sales.TillSale

/**
 * How much of the ring a phase fills, from the top, clockwise: a quarter
 * while it waits, the whole ring paid. Seen and confirming, it is the
 * payment's way to the store's threshold (Board 5): [SEEN_FILL] seen, and
 * the rest of the ring shared out over the [required] confirmations. With
 * no count from Monokulo, a quarter a step.
 */
fun ringFill(phase: SalePhase, confirmations: Int? = null, required: Int? = null): Float {
    if (ringSpins(phase) && required != null && required > 0) {
        val have = (confirmations ?: 0).coerceIn(0, required)
        return SEEN_FILL + (1f - SEEN_FILL) * have / required
    }
    return SaleText.step(phase) / 4f
}

/** Whether the arc turns: the payment is on the network, seen or confirming, and not yet paid. */
fun ringSpins(phase: SalePhase): Boolean = phase == SalePhase.Seen || phase == SalePhase.Confirming

/** Whether the arc is the paid green: from the moment the payment is seen. Orange means waiting for money. */
fun ringGreen(phase: SalePhase): Boolean = ringSpins(phase) || phase == SalePhase.Paid

/** How much of the ring a seen payment with no confirmation yet fills. */
const val SEEN_FILL = 0.12f

/** [sale]'s ring: its phase, and its payment's confirmations when Monokulo said. */
@Composable
fun PhaseRing(
    sale: TillSale,
    size: Dp,
    modifier: Modifier = Modifier,
    track: Color = mk.chip,
    fill: Color = mk.accent,
    paid: Color = mk.paid,
) = PhaseRing(sale.phase, size, modifier, track, fill, paid, sale.confirmations, sale.confirmationsRequired)

/**
 * The design's phase ring (its Chips and status rows): an open ring whose
 * arc is a still [fill] quarter while the sale waits, then, once the payment
 * is seen, the [paid] green turning a lap every `motion.spin` and growing
 * with each of its [confirmations] towards [required] ([ringFill]), until
 * paid closes it and it stops. Decoration: the words beside it say the phase.
 *
 * A change of length sweeps (`docs/motion.md` M6, `motion.sweep`): the arc
 * grows from where it is, so a second step mid-sweep just carries on. The
 * arc's two colours are two strokes, never a blend of them (decision 190):
 * the one going fades out under `motion.fadeQuick` before the one coming
 * fades in (`motion.fadeThrough`), so on the way to seen, or from a paid
 * ring to the next sale's, the ring is only ever [fill], [paid], or one of
 * them fading over the [track]. Paid, the turn stops where the arc is and the
 * arc closes from there, so it never jumps back to the top; under reduced
 * motion it never turns, and only its length steps.
 * The first composition draws the phase's own fill, so a ring that appears
 * at Seen doesn't sweep in. Every animated value, the turn too, is read only
 * in the draw lambda: a step or a turn redraws the ring and recomposes
 * nothing, and nothing is allocated per frame.
 */
@Composable
fun PhaseRing(
    phase: SalePhase,
    size: Dp,
    modifier: Modifier = Modifier,
    track: Color = mk.chip,
    fill: Color = mk.accent,
    paid: Color = mk.paid,
    confirmations: Int? = null,
    required: Int? = null,
) {
    val m = motion
    val fraction by animateFloatAsState(ringFill(phase, confirmations, required), m.sweep.fraction, label = "ring")
    val green = ringGreen(phase)
    val fillShown by animateFloatAsState(if (green) 0f else 1f, (if (green) m.fadeQuick else m.fadeThrough).fraction, label = "fill")
    val paidShown by animateFloatAsState(if (green) 1f else 0f, (if (green) m.fadeThrough else m.fadeQuick).fraction, label = "paid")
    // Only while the payment is on its way: a waiting, paid or ended ring runs no loop.
    val turn = m.spin
    val spin = if (turn != null && ringSpins(phase)) {
        rememberInfiniteTransition(label = "ring turn").animateFloat(0f, 360f, turn, label = "turn")
    } else {
        null
    }
    // Where the arc was when it stopped turning, so it closes or fades there rather than jumping to 12 o'clock: written by the draw.
    val heldAt = remember { FloatArray(1) }
    val density = LocalDensity.current
    val strokes = remember(size, density) { RingStrokes(with(density) { size.toPx() }) }
    Canvas(modifier.size(size).clearAndSetSemantics {}) {
        // Turning, it holds the turn's angle; with animations off, the still arc starts at the top again.
        if (spin != null) heldAt[0] = spin.value else if (turn == null) heldAt[0] = 0f
        val inset = strokes.inset
        val arcSize = Size(this.size.width - 2 * inset, this.size.height - 2 * inset)
        val topLeft = Offset(inset, inset)
        drawArc(track, 0f, 360f, useCenter = false, topLeft = topLeft, size = arcSize, style = strokes.track)
        val sweep = 360f * fraction
        for (stroke in 0..1) {
            val colour = if (stroke == 0) fill else paid
            val shown = if (stroke == 0) fillShown else paidShown
            if (shown <= 0f) continue
            // The waiting quarter stands still at the top; the green arc turns.
            val start = if (stroke == 0) -90f else -90f + heldAt[0]
            if (sweep > 0f) drawArc(colour, start, sweep, useCenter = false, topLeft = topLeft, size = arcSize, alpha = shown, style = strokes.arc)
        }
    }
}

/** The ring's two strokes at one size, made once rather than on every frame of a sweep. */
private class RingStrokes(side: Float) {
    private val width = side * 0.115f
    val inset = width / 2 + side * 0.07f
    val track = Stroke(width)
    val arc = Stroke(width, cap = StrokeCap.Round)
}

/** The ring at the design's row size. */
val RingRowSize = 36.dp
