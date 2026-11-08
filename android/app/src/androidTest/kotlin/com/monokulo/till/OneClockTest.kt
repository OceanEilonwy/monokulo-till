package com.monokulo.till

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.SaleScreen
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals

/**
 * Amendment 10 on the engine's clock (docs/decisions/core.md 37): the engine
 * writes a countdown and publishes the view again each second; a tick
 * changes only the words that count down, so only they recompose, never the
 * code, the ring or the badges.
 */
class OneClockTest {
    @get:Rule
    val compose = createComposeRule()

    private val recompositions = Recompositions()

    @After
    fun stopCounting() = recompositions.close()

    @Test
    fun aTickRecomposesTheLiveRowsAndNotTheirBadges() {
        val sales = List(6) { testSale("live$it", expiresAt = 1_900L + it) } +
            List(6) { testSale("paid$it", phase = com.monokulo.till.core.SalePhase.Paid, paidAt = 990) }
        var view by mutableStateOf(shiftView(sales, now = 1_000))
        compose.setContent { Till { ShiftScreen(view, noActions) } }
        compose.waitForIdle()
        recompositions.reset()

        repeat(5) {
            view = shiftView(sales, now = 1_001L + it)
            compose.waitForIdle()
        }

        // A row's badge transition is called only when its glyph changes, never by the row's countdown.
        assertEquals(0, recompositions.count("com.monokulo.till.ui.RowBadge"), "the badges don't tick")
        assertEquals(0, recompositions.count("com.monokulo.till.ui.TopRow"), "the top row doesn't tick")
    }

    @Test
    fun aTickRecomposesNeitherTheCodeNorTheRing() {
        val sale = testSale("a", expiresAt = 1_900)
        var face by mutableStateOf(saleFace(sale, now = 1_000))
        compose.setContent { Till { SaleScreen(face, noActions) } }
        compose.waitForIdle()
        recompositions.reset()

        repeat(5) {
            face = saleFace(sale, now = 1_001L + it)
            compose.waitForIdle()
        }

        for (name in listOf("RingTick", "QrCode", "PhaseRing")) {
            assertEquals(0, recompositions.count("com.monokulo.till.ui.$name"), "$name doesn't tick")
        }
    }
}
