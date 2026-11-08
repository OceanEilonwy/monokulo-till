package com.monokulo.till

import androidx.compose.ui.graphics.Color
import com.monokulo.till.overlay.BubbleInks
import com.monokulo.till.ui.Fixed
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.test.Test
import kotlin.test.assertTrue

/**
 * The logo bubble's fixed colours (`docs/motion.md` §12.5, decision 267)
 * hold the contrast the design claims for what a cashier reads off the
 * bubble: the white tick on each of the lens's greens and the ink
 * segment on the pale track it chases round, each at least WCAG's 3:1 for
 * graphics, on the white disc and on the dark one night mode gives it.
 * The bubble sits over another app, so these are [Fixed], not the theme's.
 */
class BubbleColoursTest {
    private fun channel(c: Float): Double = if (c <= 0.03928f) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4)

    private fun luminance(c: Color): Double = 0.2126 * channel(c.red) + 0.7152 * channel(c.green) + 0.0722 * channel(c.blue)

    private fun contrast(a: Color, b: Color): Double {
        val la = luminance(a)
        val lb = luminance(b)
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    @Test
    fun `the white tick reads on every green of the lens`() {
        for (green in listOf(Fixed.PaidLight, Fixed.PaidMid, Fixed.Paid)) {
            val ratio = contrast(Fixed.White, green)
            assertTrue(ratio >= 3.0, "white on $green is ${"%.2f".format(ratio)}:1, under 3:1")
        }
    }

    @Test
    fun `the lens darkens toward its core, as the orange mark does`() {
        assertTrue(luminance(Fixed.PaidLight) > luminance(Fixed.PaidMid), "the light facets are lighter than the middle ones")
        assertTrue(luminance(Fixed.PaidMid) > luminance(Fixed.Paid), "and the middle ones lighter than the core")
    }

    @Test
    fun `the ink segment reads on the track it chases round`() {
        val ratio = contrast(Fixed.Ink, Fixed.ChaseTrack)
        assertTrue(ratio >= 3.0, "ink on the chase track is ${"%.2f".format(ratio)}:1, under 3:1")
        assertTrue(contrast(Fixed.ChaseTrack, Fixed.White) < 3.0, "the track is pale: it is not itself a mark on the white glass")
    }

    @Test
    fun `in either mode the border's ink and idle grey read on the disc, and the track is quiet`() {
        for ((mode, inks) in listOf("light" to BubbleInks.Light, "dark" to BubbleInks.Dark)) {
            for ((what, colour) in listOf("ink" to inks.ink, "idle border" to inks.idle)) {
                val ratio = contrast(colour, inks.disc)
                assertTrue(ratio >= 3.0, "$mode: the $what on the disc is ${"%.2f".format(ratio)}:1, under 3:1")
            }
            val chase = contrast(inks.ink, inks.track)
            assertTrue(chase >= 3.0, "$mode: the chasing segment on its track is ${"%.2f".format(chase)}:1, under 3:1")
            assertTrue(contrast(inks.track, inks.disc) < 3.0, "$mode: the track is not itself a mark on the disc")
        }
    }

    @Test
    fun `light mode keeps its colours and dark mode takes the design's disc`() {
        assertTrue(BubbleInks.of(dark = false) === BubbleInks.Light && BubbleInks.of(dark = true) === BubbleInks.Dark)
        assertTrue(BubbleInks.Light.disc == Fixed.White && BubbleInks.Light.ink == Fixed.Ink, "light mode is as it was")
        assertTrue(BubbleInks.Dark.disc == Fixed.BubbleDark, "the dark disc is the design's #2A2825")
    }
}
