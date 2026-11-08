package com.monokulo.till

import com.monokulo.till.overlay.BubbleMark
import com.monokulo.till.overlay.BubblePadding
import com.monokulo.till.overlay.BubbleSize
import com.monokulo.till.overlay.Panel
import com.monokulo.till.overlay.haloRadius
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.LensLook
import com.monokulo.till.ui.MarkGeometry
import com.monokulo.till.ui.Motion
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * The mark's rings, border arc and greens (`docs/motion.md` §12.5, §13.5):
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
    fun `the chasing segment is the canvas's 30 percent of the border, from 12 o'clock`() {
        assertEquals(0.30f, MarkGeometry.ARC_SHARE)
        assertEquals(-90f, MarkGeometry.ARC_START, "12 o'clock in the draw's angles, clockwise from 3")
        val border = 2 * Math.PI * MarkGeometry.BORDER_RADIUS
        assertEquals(34.0, border * MarkGeometry.ARC_SHARE, 0.1, "34 of the border's 113.1 u, as the canvas's dash")
    }

    @Test
    fun `the green lens is the fixed greens, the core darkest`() {
        assertEquals(Fixed.Paid, LensLook.Green[0])
        assertEquals(Fixed.PaidMid, LensLook.Green[1])
        assertEquals(Fixed.PaidLight, LensLook.Green[2])
        assertEquals(Fixed.PaidMid, LensLook.Green[3])
    }

    /**
     * The bubble's 56 dp disc round its 46 dp mark: the border's outer edge,
     * in the mark's 64-unit box scaled 1.42, leaves about 7 dp of the disc
     * (the design's board 7), and the pulse's halo, 16 dp past the disc at
     * its widest, still ends inside the bubble's window.
     */
    @Test
    fun `the bubble's rim is about 7 dp and its halo fits its window`() {
        val edge = BubbleMark.value / 64f * 1.42f * (MarkGeometry.BORDER_RADIUS + MarkGeometry.BORDER_STROKE / 2)
        val rim = BubbleSize.value / 2 - edge
        assertEquals(7f, rim, 0.5f, "the disc round the border is $rim dp")
        assertEquals(Panel.BUBBLE_WINDOW_DP.toFloat(), BubbleSize.value + 2 * BubblePadding.value, "the window is the disc and its padding")
        val halo = haloRadius(BubbleSize.value, BubblePadding.value, p = 1f)
        assertEquals(Panel.BUBBLE_WINDOW_DP / 2f, halo, "the halo's widest ring touches the window's edge, unclipped")
    }
}
