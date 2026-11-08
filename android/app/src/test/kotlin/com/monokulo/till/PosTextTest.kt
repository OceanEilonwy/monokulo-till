package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import kotlin.test.Test
import kotlin.test.assertEquals

/** The words that name the point of sale (acceptance D5, T2.6, T3.2, T4.1; decision 136). */
class PosTextTest {
    private val square = PointOfSale.Square
    private val clover = PointOfSale.Clover

    @Test
    fun `step 4 and Settings name the point of sale chosen`() {
        assertEquals("Ring up in Square or Clover as usual and choose Monero as the payment.", PosText.WELCOME_FEATURE)
        assertEquals("Square Point of Sale on a phone, tablet or Register", PosText.choiceDetail(square))
        assertEquals("A Clover device", PosText.choiceDetail(clover))
        assertEquals("Connect Clover", PosText.connect(clover))
        assertEquals("Till checks Clover for each Monero payment you take, so it can show the code straight away.", PosText.lead(clover))
        assertEquals(
            "Till can read payments and orders, and add a Monero payment type. Your Clover sign-in stays on this device: " +
                "Monokulo never sees it, or what was bought.",
            PosText.permissions(clover),
        )
        assertEquals(
            "Till can read payments, orders and locations, and refund a Monero sale that's never paid. Your Square sign-in stays " +
                "on this device: Monokulo never sees it, or what was bought.",
            PosText.permissions(square),
        )
        assertEquals("On your Clover device", PosText.atTheTill(clover))
        assertEquals("In Square Point of Sale", PosText.atTheTill(square))
        assertEquals("Not in Clover yet", PosText.tenderMissing(clover))
        assertEquals(listOf("Sign in to Square", "Sign in to Clover"), PointOfSale.entries.map(PosText::signIn))
        assertEquals(listOf("Sign out of Square", "Sign out of Clover"), PointOfSale.entries.map(PosText::signOut))
        assertEquals(listOf("Location", "Merchant"), PointOfSale.entries.map(PosText::placeRow))
        assertEquals(listOf("Stand-in Square address", "Stand-in Clover address"), PointOfSale.entries.map(PosText::standIn))
        assertEquals("Sales paid with this payment type at this merchant show up in Till.", PosText.signedInFootnote(clover))
        assertEquals("Without Clover, Till shows the sales other tills make, without receipts.", PosText.withoutFootnote(clover))
        assertEquals("Without a point of sale, Till shows the sales other tills make, without receipts.", PosText.withoutFootnote(null))
    }

    @Test
    fun `Clover's steps card reads as Square's, with its test sale refunded in the Orders app`() {
        assertEquals("Take Monero in Clover Register", PosText.CLOVER_STEPS_TITLE)
        assertEquals(
            listOf(
                "Till added a payment type called **Monero** for you",
                "Open **Clover Register** on the Station, Mini, Flex or Go",
                "Ring up a sale, tap **Pay**, then choose **Monero**",
            ),
            listOf(PosText.cloverTenderStep(added = true)) + PosText.CLOVER_STEPS,
        )
        assertEquals(
            "Till adds a payment type called **Monero** for you",
            PosText.cloverTenderStep(added = false),
            "before the sign-in, or while the till looks, it hasn't added it yet",
        )
        assertEquals("Waiting for a test sale… Any amount; refund it in Clover's Orders app after.", PosText.testSaleWaiting(clover))
        assertEquals("Test sale found: $1.00. Refund it in Clover's Orders app when you like.", PosText.testSaleFound(clover, "$1.00"))
        assertEquals("Waiting for a test sale… Any amount, you can refund it after.", PosText.testSaleWaiting(square))
        assertEquals("Test sale found: $1.00. Refund it in Square when you like.", PosText.testSaleFound(square, "$1.00"))
    }

    @Test
    fun `each point of sale has its own mark, Square's in the theme's ink and Clover's in its green`() {
        assertEquals(listOf(R.drawable.ic_pos_square, R.drawable.ic_pos_clover), PointOfSale.entries.map(PosText::mark))
        assertEquals(listOf(true, false), PointOfSale.entries.map(PosText::markInInk))
    }

    @Test
    fun `the bubble floats over Square's app, and over other apps beside a Clover device`() {
        assertEquals("Float over Square", PosText.floatHeading(square))
        assertEquals("Float over other apps", PosText.floatHeading(clover))
        assertEquals(
            "Start Till and ring up on your Clover device. Till waits in a faded bubble over other apps and lights up when a sale comes in.",
            PosText.readyLead(clover, bubble = true),
        )
        assertEquals(
            "Start Till and switch to Square. Till waits in its notification and alerts you when a sale comes in.",
            PosText.readyLead(square, bubble = false),
        )
    }

    @Test
    fun `the refund sheet says what to refund, where, and what the till saw`() {
        assertEquals("Refund in Clover", PosText.refundSheetTitle(clover))
        assertEquals("Refund in Square", PosText.refundSheetTitle(square))
        assertEquals(
            "This sale wasn't paid in Monero. Refund it in Clover's Orders app: order 7F3KQ2MA, 4:07 PM, $13.88, paid with Monero. " +
                "Till will notice when it's done.",
            PosText.refundSheetBody(clover, asks = true, "7F3KQ2MA", "4:07 PM", "$13.88"),
        )
        assertEquals(
            "This sale wasn't paid in Monero. Refund it in Clover's Orders app: $13.88, paid with Monero. Till will notice when it's done.",
            PosText.refundSheetBody(clover, asks = true, null, null, "$13.88"),
        )
        assertEquals(
            "This sale wasn't paid in Monero. Refund it in Square's Transactions: order bP9m, $13.88, paid with Monero. " +
                "Tap I've refunded it once it's done.",
            PosText.refundSheetBody(square, asks = false, "bP9m", null, "$13.88"),
            "a sale of the point of sale the till changed from: only the cashier's word",
        )
        assertEquals(
            "Clover doesn't show a refund yet. Finish it in Clover's Orders app, then tap again.",
            PosText.refundNotSeen(clover),
        )
    }

    @Test
    fun `a receipt is Square's receipt or Clover's order`() {
        assertEquals("Square receipt bP9m", PosText.receiptLabel(square, "bP9m"))
        assertEquals("Clover order 7F3KQ2MA", PosText.receiptLabel(clover, "7F3KQ2MA"))
        assertEquals("Receipt bP9m", PosText.receiptLabel(null, "bP9m"))
        assertEquals("Back to Square", PosText.backTo(square))
    }
}
