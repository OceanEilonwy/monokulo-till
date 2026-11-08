package com.monokulo.till

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.LightTokens
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.PhaseRing
import org.junit.Rule
import org.junit.Test
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlin.test.assertTrue

/** The ring's sweep on a real window, frame by frame on the test clock (M6). */
class PhaseRingOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    private val track = Fixed.Track
    private val fill = Fixed.Accent
    /** The closed ring is the theme's paid green: the light theme's here, which these rings are drawn in. */
    private val paid = LightTokens.paid

    private var phase by mutableStateOf(SalePhase.Waiting)
    private var confirmations by mutableStateOf<Int?>(null)
    private val required = 10

    private fun ring(motion: Motion, from: SalePhase, have: Int? = null) {
        phase = from
        confirmations = have
        compose.mainClock.autoAdvance = false
        compose.setContent {
            TillTheme(dark = false) {
                CompositionLocalProvider(LocalMotion provides motion) {
                    Box(Modifier.testTag("ring").padding(0.dp)) {
                        PhaseRing(phase.step, 120.dp, track = track, fill = fill, confirmations = confirmations, required = required.takeIf { confirmations != null })
                    }
                }
            }
        }
        compose.mainClock.advanceTimeByFrame()
    }

    private fun frame(): ImageBitmap = compose.onNodeWithTag("ring").captureToImage()

    /** The colour on the middle of the stroke at [degrees] clockwise from the top. */
    private fun ImageBitmap.at(degrees: Double): Color {
        val side = width.toFloat()
        val stroke = side * 0.115f
        val radius = side / 2 - (stroke / 2 + side * 0.07f)
        val angle = Math.toRadians(degrees)
        val x = (side / 2 + radius * sin(angle)).toInt()
        val y = (side / 2 - radius * cos(angle)).toInt()
        return toPixelMap()[x, y]
    }

    private fun assertColour(expected: Color, actual: Color, what: String) {
        val close = abs(expected.red - actual.red) < 0.08f && abs(expected.green - actual.green) < 0.08f && abs(expected.blue - actual.blue) < 0.08f
        assertTrue(close, "$what: expected $expected, got $actual")
    }

    @Test
    fun aRingThatAppearsAtSeenIsAlreadyItsShortGreenArc() {
        ring(Motion.Full, SalePhase.Seen, have = 0)
        val image = frame()
        assertColour(paid, image.at(20.0), "0.12 of the ring, green, on the first frame")
        assertColour(track, image.at(90.0), "and no more")
    }

    @Test
    fun waitingIsAStillOrangeQuarter() {
        ring(Motion.Full, SalePhase.Waiting)
        val first = frame()
        assertColour(fill, first.at(60.0), "the quarter in the fill")
        compose.mainClock.advanceTimeBy(800)
        val later = frame()
        assertColour(fill, later.at(60.0), "still there")
        assertColour(track, later.at(135.0), "and it hasn't moved on")
    }

    @Test
    fun aConfirmationLengthensTheArcOnTheSweep() {
        ring(Motion.Full, SalePhase.Confirming, have = 1)
        confirmations = 4
        // A whole turn: the arc is back at the top, and the 600 ms sweep long done.
        compose.mainClock.advanceTimeBy(Motion.SPIN_MS.toLong())
        val settled = frame()
        assertColour(paid, settled.at(140.0), "4 of 10 is 0.47 of the ring")
        assertColour(track, settled.at(220.0), "and no more")
    }

    @Test
    fun theArcTurnsWhileConfirming() {
        ring(Motion.Full, SalePhase.Confirming, have = 4)
        assertColour(paid, frame().at(90.0), "from the top to 170 degrees")
        compose.mainClock.advanceTimeBy(Motion.SPIN_MS / 2L)
        val half = frame()
        assertColour(track, half.at(90.0), "half a turn later the top right is empty")
        assertColour(paid, half.at(270.0), "and the arc is on the other side")
    }

    @Test
    fun paidClosesTheRingInThePaidColourAndStops() {
        ring(Motion.Full, SalePhase.Confirming, have = 9)
        compose.mainClock.advanceTimeBy(300)
        phase = SalePhase.Paid
        compose.mainClock.advanceTimeBy(2_000)
        val image = frame()
        for (degrees in listOf(45.0, 135.0, 225.0, 315.0)) assertColour(paid, image.at(degrees), "closed at $degrees")
    }

    /** How far [c] is from the nearest colour between the track and [to]: a stroke fading over the track is on that line. */
    private fun offLine(c: Color, to: Color): Float {
        val d = floatArrayOf(to.red - track.red, to.green - track.green, to.blue - track.blue)
        val v = floatArrayOf(c.red - track.red, c.green - track.green, c.blue - track.blue)
        val a = ((v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / (d[0] * d[0] + d[1] * d[1] + d[2] * d[2])).coerceIn(0f, 1f)
        return maxOf(abs(v[0] - a * d[0]), abs(v[1] - a * d[1]), abs(v[2] - a * d[2]))
    }

    /** Every frame of a change, the arc where it's filled on both sides is the fill or the paid colour over the track, never a mix of the two (decision 190). */
    private fun assertNoMixedHue(what: String) {
        repeat(60) { frame ->
            val c = frame().at(45.0)
            val off = minOf(offLine(c, fill), offLine(c, paid))
            assertTrue(off < 0.06f, "$what, frame $frame: $c is neither colour over the track (off by $off)")
            compose.mainClock.advanceTimeByFrame()
        }
    }

    @Test
    fun seenAndTheNextSaleNeverDrawTheRingInAMixOfTheirColours() {
        ring(Motion.Full, SalePhase.Waiting)
        phase = SalePhase.Confirming
        confirmations = 1
        assertNoMixedHue("turning green on the way")
        phase = SalePhase.Paid
        assertNoMixedHue("closing on paid")
        assertColour(paid, frame().at(45.0), "closed in the paid colour")
        phase = SalePhase.Waiting
        confirmations = null
        assertNoMixedHue("the next sale's ring")
        assertColour(fill, frame().at(45.0), "back to the fill")
    }

    @Test
    fun underStillMotionTheArcNeverTurnsAndALengthIsDrawnOnTheNextFrame() {
        ring(Motion.Still, SalePhase.Confirming, have = 1)
        confirmations = 4
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertColour(paid, frame().at(140.0), "snapped to 4 of 10")
        compose.mainClock.advanceTimeBy(Motion.SPIN_MS / 2L)
        val later = frame()
        assertColour(paid, later.at(90.0), "still from the top")
        assertColour(track, later.at(220.0), "and no further")
    }
}
