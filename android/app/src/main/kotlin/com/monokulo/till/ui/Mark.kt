package com.monokulo.till.ui

import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathMeasure
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp

/**
 * The mark's measures that aren't paths (`docs/motion.md` §12.5 and
 * §13.5), in mark units: the SVG's 64-unit box, before the lens's own
 * 1.42 scale. Pure, so the rings are tested on the JVM.
 */
object MarkGeometry {
    /** The lens's centre and radius: every facet is clipped to it. */
    const val CENTRE_X = 30f
    const val CENTRE_Y = 28f
    const val LENS_RADIUS = 15.75f

    /** The border round the lens: its radius and stroke. */
    const val BORDER_RADIUS = 18f
    const val BORDER_STROKE = 4.5f

    /** The lens's rings, core to rim, which the bubble colours one after another ([Motion.spread]). */
    const val RINGS = 4

    /** The ring the lens's base circle, under the facets, is coloured with: the middle twelve's, whose shade it is. */
    const val BASE_RING = 2

    /** Each facet and its shade (0 #ff6600, 1 #e85d00, 2 #c24e00), in the SVG's order: the core, the inner six, the middle twelve, the rim twelve. */
    val facets: List<Pair<String, Int>> = listOf(
        "M30,23L34.33,25.5L34.33,30.5L30,33L25.67,30.5L25.67,25.5Z" to 2,
        "M30,23L34.33,25.5L35.5,18.47Z" to 1,
        "M34.33,25.5L34.33,30.5L41,28Z" to 1,
        "M34.33,30.5L30,33L35.5,37.53Z" to 1,
        "M30,33L25.67,30.5L24.5,37.53Z" to 1,
        "M25.67,30.5L25.67,25.5L19,28Z" to 1,
        "M25.67,25.5L30,23L24.5,18.47Z" to 1,
        "M30,23L24.5,18.47L30,4Z" to 0,
        "M30,23L30,4L35.5,18.47Z" to 0,
        "M34.33,25.5L35.5,18.47L50.78,16Z" to 0,
        "M34.33,25.5L50.78,16L41,28Z" to 0,
        "M34.33,30.5L41,28L50.78,40Z" to 0,
        "M34.33,30.5L50.78,40L35.5,37.53Z" to 0,
        "M30,33L35.5,37.53L30,52Z" to 0,
        "M30,33L30,52L24.5,37.53Z" to 0,
        "M25.67,30.5L24.5,37.53L9.22,40Z" to 0,
        "M25.67,30.5L9.22,40L19,28Z" to 0,
        "M25.67,25.5L19,28L9.22,16Z" to 0,
        "M25.67,25.5L9.22,16L24.5,18.47Z" to 0,
        "M35.5,18.47L30,4L42,7.22Z" to 1,
        "M35.5,18.47L42,7.22L50.78,16Z" to 1,
        "M41,28L50.78,16L54,28Z" to 1,
        "M41,28L54,28L50.78,40Z" to 1,
        "M35.5,37.53L50.78,40L42,48.78Z" to 1,
        "M35.5,37.53L42,48.78L30,52Z" to 1,
        "M24.5,37.53L30,52L18,48.78Z" to 1,
        "M24.5,37.53L18,48.78L9.22,40Z" to 1,
        "M19,28L9.22,40L6,28Z" to 1,
        "M19,28L6,28L9.22,16Z" to 1,
        "M24.5,18.47L9.22,16L18,7.22Z" to 1,
        "M24.5,18.47L18,7.22L30,4Z" to 1,
    )

    /** The ring the facet at [index] of [facets] is in, from the list's order: 1 core, 6 inner, 12 middle, 12 rim. */
    fun ring(index: Int): Int = when {
        index < 1 -> 0
        index < 7 -> 1
        index < 19 -> 2
        else -> 3
    }

    /** Each ring's shade in the SVG: the core darkest, the middle twelve lightest. */
    fun shade(ring: Int): Int = when (ring) {
        0 -> 2
        2 -> 0
        else -> 1
    }
}

/**
 * The lens's colour per ring, core (0) to rim (3), read each time the mark
 * is drawn: a [Color] is an inline value, so a look reading animated
 * colours allocates nothing per frame.
 */
fun interface LensLook {
    operator fun get(ring: Int): Color

    companion object {
        /** The logo's own orange. */
        val Orange = shades(listOf(Color(0xFFFF6600), Color(0xFFE85D00), Color(0xFFC24E00)))

        /** The same shades through a greyscale filter (Rec. 709 luma), for the idle bubble. */
        val Grey = shades(listOf(Color(0xFF7F7F7F), Color(0xFF747474), Color(0xFF616161)))

        /** The paid bubble's: the theme's fixed greens, lightest to the core's (decision 271). */
        val Green = shades(listOf(Fixed.PaidLight, Fixed.PaidMid, Fixed.Paid))

        /** A look from three shades, light to dark, each ring in the SVG's shade for it. */
        private fun shades(light: List<Color>): LensLook = LensLook { ring -> light[MarkGeometry.shade(ring)] }
    }
}

