package com.monokulo.till

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** The folded tray's stack (the fixes canvas's board 4): its edges at rest and pulled, and when a release opens the tray. */
class FoldStackTest {
    @Test
    fun atRestTheEdgesAreTheBoardsTwoPeeks() {
        assertEquals(listOf(8f, 7f), List(FoldStack.EDGES) { FoldStack.height(it, 0f) })
        assertEquals(listOf(10f, 20f), List(FoldStack.EDGES) { FoldStack.inset(it, 0f) })
        assertEquals(listOf(14f, 12f), List(FoldStack.EDGES) { FoldStack.corner(it) })
        assertEquals(listOf(0f, 8f), List(FoldStack.EDGES) { FoldStack.top(it, 0f) }, "the second under the first")
        assertEquals(FoldStack.REST_DP, FoldStack.area(0f))
    }

    @Test
    fun pulledTheEdgesTakeTheWholePullSoThePillStaysUnderTheFinger() {
        for (pull in listOf(1f, 30f, FoldStack.REACH_DP, 150f)) {
            val tall = List(FoldStack.EDGES) { FoldStack.height(it, pull) }.sum()
            assertEquals(FoldStack.REST_DP + pull, tall, 0.001f, "at $pull")
            assertEquals(FoldStack.area(pull), tall, 0.001f)
        }
        assertEquals(listOf(5f, 10f), List(FoldStack.EDGES) { FoldStack.inset(it, FoldStack.REACH_DP / 2f) }, "half way to the card's width")
        assertEquals(listOf(0f, 0f), List(FoldStack.EDGES) { FoldStack.inset(it, FoldStack.REACH_DP * 2f) }, "never wider than the card")
        assertTrue(FoldStack.height(1, FoldStack.REACH_DP) >= 58f, "fanned out whole, each edge is about a row tall")
    }

    @Test
    fun foldingTheEdgesGrowBackInFromNothing() {
        assertEquals(0f, FoldStack.area(-FoldStack.REST_DP))
        assertEquals(0f, FoldStack.area(-40f), "never less than nothing")
        assertEquals(listOf(4f, 3.5f), List(FoldStack.EDGES) { FoldStack.height(it, -FoldStack.REST_DP / 2f) }, "in proportion")
        assertEquals(FoldStack.REST_DP - FoldStack.GAP_DP, FoldStack.area(-FoldStack.GAP_DP), "the gap a fold starts from leaves the pill where the open one was")
        assertEquals(13f, FoldStack.GAP_DP, "folded the pill is under 15 dp of edges and 8 of tray; open, under 10")
    }

    @Test
    fun theFingerIsFollowedToTheReachThenResisted() {
        assertEquals(0f, FoldStack.pulled(-20f), "a pull up is no pull")
        assertEquals(50f, FoldStack.pulled(50f))
        assertEquals(FoldStack.REACH_DP, FoldStack.pulled(FoldStack.REACH_DP))
        assertEquals(FoldStack.REACH_DP + 10f, FoldStack.pulled(FoldStack.REACH_DP + 30f), "a third past the reach")
    }

    @Test
    fun aReleasePastAThirdOrAFlingDownOpens() {
        val third = FoldStack.REACH_DP / 3f
        assertFalse(FoldStack.opens(third, 0f), "a third exactly settles back")
        assertTrue(FoldStack.opens(third + 1f, 0f))
        assertTrue(FoldStack.opens(4f, FoldStack.FLING_DP_PER_S), "a short fling down")
        assertFalse(FoldStack.opens(4f, FoldStack.FLING_DP_PER_S - 1f))
        assertFalse(FoldStack.opens(20f, -2_000f), "flung back up")
    }
}
