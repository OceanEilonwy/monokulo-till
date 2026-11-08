package com.monokulo.till

import uniffi.till_app.Screen
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.DeviceConfigurationOverride
import androidx.compose.ui.test.ForcedSize
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.LocalReceiptFolds
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.ReceiptFolds
import com.monokulo.till.ui.SaleScreen
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * The sale's order card is folded to its one row unless the page has room
 * for it open: on a phone it is folded from its first frame, on a tablet
 * it opens, and a tap folds or unfolds it whatever the room. The fold
 * outlives the page: back from the sale's order or code, it is as it was
 * from the first frame. A sale's page folds its card the same way, and a
 * fold's frames recompose nothing.
 */
class ReceiptFoldTest {
    @get:Rule
    val compose = createComposeRule()

    private val sale = testSale("a1", expiresAt = 1_600).copy(receipt = sampleReceipt())

    /** The screen's clock, held: a sale's countdown never runs out (and its ring never spins) under the test. */
    private val now = mutableLongStateOf(1_000)

    private val recompositions = Recompositions()

    @After
    fun stopCounting() = recompositions.close()

    /** The card's row: whose receipt, how many items, the total. */
    private val row get() = compose.onNodeWithText(RowWords, substring = true)

    /** One of its lines, shown only open. */
    private val line get() = compose.onNodeWithText("2 × Flat white")