/**
 * The Monokulo mark (the design's Logo and LoadingMark parts): an orange
 * lens of facets in a ring, drawn from the same paths as the web's SVG.
 */
private object MarkPaths {
    private fun p(d: String): Path = PathParser().parsePathString(d).toPath()

    /** Each facet's path, in [MarkGeometry.facets]'s order. */
    val facets: List<Path> = MarkGeometry.facets.map { (d, _) -> p(d) }

    /** The ring of each facet, by index into [facets]. */
    val rings: IntArray = IntArray(facets.size, MarkGeometry::ring)

    val core: Path = p("M30,23L34.33,25.5L34.33,30.5L30,33L25.67,30.5L25.67,25.5Z")

    /** The loading mark's glints, each with its delay in ms (one turn is [GLINT_MS]). */
    val spokes: List<Pair<Path, Int>> = listOf(
        "M35.5,18.47L30,4L42,7.22Z" to 0,
        "M30,23L30,4L35.5,18.47Z" to 92,
        "M30,23L34.33,25.5L35.5,18.47Z" to 181,
        "M35.5,37.53L42,48.78L30,52Z" to 701,
        "M30,33L35.5,37.53L30,52Z" to 790,
        "M34.33,30.5L30,33L35.5,37.53Z" to 882,
        "M24.5,18.47L9.22,16L18,7.22Z" to 1399,
        "M25.67,25.5L9.22,16L24.5,18.47Z" to 1491,
        "M25.67,25.5L30,23L24.5,18.47Z" to 1583,
        "M41,28L50.78,16L54,28Z" to 2100,
        "M34.33,25.5L50.78,16L41,28Z" to 2192,
        "M34.33,25.5L34.33,30.5L41,28Z" to 2281,
        "M24.5,37.53L18,48.78L9.22,40Z" to 2801,
        "M25.67,30.5L24.5,37.53L9.22,40Z" to 2890,
        "M30,33L25.67,30.5L24.5,37.53Z" to 2982,
        "M35.5,37.53L50.78,40L42,48.78Z" to 3499,
        "M34.33,30.5L50.78,40L35.5,37.53Z" to 3591,
    ).map { (d, delay) -> p(d) to delay }

    val lens: Path = Path().apply {
        addOval(Rect(Offset(MarkGeometry.CENTRE_X, MarkGeometry.CENTRE_Y), MarkGeometry.LENS_RADIUS))
    }

    val centre = Offset(MarkGeometry.CENTRE_X, MarkGeometry.CENTRE_Y)
    val border = Stroke(MarkGeometry.BORDER_STROKE)
}

const val GLINT_MS = 4200
const val CORE_MS = 700

private val Glint = Color(0xFFFFF3E8)

/** The glint's opacity [p] of the way through its turn: `mk-glint` in the web's CSS. */
fun glintAlpha(p: Float): Float = when {
    p < 0.01f -> 0.95f * p / 0.01f
    p < 0.09f -> 0.95f
    p < 0.12f -> 0.95f * (0.12f - p) / 0.03f
    else -> 0f
}

/** The core's opacity [p] of the way through its turn: `mk-core`. */
fun coreAlpha(p: Float): Float = when {
    p < 0.39f -> 0f
    p < 0.48f -> 0.95f * (p - 0.39f) / 0.09f
    p < 0.78f -> 0.95f
    p < 0.96f -> 0.95f * (0.96f - p) / 0.18f
    else -> 0f
}

/** [block] drawn in mark units: the 64-unit box fitted to the canvas, the lens scaled about its centre as the SVG does. */
private inline fun DrawScope.inMark(block: DrawScope.() -> Unit) {
    scale(size.minDimension / 64f, pivot = Offset.Zero) {
        translate(32f, 32f) {
            scale(1.42f, pivot = Offset.Zero) {
                translate(-MarkGeometry.CENTRE_X, -MarkGeometry.CENTRE_Y) { block() }
            }
        }
    }
}

/** The lens's base and facets in [lens]'s colours (to be drawn inside the lens's clip). */
private fun DrawScope.lens(lens: LensLook) {
    drawCircle(lens[MarkGeometry.BASE_RING], MarkGeometry.LENS_RADIUS, MarkPaths.centre)
    val facets = MarkPaths.facets
    for (i in facets.indices) drawPath(facets[i], lens[MarkPaths.rings[i]])
}

