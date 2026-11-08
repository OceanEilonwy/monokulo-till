package com.monokulo.till

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import com.monokulo.till.core.SalePhase
import com.monokulo.till.ui.Countdown
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.StatusPill
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals

/** M12 on a real frame clock: the status words fade through (the new in once the old are out, decision 225), and a countdown's digits tick without moving the words. */
class StatusWordsTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun newStatusWordsFadeThroughTheOldAndThenStandAlone() {
        var sale by mutableStateOf(testSale("a"))
        val now = mutableLongStateOf(0)
        compose.setContent { Till { StatusPill(sale, now) } }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false

        sale = sale.copy(phase = SalePhase.Seen)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(100)
        compose.onAllNodesWithText("Waiting for payment", useUnmergedTree = true).assertCountEquals(1)
        compose.onAllNodesWithText("Seen on the network", useUnmergedTree = true).assertCountEquals(1)

        compose.mainClock.advanceTimeBy(300)
        compose.onAllNodesWithText("Waiting for payment", useUnmergedTree = true).assertCountEquals(0)
        compose.onNodeWithText("Seen on the network", useUnmergedTree = true).assertExists()
    }

    @Test
    fun underStillTheWordsSwapWithTheQuickFade() {
        var sale by mutableStateOf(testSale("a"))
        val now = mutableLongStateOf(0)
        compose.setContent { Till(Motion.Still) { StatusPill(sale, now) } }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false

        sale = sale.copy(phase = SalePhase.Seen)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(150)
        compose.onAllNodesWithText("Waiting for payment", useUnmergedTree = true).assertCountEquals(0)
        compose.onNodeWithText("Seen on the network", useUnmergedTree = true).assertExists()
    }

    @Test
    fun aCountdownIsTheSameWidthAtEverySecond() {
        // 1:11 and 1:08 left: as narrow and as wide as proportional figures get.
        val now = mutableLongStateOf(1_000)
        compose.setContent {
            Till { Countdown(now, expiresAt = 1_071, Modifier.testTag("left")) { "Scan with any Monero wallet · ${SaleText.clock(it)} left" } }
        }
        compose.onNodeWithText("Scan with any Monero wallet · 1:11 left").assertExists()
        val before = compose.onNodeWithTag("left").fetchSemanticsNode().size
        now.longValue = 1_003
        compose.onNodeWithText("Scan with any Monero wallet · 1:08 left").assertExists()
        assertEquals(before, compose.onNodeWithTag("left").fetchSemanticsNode().size)
    }
}
