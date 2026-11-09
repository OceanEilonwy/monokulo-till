package com.monokulo.till

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.StatusPill
import org.junit.Rule
import org.junit.Test

/** M12 on a real frame clock: the status words fade through (the new in once the old are out, decision 225). */
class StatusWordsTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun newStatusWordsFadeThroughTheOldAndThenStandAlone() {
        var sale by mutableStateOf(testSale("a"))
        compose.setContent { Till { StatusPill(saleFace(sale).view) } }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false

        sale = sale.copy(phase = SalePhase.SEEN)
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
        compose.setContent { Till(Motion.Still) { StatusPill(saleFace(sale).view) } }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false

        sale = sale.copy(phase = SalePhase.SEEN)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(150)
        compose.onAllNodesWithText("Waiting for payment", useUnmergedTree = true).assertCountEquals(0)
        compose.onNodeWithText("Seen on the network", useUnmergedTree = true).assertExists()
    }
}