private fun DrawScope.mark(ring: Color, grey: Boolean, time: Float?) = inMark {
    clipPath(MarkPaths.lens) {
        lens(if (grey) LensLook.Grey else LensLook.Orange)
        if (time != null) {
            drawPath(MarkPaths.core, Glint, alpha = coreAlpha((time % CORE_MS) / CORE_MS))
            MarkPaths.spokes.forEach { (path, delay) ->
                val p = (((time - delay) % GLINT_MS) + GLINT_MS) % GLINT_MS / GLINT_MS
                val a = glintAlpha(p)
                if (a > 0f) drawPath(path, Glint, alpha = a)
            }
        }
    }
    drawCircle(ring, MarkGeometry.BORDER_RADIUS, MarkPaths.centre, style = MarkPaths.border)
}

/** The Monokulo logo; [label] null when it's decoration. */
@Composable
fun Logo(size: Dp, modifier: Modifier = Modifier, ring: Color = mk.ink, grey: Boolean = false, label: String? = null) {
    val described = if (label != null) Modifier.semantics { contentDescription = label } else Modifier
    Canvas(modifier.size(size).then(described)) { mark(ring, grey, null) }
}

/**
 * The loading mark: the logo with glints running round it, which says
 * "watching" or "waiting". Still when the person has asked for less
 * motion ([Motion.Still]), as the web's `prefers-reduced-motion` does.
 */
@Composable
fun LoadingMark(size: Dp, label: String, modifier: Modifier = Modifier, ring: Color = mk.ink) {
    val described = modifier.size(size).semantics { contentDescription = label }
    val glints = motion.glints
    if (glints == null) {
        Canvas(described) { mark(ring, false, null) }
        return
    }
    // Read in the draw phase only: each frame redraws, nothing recomposes.
    val time = rememberInfiniteTransition(label = "loading mark").animateFloat(
        initialValue = 0f,
        targetValue = GLINT_MS.toFloat(),
        animationSpec = glints,
        label = "glints",
    )
    Canvas(described) { mark(ring, false, time.value) }
}

/**
 * Till's status mark, the app icon's terminal as Monokulo's "Keys count"
 * canvas draws it, in its round form (`TillRound`): for the bubble, which
 * is a circle. The screen alone, scaled 1.22 about the disc's centre, with
 * no keys and no outer ring; the progress runs clockwise round the
 * screen's own rounded-square border from 12 o'clock. Measures are on the
 * canvas's 120 grid, so they match the icon's; the disc behind it is the
 * caller's.
 */
object TillGeometry {
    /** The screen, which the facets are clipped to. */
    const val SCREEN = "M33.0,29.5H63.0A6.5,6.5 0 0 1 69.5,36.0V66.0A6.5,6.5 0 0 1 63.0,72.5H33.0A6.5,6.5 0 0 1 26.5,66.0V36.0A6.5,6.5 0 0 1 33.0,29.5Z"

    /** The screen's eight triangles round its centre square, in the warm shade. */
    const val FACETS = "M43,46H53L48,39.5ZM53,46V56L59.5,51ZM53,56H43L48,62.5ZM43,56V46L36.5,51ZM48,39.5L26.5,29.5H69.5ZM59.5,51L69.5,29.5V72.5ZM48,62.5L69.5,72.5H26.5ZM36.5,51L26.5,72.5V29.5Z"

    /** The centre square, in the deep shade. */
    const val CENTRE = "M43,46H53V56H43Z"

    /** The screen's border, from 12 o'clock, clockwise: the track the progress runs round. */
    const val BORDER = "M48,27H63A9,9 0 0 1 72,36V66A9,9 0 0 1 63,75H33A9,9 0 0 1 24,66V36A9,9 0 0 1 33,27Z"
    const val BORDER_STROKE = 5f

    /** The paid screen's tick, drawn over it inside its clip. */
    const val TICK = "M37.5,51.5 l7,7 l14,-15"
    const val TICK_STROKE = 5f

    /** The disc's box on the grid (the canvas's viewBox `10 10 100 100`), and the screen's scale in it, about the screen's centre (48, 51). */
    const val BOX = 100f
    const val SCALE = 1.22f
    const val SCREEN_X = 48f
    const val SCREEN_Y = 51f

    /** How much of the border the running segment is while the code waits to be scanned. */
    const val RUN_SHARE = 0.22f

    /** The steps of progress after the first: seen, confirming, paid. */
    const val STEPS = 3

    /**
     * How many of the [STEPS] the border is filled to for a bubble that is
     * [idle] or [paid], or lit at the sale's ring [step] (1 waiting, 2
     * seen, 3 confirming, 4 paid; 0 for a sale that ended unpaid): the
     * canvas's 1/3 at seen, 2/3 at confirming, the whole once paid.
     */
    fun steps(idle: Boolean, paid: Boolean, step: Int): Int = when {
        idle -> 0
        paid || step >= 4 -> STEPS
        step == 3 -> 2
        step == 2 -> 1
        else -> 0
    }

