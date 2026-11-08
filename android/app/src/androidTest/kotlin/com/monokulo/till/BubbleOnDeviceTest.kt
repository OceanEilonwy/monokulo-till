package com.monokulo.till

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import com.monokulo.till.overlay.BubbleFace
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import uniffi.till_screens.BubbleLook

/** The bubble's pulse on a real window (M4): only with motion, only while a code waits. */
class BubbleOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    /** The free bubble in [look], pulsing as the engine says ([pulses]: `BubbleView.pulses`). */
    private fun bubble(motion: Motion, look: BubbleLook, pulses: Boolean) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            TillTheme {
                CompositionLocalProvider(LocalMotion provides motion) {
                    Box(Modifier.testTag("bubble")) { BubbleFace(look, pulses = pulses) }
                }
            }
        }
    }

    /** Whether any frame over one turn of the pulse has the accent's halo just outside the circle. */
    private fun haloWithinATurn(): Boolean {
        var seen = false
        repeat(40) {
            compose.mainClock.advanceTimeBy(40)
            val image = compose.onNodeWithTag("bubble").captureToImage()
            val pixels = image.toPixelMap()
            val centre = image.width / 2
            // The circle is 56 of the window's 88 dp: 6/88 of the width beyond its edge, straight up from the centre.
            val y = centre - (image.width * (28f + 6f) / 88f).toInt()
            val c = pixels[centre, y]
            if (c.alpha > 0.1f && c.red - c.blue > 0.3f) seen = true
        }
        return seen
    }

    @Test
    fun aWaitingCodePulses() {
        bubble(Motion.Full, BubbleLook.LIT, pulses = true)
        assertTrue(haloWithinATurn(), "the halo shows within one turn")
    }

    @Test
    fun underStillMotionNothingPulses() {
        bubble(Motion.Still, BubbleLook.LIT, pulses = true)
        assertFalse(haloWithinATurn())
    }

    @Test
    fun aPanelUpBesideItStopsThePulse() {
        // A tablet's free bubble stays up beside the open panel: it has called the cashier already, and the engine,
        // told so by PanelShown, says it doesn't pulse (`till_screens::bubble::pulses`).
        bubble(Motion.Full, BubbleLook.LIT, pulses = false)
        assertFalse(haloWithinATurn(), "the panel is up: nothing to call for")
    }

    @Test
    fun aSeenPaymentStopsThePulse() {
        // Seen: the engine says the bubble doesn't pulse; still lit, the face draws no halo.
        bubble(Motion.Full, BubbleLook.LIT, pulses = false)
        assertFalse(haloWithinATurn(), "nothing left to call the cashier for")
    }
}
