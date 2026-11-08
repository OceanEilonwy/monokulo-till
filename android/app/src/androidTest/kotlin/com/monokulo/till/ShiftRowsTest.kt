package com.monokulo.till

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.ShiftScreen
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** M11 on a real frame clock: a new sale at the top slides the rows under it down to their place. */
class ShiftRowsTest {
    @get:Rule
    val compose = createComposeRule()

    private val now = mutableLongStateOf(1_000)

    private fun top(sale: TillSale): Float =
        compose.onNodeWithText(rowTitle(sale), substring = true, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.top

    @Test
    fun aNewSaleAtTheTopSlidesTheOthersDownByOneRow() {
        val a = testSale("a1", expiresAt = 1_600)
        val b = testSale("b2", expiresAt = 1_600)
        var sales by mutableStateOf(listOf(a, b))
        compose.setContent { Till { EngineShift(sales, now.longValue, noActions) } }
        compose.waitForIdle()
        val pitch = top(b) - top(a)
        val before = top(a)
        compose.mainClock.autoAdvance = false

        val c = testSale("c3", expiresAt = 1_600)
        sales = listOf(c, a, b)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(100)
        val during = top(a)
        assertTrue(during > before && during < before + pitch, "mid-slide the row is between where it was and where it goes: $before < $during < ${before + pitch}")

        compose.mainClock.advanceTimeBy(600)
        assertEquals(pitch, top(a) - top(c), 0.5f, "it lands one row under the new one")
        assertEquals(before + pitch, top(a), 0.5f)
    }

    @Test
    fun underStillTheRowsJumpToTheirPlace() {
        val a = testSale("a1", expiresAt = 1_600)
        val b = testSale("b2", expiresAt = 1_600)
        var sales by mutableStateOf(listOf(a, b))
        compose.setContent { Till(Motion.Still) { EngineShift(sales, now.longValue, noActions) } }
        compose.waitForIdle()
        val pitch = top(b) - top(a)
        val before = top(a)
        compose.mainClock.autoAdvance = false

        sales = listOf(testSale("c3", expiresAt = 1_600), a, b)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertEquals(before + pitch, top(a), 0.5f)
    }
}
