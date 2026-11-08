package com.monokulo.till

import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.codeCornerShare
import com.monokulo.till.ui.codeSide
import kotlin.test.Test
import kotlin.test.assertEquals

/** Board 8: the sale's code card on a phone. */
class CodeSideTest {
    @Test
    fun `the code card is the window less 20 dp a side, up to 372 dp`() {
        assertEquals(372.dp, codeSide(412.dp), "the design's phone")
        assertEquals(320.dp, codeSide(360.dp), "a narrow phone gives its width")
        assertEquals(372.dp, codeSide(560.dp), "a wider window keeps the cap")
    }

    @Test
    fun `the hero's circle opens from the card's own 32 dp corners at the phone's width`() {
        assertEquals(32f / 372f, codeCornerShare(412.dp), 1e-6f, "at the cap")
        assertEquals(32f / 320f, codeCornerShare(360.dp), 1e-6f, "a narrow phone's smaller card has the same corners")
        assertEquals(32f / 372f, codeCornerShare(0.dp), 1e-6f, "a window not yet measured opens from the cap's")
    }
}
