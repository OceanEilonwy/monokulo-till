package com.monokulo.till

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.PointOfSaleScreen
import com.monokulo.till.ui.PosSetupActions
import org.junit.Rule
import org.junit.Test
import uniffi.till_screens.Account
import uniffi.till_screens.Choice
import uniffi.till_screens.PosCard
import uniffi.till_screens.PosChoice
import uniffi.till_screens.PosMark
import uniffi.till_screens.PosStep
import uniffi.till_screens.Press
import uniffi.till_screens.TestSale
import kotlin.test.assertTrue

/**
 * Step 4's Clover card laid out as Square's (the fixes canvas, board 2), drawn from the engine's card: titled
 * numbered steps, Look again after the first when Clover refused the payment type, the test sale row. The words
 * are the engine's, and its tests check them; here, where they go.
 */
class CloverCardTest {
    @get:Rule
    val compose = createComposeRule()

    private val steps = listOf("Add a payment type called **Monero** in Clover", "Open **Clover Register**", "Ring up a sale")

    private fun show(lookAgain: Boolean, testSale: TestSale?) {
        val card = PosCard(
            mark = PosMark.CLOVER, heading = "Connect Clover", lead = "Lead", accountTitle = "Clover account",
            account = Account.Connected("Corner Café"), place = null, changeLocation = false, permissions = "Permissions", problem = null,
            atTheTill = "On your Clover device", stepsTitle = "Take Monero in Clover Register", steps = steps,
            lookAgain = Press("Look again", true).takeIf { lookAgain }, testSale = testSale, footnote = "Refunded in Clover's Orders app.",
        )
        val step = PosStep(
            back = false,
            choices = listOf(PosChoice(PosMark.CLOVER, false, Choice("Clover", null, selected = true, enabled = true))),
            card = card, next = Press("Continue", testSale?.found == true), skip = Press("Skip the test", true),
        )
        val actions = PosSetupActions({}, {}, {}, {}, {}, {})
        // Still, so the loading mark's glints don't keep the clock from idling.
        compose.setContent { Till(Motion.Still) { PointOfSaleScreen(step, "Step 4 of 6", actions) } }
        compose.waitForIdle()
    }

    private fun shown(text: String) = compose.onNodeWithText(text, substring = true, useUnmergedTree = true).assertExists()

    @Test
    fun theStepsAreNumberedUnderTheCardsTitleWithTheTestSaleWaiting() {
        show(lookAgain = false, TestSale(false, "Waiting for a test sale…"))
        shown("ON YOUR CLOVER DEVICE")
        shown("Take Monero in Clover Register")
        shown("1.  Add a payment type called Monero in Clover")
        shown("2.  Open Clover Register")
        shown("3.  Ring up a sale")
        compose.onNodeWithContentDescription("Waiting for a test sale", useUnmergedTree = true).assertExists()
        shown("Refunded in Clover's Orders app.")
    }

    @Test
    fun lookAgainComesAfterTheFirstStep() {
        show(lookAgain = true, null)
        // Where each is laid out, scrolled into view or not.
        fun top(node: androidx.compose.ui.semantics.SemanticsNode) = node.positionInRoot.y
        val first = compose.onNodeWithText("1.  Add a payment type", substring = true, useUnmergedTree = true).fetchSemanticsNode()
        val look = compose.onNodeWithText("Look again").assertHasClickAction().assertHeightIsAtLeast(48.dp).fetchSemanticsNode()
        val second = compose.onNodeWithText("2.  Open Clover Register", substring = true, useUnmergedTree = true).fetchSemanticsNode()
        assertTrue(top(first) < top(look) && top(look) < top(second), "step 1, Look again, step 2: ${top(first)} ${top(look)} ${top(second)}")
        compose.onNodeWithText("Waiting for a test sale", substring = true, useUnmergedTree = true).assertDoesNotExist()
    }
}
