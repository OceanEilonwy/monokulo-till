package com.monokulo.till

import com.monokulo.till.overlay.BubbleMark
import com.monokulo.till.overlay.BubblePadding
import com.monokulo.till.overlay.BubbleSize
import com.monokulo.till.overlay.Panel
import com.monokulo.till.overlay.haloRadius
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.LensLook
import com.monokulo.till.ui.MarkGeometry
import com.monokulo.till.ui.TillGeometry
import com.monokulo.till.ui.Motion
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * The mark's rings, the bubble's terminal and greens (`docs/motion.md` §12.5, §13.5):
 * the facets' list order is the lens's rings from the core out, which the
 * bubble's spread colours one after another, so the list must stay the
 * SVG's 1 + 6 + 12 + 12 and each ring one shade of it.
 */
class MarkGeometryTest {
    @Test
    fun `the facets are the core, the inner six, the middle twelve and the rim twelve, in that order`() {
        val facets = MarkGeometry.facets
        assertEquals(31, facets.size)
        val sizes = facets.indices.groupingBy(MarkGeometry::ring).eachCount()
        assertEquals(mapOf(0 to 1, 1 to 6, 2 to 12, 3 to 12), sizes)
        assertEquals(facets.indices.map(MarkGeometry::ring), facets.indices.map(MarkGeometry::ring).sorted(), "rings run core to rim")
        assertEquals(Motion.SPREAD_RINGS, MarkGeometry.RINGS, "the spread has a preset for each ring")
    }

    @Test
    fun `each ring is the one shade the SVG gives its facets, the base circle the middle twelve's`() {
        MarkGeometry.facets.forEachIndexed { i, (_, shade) ->
            assertEquals(shade, MarkGeometry.shade(MarkGeometry.ring(i)), "facet $i")
        }
        assertEquals(0, MarkGeometry.shade(MarkGeometry.BASE_RING), "the base circle under the facets is the light shade, as the SVG draws it")
    }

    @Test
    fun `the green lens is the fixed greens, the core darkest`() {
        assertEquals(Fixed.Paid, LensLook.Green[0])
        assertEquals(Fixed.PaidMid, LensLook.Green[1])
        assertEquals(Fixed.PaidLight, LensLook.Green[2])
        assertEquals(Fixed.PaidMid, LensLook.Green[3])
    }

    /**
     * The bubble's 56 dp disc is the canvas's 100-unit disc: the screen's
     * border, its outer edge (the 24..72 square and half its stroke)
     * scaled 1.22, leaves about 10 dp of the disc beside it, and the
     * pulse's halo, 16 dp past the disc at its widest, still ends inside the
     * bubble's window.
     */
    @Test
    fun `the bubble's rim is about 10 dp and its halo fits its window`() {
        assertEquals(BubbleSize, BubbleMark, "the mark fills the disc: the canvas draws the screen small in it")
        val edge = BubbleMark.value / TillGeometry.BOX * TillGeometry.SCALE * (24f + TillGeometry.BORDER_STROKE / 2)
        val rim = BubbleSize.value / 2 - edge
        assertEquals(9.9f, rim, 0.5f, "the disc beside the border is $rim dp")
        assertEquals(Panel.BUBBLE_WINDOW_DP.toFloat(), BubbleSize.value + 2 * BubblePadding.value, "the window is the disc and its padding")
        val halo = haloRadius(BubbleSize.value, BubblePadding.value, p = 1f)
        assertEquals(Panel.BUBBLE_WINDOW_DP / 2f, halo, "the halo's widest ring touches the window's edge, unclipped")
    }

    /** The canvas's states on Till's: idle and waiting fill nothing, seen a third, confirming two thirds, paid the whole border. */
    @Test
    fun `the border fills a third a step, the sale ring's steps`() {
        assertEquals(0, TillGeometry.steps(idle = true, paid = false, step = 3), "idle shows no progress whatever the sale")
        assertEquals(0, TillGeometry.steps(idle = false, paid = false, step = 1), "waiting")
        assertEquals(0, TillGeometry.steps(idle = false, paid = false, step = 0), "a sale that ended unpaid but still lit")
        assertEquals(1, TillGeometry.steps(idle = false, paid = false, step = 2), "seen")
        assertEquals(2, TillGeometry.steps(idle = false, paid = false, step = 3), "confirming")
        assertEquals(TillGeometry.STEPS, TillGeometry.steps(idle = false, paid = true, step = 3), "paid, the look says so before the sale's step catches up")
        assertEquals(TillGeometry.STEPS, TillGeometry.steps(idle = false, paid = false, step = 4))
    }

    @Test
    fun `the orange segment runs only while the code waits for its scan`() {
        assertTrue(TillGeometry.runs(idle = false, paid = false, step = 1))
        assertTrue(TillGeometry.runs(idle = false, paid = false, step = 0))
        assertFalse(TillGeometry.runs(idle = true, paid = false, step = 1), "idle")
        assertFalse(TillGeometry.runs(idle = false, paid = false, step = 2), "seen: the fill says so")
        assertFalse(TillGeometry.runs(idle = false, paid = false, step = 3), "confirming")
        assertFalse(TillGeometry.runs(idle = false, paid = true, step = 4), "paid")
    }

    @Test
    fun `the screen is the icon's, the border its own rounded square`() {
        assertEquals(1.22f, TillGeometry.SCALE)
        assertEquals(0.22f, TillGeometry.RUN_SHARE)
        assertTrue(TillGeometry.BORDER.startsWith("M48,27H"), "the border starts at 12 o'clock and runs clockwise")
    }
}
