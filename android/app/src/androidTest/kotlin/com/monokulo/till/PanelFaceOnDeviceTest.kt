package com.monokulo.till

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import androidx.compose.ui.unit.dp
import com.monokulo.till.overlay.CODE_SLOT_TAG
import com.monokulo.till.overlay.CODE_TAG
import com.monokulo.till.overlay.STATUS_TAG
import com.monokulo.till.overlay.PanelActions
import com.monokulo.till.overlay.Presence
import com.monokulo.till.overlay.SalePanel
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion
import org.junit.Rule
import org.junit.Test

/** The panel's code folding into its paid tick and growing back out of it (M5): the code itself never changes size. */
class PanelFaceOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    private fun sale(id: String, phase: SalePhase) = TillSale(
        orderId = id,
        amount = "79.21",
        currency = "USD",
        reference = "k4Rt",
        code = PaymentCode("monero:4abc?tx_amount=0.31684&id=$id", hasReceipt = false),
        phase = phase,
        expiresAt = System.currentTimeMillis() / 1000 + 600,
        xmr = "0.31684",
        address = "4abc",
        found = FoundSale("p1", 7921, "USD", "k4Rt", "ord", "2026-10-22T23:07:00Z", null),
    )

    private var shown by mutableStateOf(sale("1", SalePhase.Waiting))

    private val actions = PanelActions({}, {}, {}, {}, {}, {}, {}, {})

    private fun panel(wide: Boolean) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            TillTheme {
                CompositionLocalProvider(LocalMotion provides Motion.Full) {
                    SalePanel(panelOf(shown), "Front Counter", wide, actions, Presence(1f))
                }
            }
        }
        compose.mainClock.advanceTimeByFrame()
    }

    private fun assertCodeAt(side: Int) {
        compose.onNodeWithTag(CODE_TAG, useUnmergedTree = true).assertWidthIsEqualTo(side.dp).assertHeightIsEqualTo(side.dp)
    }

    @Test
    fun theCodeIsItsDesignedSizeAtRestAndOnEveryFrameOfTheFold() {
        panel(wide = false)
        assertCodeAt(264)
        shown = sale("1", SalePhase.Paid)
        repeat(30) {
            compose.mainClock.advanceTimeBy(20)
            assertCodeAt(264)
        }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNodeWithText("Paid").assertExists()
        compose.onNodeWithText("Done").assertExists()
        shown = sale("2", SalePhase.Waiting)
        repeat(30) {
            compose.mainClock.advanceTimeBy(20)
            assertCodeAt(264)
        }
        compose.mainClock.advanceTimeBy(1_000)
        assertCodeAt(264)
        compose.onNodeWithText("Full screen").assertExists()
    }

    private fun slotHeight(): Float =
        compose.onNodeWithTag(CODE_SLOT_TAG, useUnmergedTree = true).fetchSemanticsNode().size.height.toFloat()

    /** Every word drawn inside a status row. */
    private fun statusWords(): List<String> =
        compose.onAllNodes(hasAnyAncestor(hasTestTag(STATUS_TAG)), useUnmergedTree = true).fetchSemanticsNodes()
            .flatMap { node -> node.config.getOrNull(SemanticsProperties.Text).orEmpty().map { it.text } }

    @Test
    fun theFoldStartsOnThePaymentsFrameAndTheStatusRowNeverSaysPaid() {
        panel(wide = false)
        val rest = slotHeight()
        shown = sale("1", SalePhase.Paid)
        var folding = -1
        repeat(40) { frame ->
            compose.mainClock.advanceTimeByFrame()
            if (folding < 0 && slotHeight() < rest) folding = frame
            val words = statusWords()
            assertFalse(words.any { it.startsWith("Paid") }, "frame $frame: the leaving status row says $words")
        }
        assertTrue(folding in 0..2, "the code starts folding within two frames of the payment (frame $folding)")
    }

    @Test
    fun theNextSalesWordsAreNeverClippedToTheOldWordsWidth() {
        panel(wide = false)
        shown = sale("1", SalePhase.Paid)
        compose.mainClock.advanceTimeBy(2_000)
        shown = sale("2", SalePhase.Waiting)
        repeat(40) { frame ->
            compose.mainClock.advanceTimeByFrame()
            compose.onAllNodesWithText("Waiting for payment", useUnmergedTree = true).fetchSemanticsNodes().forEach { words ->
                val drawn = words.boundsInRoot.width
                val laidOut = words.size.width.toFloat()
                assertTrue(drawn >= laidOut - 1f, "frame $frame: the words are clipped to $drawn of $laidOut px")
            }
        }
    }

    @Test
    fun onATabletTheCodeIs280() {
        panel(wide = true)
        assertCodeAt(280)
    }
}
