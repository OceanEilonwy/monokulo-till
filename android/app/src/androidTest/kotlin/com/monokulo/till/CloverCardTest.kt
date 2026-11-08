package com.monokulo.till

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.CloverTender
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.PointOfSaleScreen
import com.monokulo.till.ui.PosSetup
import com.monokulo.till.ui.PosSetupActions
import org.junit.Rule
import org.junit.Test

/** Step 4's Clover card laid out as Square's (the fixes canvas, board 2): titled steps, the payment type as step 1, the test sale row. */
class CloverCardTest {
    @get:Rule
    val compose = createComposeRule()

    private fun show(tender: CloverTender, testSale: TillSale? = null) {
        val setup = PosSetup(
            chosen = PointOfSale.Clover, location = null, merchant = "Corner Café", signedIn = true,
            tender = tender, busy = false, problem = null, testSale = testSale,
        )
        val actions = PosSetupActions({}, {}, null, {}, {}, {})
        // Still, so the loading mark's glints don't keep the clock from idling.
        compose.setContent { Till(Motion.Still) { PointOfSaleScreen(setup, actions) } }
        compose.waitForIdle()
    }

    private fun shown(text: String) = compose.onNodeWithText(text, substring = true, useUnmergedTree = true).assertExists()

    @Test
    fun theAddedPaymentTypeIsStepOneUnderTheCardsTitle() {
        show(CloverTender.Ready)
        shown("On your Clover device")
        shown("Take Monero in Clover Register")
        shown("1.  Till added a payment type called Monero for you")
        shown("2.  Open Clover Register on the Station, Mini, Flex or Go")
        shown("3.  Ring up a sale, tap Pay, then choose Monero")
        shown("Waiting for a test sale… Any amount; refund it in Clover's Orders app after.")
        compose.onNodeWithContentDescription("Waiting for a test sale", useUnmergedTree = true).assertExists()
        shown("A sale never paid in Monero is refunded in Clover's Orders app; Till shows you which.")
    }

    @Test
    fun aPaymentTypeCloverRefusedIsStepOneWithLookAgain() {
        show(CloverTender.AddInClover)
        shown("1.  Add a payment type called Monero in Clover")
        compose.onNodeWithText("Look again").assertHasClickAction().assertHeightIsAtLeast(48.dp)
        shown("2.  Open Clover Register")
        compose.onNodeWithText("Till added", substring = true, useUnmergedTree = true).assertDoesNotExist()
        // No sale can be paid in Monero until the payment type is there, so nothing waits for one.
        compose.onNodeWithText("Waiting for a test sale", substring = true, useUnmergedTree = true).assertDoesNotExist()
    }

    @Test
    fun aTestSaleFoundSaysToRefundItInTheOrdersApp() {
        val sale = testSale("t")
        show(CloverTender.Ready, sale)
        shown("Test sale found: ${SaleText.amount(sale)}. Refund it in Clover's Orders app when you like.")
    }
}