    /** Whether the orange segment runs round the border: the code is up and no payment is on its way yet. */
    fun runs(idle: Boolean, paid: Boolean, step: Int): Boolean = !idle && !paid && step < 2
}

private object TillPaths {
    private fun p(d: String): Path = PathParser().parsePathString(d).toPath()

    val screen: Path = p(TillGeometry.SCREEN)
    val facets: Path = p(TillGeometry.FACETS)
    val centre: Path = p(TillGeometry.CENTRE)
    val border: Path = p(TillGeometry.BORDER)
    private val tickWhole: Path = p(TillGeometry.TICK)

    val borderStroke = Stroke(TillGeometry.BORDER_STROKE)
    val segmentStroke = Stroke(TillGeometry.BORDER_STROKE, cap = StrokeCap.Round)
    val tickStroke = Stroke(TillGeometry.TICK_STROKE, cap = StrokeCap.Round, join = StrokeJoin.Round)

    private val borderMeasure = PathMeasure().apply { setPath(border, forceClosed = false) }
    private val borderLength = borderMeasure.length
    private val tickMeasure = PathMeasure().apply { setPath(tickWhole, forceClosed = false) }
    private val tickLength = tickMeasure.length

    /** The two kept paths a frame refills: the border's segment, and the tick's trim (the main thread's alone, where every draw runs). */
    private val segment = Path()
    private val trimmed = Path()

    /** The border from [from] to [to] of its length, both in 0..1, into the one kept path. */
    fun segment(from: Float, to: Float): Path {
        segment.rewind()
        borderMeasure.getSegment(borderLength * from, borderLength * to, segment, startWithMoveTo = true)
        return segment
    }

    fun tick(trim: Float): Path {
        if (trim >= 1f) return tickWhole
        trimmed.rewind()
        tickMeasure.getSegment(0f, tickLength * trim, trimmed, startWithMoveTo = true)
        return trimmed
    }
}

/** [block] on the canvas's 120 grid: the 100-unit box fitted to the canvas, the screen scaled about its centre onto the box's. */
private inline fun DrawScope.inTill(block: DrawScope.() -> Unit) {
    scale(size.minDimension / TillGeometry.BOX, pivot = Offset.Zero) {
        translate(TillGeometry.BOX / 2, TillGeometry.BOX / 2) {
            scale(TillGeometry.SCALE, pivot = Offset.Zero) {
                translate(-TillGeometry.SCREEN_X, -TillGeometry.SCREEN_Y) { block() }
            }
        }
    }
}

/**
 * Till's round status mark: the screen in [lens]'s colours (the middle
 * twelve's shade the screen, the inner six's the triangles, the core's the
 * centre square: the icon's three shades); the border's [track], and over
 * it the [fill] (0..1) of the way round from 12 o'clock in [fillInk], and
 * the running segment ([TillGeometry.RUN_SHARE] of the border) in orange
 * at [runAlpha], [runStart] (0..1) of the way round; and the white tick
 * drawn to [tick] of its length at [tickAlpha]. Every argument is a value
 * the caller reads in its draw, so a frame redraws and recomposes nothing,
 * and nothing here allocates: the paths and strokes are made once, and the
 * segment and the tick's trim are refilled into kept paths.
 */
fun DrawScope.till(
    lens: LensLook,
    track: Color,
    fill: Float,
    fillInk: Color,
    runStart: Float,
    runAlpha: Float,
    tick: Float,
    tickAlpha: Float,
    run: Color = Fixed.Accent,
) = inTill {
    clipPath(TillPaths.screen) {
        drawPath(TillPaths.screen, lens[MarkGeometry.BASE_RING])
        drawPath(TillPaths.facets, lens[1])
        drawPath(TillPaths.centre, lens[0])
        if (tick > 0f && tickAlpha > 0f) drawPath(TillPaths.tick(tick), Fixed.White, alpha = tickAlpha, style = TillPaths.tickStroke)
    }
    drawPath(TillPaths.border, track, style = TillPaths.borderStroke)
    if (fill >= 1f) {
        drawPath(TillPaths.border, fillInk, style = TillPaths.borderStroke)
    } else if (fill > 0f) {
        drawPath(TillPaths.segment(0f, fill), fillInk, style = TillPaths.segmentStroke)
    }
    if (runAlpha > 0f) {
        val end = runStart + TillGeometry.RUN_SHARE
        if (end <= 1f) {
            drawPath(TillPaths.segment(runStart, end), run, alpha = runAlpha, style = TillPaths.segmentStroke)
        } else {
            drawPath(TillPaths.segment(runStart, 1f), run, alpha = runAlpha, style = TillPaths.segmentStroke)
            drawPath(TillPaths.segment(0f, end - 1f), run, alpha = runAlpha, style = TillPaths.segmentStroke)
        }
    }
}

