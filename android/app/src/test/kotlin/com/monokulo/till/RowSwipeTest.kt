package com.monokulo.till

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** A shift row swiped to the start (which rows swipe is the engine's, a row's `closes`): how far the row follows, and when a release asks. */
class RowSwipeTest {
    @Test
    fun theRowFollowsTheFingerToTheStartOnly() {
        assertEquals(0f, RowSwipe.pulled(30f, 360f), "to the end is no swipe")
        assertEquals(-40f, RowSwipe.pulled(-40f, 360f))
        assertEquals(-360f, RowSwipe.pulled(-500f, 360f), "never past its own width")
    }

    @Test
    fun aReleasePastAThirdOrAFlingToTheStartAsks() {
        val third = -360f * RowSwipe.REACH
        assertFalse(RowSwipe.asks(third, 360f, 0f), "a third exactly settles back")
        assertTrue(RowSwipe.asks(third - 1f, 360f, 0f))
        assertFalse(RowSwipe.asks(-20f, 360f, 0f), "a small swipe settles back")
        assertTrue(RowSwipe.asks(-20f, 360f, -RowSwipe.FLING_DP_PER_S), "a short fling to the start")
        assertFalse(RowSwipe.asks(-20f, 360f, 1f - RowSwipe.FLING_DP_PER_S))
        assertFalse(RowSwipe.asks(-200f, 360f, RowSwipe.FLING_DP_PER_S), "past the reach but flung back")
    }
}