    private fun sale(size: DpSize) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.ForcedSize(size)) {
                Till { SaleScreen(rememberFace(sale, now.longValue), noActions) }
            }
        }
    }

    /** The sale on the till's stage, its order and code a step away, the folds kept above it as the activity keeps them. */
    private fun stage(size: DpSize): TestModel {
        val model = TestModel(Screen.Sale(sale.orderId))
        val folds = ReceiptFolds()
        // The live sale's ring turns for as long as it waits: the clock is the test's, so it never waits for it to stop.
        compose.mainClock.autoAdvance = false
        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.ForcedSize(size)) {
                Till {
                    CompositionLocalProvider(LocalReceiptFolds provides folds) {
                        ModelStage(model) { s ->
                            when (s) {
                                is Screen.Order -> OrderPage(rememberFace(sale), "Back to the sale", onBack = { model.back(s) }, onShowCode = { model.back(s) })
                                is Screen.Code -> CodeScreen(rememberFace(sale), onBack = { model.back(s) })
                                else -> SaleScreen(rememberFace(sale, now.longValue), noActions)
                            }
                        }
                    }
                }
            }
        }
        compose.mainClock.advanceTimeBy(1_500)
        return model
    }

    /**
     * The row tapped as TalkBack's double tap does, wherever it is: under the code, a scroll to it would wait on a clock
     * the test holds.
     */
    private fun tap() {
        row.performSemanticsAction(SemanticsActions.OnClick)
    }

    /** Whether the card's lines are laid out under its row (folded, they are not placed at all). */
    private fun shown(): Boolean =
        compose.onAllNodesWithText("2 × Flat white").fetchSemanticsNodes().any { it.layoutInfo.isPlaced && it.size.height > 0 }

    private fun state(words: String) = SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, words)

    @Test
    fun unfoldedOnAPhoneItIsStillOpenBackFromTheOrder() = backOpen(DpSize(412.dp, 760.dp), Screen.Order(sale.orderId), tap = true)

    @Test
    fun unfoldedOnAPhoneItIsStillOpenBackFromTheCode() = backOpen(DpSize(412.dp, 760.dp), Screen.Code(sale.orderId), tap = true)

    @Test
    fun onATabletItComesBackFromTheOrderOpenNeverFoldedFirst() = backOpen(DpSize(800.dp, 1280.dp), Screen.Order(sale.orderId), tap = false)

    @Test
    fun onAWideTabletItComesBackFromTheCodeOpen() = backOpen(DpSize(1280.dp, 800.dp), Screen.Code(sale.orderId), tap = false)

    /** Open (by a tap on a phone, by the room on a tablet), the card leaves with its page for [away] and comes back open. */
    private fun backOpen(size: DpSize, away: Screen, tap: Boolean) {
        val model = stage(size)
        if (tap) {
            row.assert(state("Collapsed"))
            tap()
            compose.mainClock.advanceTimeBy(1_000)
        }
        row.assert(state("Expanded"))

        compose.runOnUiThread { model.go(away) }
        compose.mainClock.advanceTimeBy(1_500)
        row.assertDoesNotExist()
        compose.runOnUiThread { model.back(away) }
        // Every frame of the way back, from the first the card is on, it is open: the page it shrinks into is the one it left.
        var frames = 0
        repeat(30) {
            compose.mainClock.advanceTimeByFrame()
            val on = compose.onAllNodesWithText(RowWords, substring = true).fetchSemanticsNodes()
            if (on.isNotEmpty()) {
                frames++
                assertEquals("Expanded", on.single().config[SemanticsProperties.StateDescription], "frame $it of the way back")
            }
        }
        assertTrue(frames > 20, "the card is on the way back's frames: $frames")
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(Screen.Sale(sale.orderId), model.stage.currentState)
        row.assert(state("Expanded"))
        assertTrue(shown(), "its lines are laid out under the row")
    }

    @Test
    fun onAPhoneASalesPageFoldsItsCardAndATapOpensIt() {
        val paid = sale.copy(phase = SalePhase.Paid, paidAt = 990)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.ForcedSize(DpSize(412.dp, 760.dp))) {
                Till { SaleScreen(rememberFace(paid, now.longValue), noActions) }
            }
        }
        compose.mainClock.advanceTimeBy(1_000)
        // Every line of the order is on a sale's page: no room for it open on a phone.
        row.assert(state("Collapsed"))
        assertFalse(shown(), "folded, its lines are not laid out")
        tap()
        compose.mainClock.advanceTimeBy(1_000)
        row.assert(state("Expanded"))
        assertTrue(shown(), "its lines are laid out under the row")
    }

    @Test
    fun aFoldsFramesRecomposeNothingOnAPhone() = foldFrames(DpSize(412.dp, 760.dp))

    @Test
    fun aFoldsFramesRecomposeNothingOnAWideTablet() = foldFrames(DpSize(1280.dp, 800.dp))

    /** The tap recomposes the card's row; the frames that fold or open it recompose nothing, not even the card. */
    private fun foldFrames(size: DpSize) {
        sale(size)
        compose.mainClock.advanceTimeBy(1_000)
        for (to in listOf("Expanded", "Collapsed").let { if (size.width > 1000.dp) it.reversed() else it }) {
            tap()
            compose.mainClock.advanceTimeByFrame()
            compose.mainClock.advanceTimeByFrame()
            recompositions.reset()
            compose.mainClock.advanceTimeBy(1_000)
            row.assert(state(to))
            for (name in listOf("OrderCard", "OrderLines", "LiveSale", "Late", "QrCode", "StatusFoot", "StatusPill", "Buttons")) {
                assertEquals(0, recompositions.count("com.monokulo.till.ui.$name"), "$name isn't recomposed by the fold's frames to $to")
            }
        }
    }

    @Test
    fun onAPhoneTheCardIsFoldedFromItsFirstFrameAndATapOpensIt() {
        sale(DpSize(412.dp, 760.dp))
        compose.mainClock.advanceTimeByFrame()
        row.assert(state("Collapsed"))
        assertFalse(shown(), "folded, its lines are not laid out")
        compose.mainClock.advanceTimeBy(1_000)
        // Under the code, past the window's foot: no room for it open.
        row.assert(state("Collapsed"))
        assertFalse(shown(), "folded, its lines are not laid out")

        tap()
        compose.mainClock.advanceTimeBy(1_000)
        row.assert(state("Expanded"))
        assertTrue(shown(), "its lines are laid out under the row")
        tap()
        compose.mainClock.advanceTimeBy(1_000)
        row.assert(state("Collapsed"))
        assertFalse(shown(), "folded, its lines are not laid out")
    }

    @Test
    fun onATabletInPortraitTheCardIsOpen() = opensWithRoom(DpSize(800.dp, 1280.dp))

    @Test
    fun onATabletInLandscapeTheCardIsOpen() = opensWithRoom(DpSize(1280.dp, 800.dp))

    private fun opensWithRoom(size: DpSize) {
        sale(size)
        compose.mainClock.advanceTimeBy(1_000)
        row.assertIsDisplayed().assert(state("Expanded"))
        line.assertIsDisplayed()

        // The cashier's tap wins over the room.
        tap()
        compose.mainClock.advanceTimeBy(1_000)
        row.assert(state("Collapsed"))
        assertFalse(shown(), "folded, its lines are not laid out")
    }

    private companion object {
        const val RowWords = "k4Rt · 15 items · 79.21 USD"
    }
}
