package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.coreAlpha
import com.monokulo.till.ui.glintAlpha
import com.monokulo.till.ui.secondsLeft
import java.time.Instant
import java.time.ZoneId
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals

class SaleTextTest {
    private fun sale(
        id: String = "o1",
        phase: SalePhase = SalePhase.Waiting,
        reference: String? = "#1042",
        found: Boolean = true,
        device: String? = null,
        closedAt: Long? = null,
        refundedAt: Long? = null,
    ) = TillSale(
        orderId = id,
        amount = "24.50",
        currency = "USD",
        reference = reference,
        code = PaymentCode("monero:4abc?tx_amount=0.1421", hasReceipt = true),
        phase = phase,
        expiresAt = 0,
        xmr = "0.1421",
        address = "4abc",
        found = if (found) FoundSale("p1", 2450, "USD", reference, "ord", "2026-10-23T10:00:00Z", null) else null,
        deviceName = device,
        closedAt = closedAt,
        refundedAt = refundedAt,
    )

    private val receipt = Receipt(
        receipt = Receipt.Info("k4Rt", "2026-10-22T16:07:00-07:00"),
        merchant = Receipt.Merchant("The Bean Room", "US"),
        lineItems = listOf(
            Receipt.LineItem("Flat white", 2.0, 4.5, 9.0),
            Receipt.LineItem("Oat latte", 1.0, 5.25, 5.25),
            Receipt.LineItem("Almond croissant", 3.0, 3.75, 11.25),
            Receipt.LineItem("Banana bread", 1.0, 4.5, 4.5),
            Receipt.LineItem("House blend beans, 250 g", 1.5, 12.0, 18.0),
        ),
        tax = Receipt.Tax(listOf(Receipt.TaxLine(0.08875, 4.26))),
        total = 52.26,
        currency = "USD",
    )

    @Test
    fun `a sale is named by the POS's receipt number, else as a Monero sale`() {
        assertEquals("Sale #1042", SaleText.title(sale(reference = "#1042")))
        assertEquals("Sale #bP9m", SaleText.title(sale(reference = "bP9m")))
        assertEquals("Monero sale", SaleText.title(sale(reference = null)))
        assertEquals("Monero sale", SaleText.title(sale(reference = " ")))
    }

    @Test
    fun `a sale's page is titled by whom it's for, its number kept on the line under it`() {
        val named = sale(reference = "m2Qe").copy(customerName = " Ana López ")
        assertEquals("Ana López", SaleText.heading(named))
        assertEquals("Sale #m2Qe · Front Counter · this till", SaleText.headingLine(named, "Front Counter · this till"))
        assertEquals("Sale #m2Qe", SaleText.title(named), "the shift, the bubble and notifications keep the number")
        for (unnamed in listOf(sale(reference = "m2Qe"), sale(reference = "m2Qe").copy(customerName = "  "))) {
            assertEquals("Sale #m2Qe", SaleText.heading(unnamed))
            assertEquals("Front Counter · this till", SaleText.headingLine(unnamed, "Front Counter · this till"))
        }
        assertEquals("Monero sale · This till", SaleText.headingLine(sale(reference = null).copy(customerName = "Ana"), "This till"))
    }

    @Test
    fun `a sale says where it came from`() {
        assertEquals("Front Counter · this till", SaleText.place(sale(), "Front Counter"))
        assertEquals("This till", SaleText.place(sale(), null))
        assertEquals("From Front Counter iPad", SaleText.place(sale(found = false, device = "Front Counter iPad"), "Front Counter"))
        assertEquals("From another till", SaleText.place(sale(found = false), null))
    }

    @Test
    fun `the four steps follow the payment, and an unpaid end is none of them`() {
        assertEquals(
            listOf(1, 1, 2, 3, 4, 0, 0, 0),
            listOf(SalePhase.Waiting, SalePhase.Partial, SalePhase.Seen, SalePhase.Confirming, SalePhase.Paid, SalePhase.Expired, SalePhase.Cancelled, SalePhase.DoubleSpend)
                .map(SaleText::step),
        )
        assertEquals("29:05", SaleText.clock(29 * 60 + 5))
    }

    @Test
    fun `a sale's status follows where it stands on this till`() {
        assertEquals("Waiting for payment", SaleText.status(sale(), PointOfSale.Square))
        assertEquals("Seen on the network", SaleText.status(sale(phase = SalePhase.Seen), PointOfSale.Square))
        assertEquals("Confirming", SaleText.status(sale(phase = SalePhase.Confirming), PointOfSale.Square), "until Monokulo says how many")
        assertEquals("Confirming · 4 of 10", SaleText.status(sale(phase = SalePhase.Confirming).copy(confirmations = 4, confirmationsRequired = 10), PointOfSale.Square))
        assertEquals(
            "Seen on the network", SaleText.status(sale(phase = SalePhase.Seen).copy(confirmations = 0, confirmationsRequired = 10), PointOfSale.Square),
            "seen has no count to give",
        )
        assertEquals("Part paid", SaleText.status(sale(phase = SalePhase.Partial), PointOfSale.Square))
        assertEquals("Paid", SaleText.status(sale(phase = SalePhase.Paid), PointOfSale.Square))
        assertEquals("Paid after you closed it", SaleText.status(sale(phase = SalePhase.Paid, closedAt = 1), PointOfSale.Square))
        assertEquals("Paid after the code expired", SaleText.status(sale(phase = SalePhase.Paid).copy(endedAt = 1), PointOfSale.Square))
        assertEquals("Paid after you closed it", SaleText.status(sale(phase = SalePhase.Paid, closedAt = 1).copy(endedAt = 2), PointOfSale.Square), "closed, then the code ran out, then paid")
        assertEquals("Paid after the refund", SaleText.status(sale(phase = SalePhase.Paid, closedAt = 1, refundedAt = 2), PointOfSale.Square), "a late payment after the Square refund")
        assertEquals("Closed · not paid", SaleText.status(sale(closedAt = 1), PointOfSale.Square))
        assertEquals("Code expired · not paid", SaleText.status(sale(phase = SalePhase.Expired), PointOfSale.Square))
        assertEquals("Code expired · not paid", SaleText.status(sale(phase = SalePhase.Expired, closedAt = 1), PointOfSale.Square), "the code running out outranks the cashier's close")
        assertEquals("Cancelled · not paid", SaleText.status(sale(phase = SalePhase.Cancelled), PointOfSale.Square))
        assertEquals("Payment reversed", SaleText.status(sale(phase = SalePhase.DoubleSpend), PointOfSale.Square))
        assertEquals("Refunded in Square", SaleText.status(sale(phase = SalePhase.Expired, refundedAt = 2), PointOfSale.Square))
        assertEquals("Refunded in Square", SaleText.status(sale(phase = SalePhase.Cancelled, closedAt = 1, refundedAt = 2), PointOfSale.Square))
        assertEquals("Code expired · not paid", SaleText.status(sale(phase = SalePhase.Expired, found = false, device = "Back till"), PointOfSale.Square), "what this till knows; the row adds the till")
        assertEquals("Payment reversed", SaleText.status(sale(phase = SalePhase.DoubleSpend, found = false), PointOfSale.Square))
        assertEquals("Closed · not paid", SaleText.status(sale(found = false, closedAt = 1), PointOfSale.Square))
        assertEquals("Paid", SaleText.status(sale(phase = SalePhase.Paid, found = false, device = "Back till"), PointOfSale.Square), "another till's paid sale is just paid")
    }

    @Test
    fun `what was bought is counted and listed from the on-device receipt`() {
        assertEquals("8 items", SaleText.items(receipt), "whole quantities add up; the 1.5 of beans by weight is one item")
        assertEquals("3 items", SaleText.items(receipt.copy(lineItems = listOf(receipt.lineItems[0], Receipt.LineItem("Granola", 2.5, 2.0, 5.0)))), "2 × Flat white and 2.5 × Granola: never '5 items'")
        assertEquals("1 item", SaleText.items(receipt.copy(lineItems = receipt.lineItems.take(1).map { it.copy(quantity = 1.0) })))
        assertEquals("2 × Flat white, Oat latte, 3 × Almond croissant, +2 more", SaleText.firstItems(receipt))
        assertEquals("2 × Flat white, Oat latte", SaleText.firstItems(receipt.copy(lineItems = receipt.lineItems.take(2))), "no '+0 more'")
        assertEquals("1.5 × House blend beans, 250 g", SaleText.line(receipt.lineItems.last()))
        assertEquals("9.00", SaleText.plain(9.0, "USD"))
        assertEquals("1125", SaleText.plain(1125.0, "JPY"))
        val at = Instant.parse("2026-10-22T16:07:00Z").epochSecond
        assertEquals("9:07 AM", SaleText.time(at, Locale.US, ZoneId.of("America/Los_Angeles")), "the locale's own form, with a plain space")
        assertEquals("16:07", SaleText.time(at, Locale.UK, ZoneId.of("UTC")))
    }

    @Test
    fun `phase changes report new sales and new phases, but not what was there at the start`() {
        val changes = PhaseChanges()
        assertEquals(emptyList(), changes.next(listOf(sale("old"))))
        assertEquals(emptyList(), changes.next(listOf(sale("old"))))
        val fresh = sale("new")
        assertEquals(listOf(fresh), changes.next(listOf(fresh, sale("old"))))
        val paid = fresh.copy(phase = SalePhase.Paid)
        assertEquals(listOf(paid), changes.next(listOf(paid, sale("old"))))
        assertEquals(emptyList(), changes.next(listOf(paid, sale("old"))))
    }

    @Test
    fun `the bubble is lit while a sale's code is up, green just after one is paid, else faded`() {
        assertEquals(BubbleLook.Idle, Bubble.look(emptyList(), justPaid = false))
        assertEquals(BubbleLook.Lit, Bubble.look(listOf(sale(phase = SalePhase.Seen)), justPaid = false))
        assertEquals(BubbleLook.Idle, Bubble.look(listOf(sale(closedAt = 1)), justPaid = false), "a closed sale's code is down")
        assertEquals(BubbleLook.Paid, Bubble.look(listOf(sale(phase = SalePhase.Seen)), justPaid = true), "paid shows first, even with another sale waiting")
        assertEquals(BubbleLook.Paid, Bubble.look(listOf(sale(phase = SalePhase.Paid)), justPaid = true))
        assertEquals(BubbleLook.Idle, Bubble.look(listOf(sale(phase = SalePhase.Paid)), justPaid = false))
    }

    @Test
    fun `the bubble's label says the amount and the step it no longer shows`() {
        val amount = SaleText.amount(sale())
        assertEquals("Till, $amount waiting for payment", Bubble.label(BubbleLook.Lit, sale()))
        assertEquals("Till, $amount part paid", Bubble.label(BubbleLook.Lit, sale(phase = SalePhase.Partial)))
        assertEquals("Till, $amount seen on the network", Bubble.label(BubbleLook.Lit, sale(phase = SalePhase.Seen)))
        assertEquals("Till, $amount confirming", Bubble.label(BubbleLook.Lit, sale(phase = SalePhase.Confirming)))
        assertEquals("Till, $amount paid", Bubble.label(BubbleLook.Paid, sale(phase = SalePhase.Paid)))
        assertEquals("Till, nothing waiting", Bubble.label(BubbleLook.Idle, sale(phase = SalePhase.Paid)))
        assertEquals("Till, nothing waiting", Bubble.label(BubbleLook.Idle, null))
    }

    @Test
    fun `a tap on the bubble with no sale to show opens Till, and with one opens or lowers the panel`() {
        // No sale this shift yet: the panel has nothing to show, so the app comes up instead, panel wanted or not.
        assertEquals(null, Bubble.focus(emptyList(), previous = null, justPaid = false))
        assertEquals(BubbleTap.OpenApp, Bubble.tap(null, panelOpen = false))
        assertEquals(BubbleTap.OpenApp, Bubble.tap(null, panelOpen = true))
        assertEquals("Open Till", Bubble.tapLabel(BubbleTap.OpenApp))
        // A sale waiting: the tap toggles the panel, as before.
        val waiting = listOf(sale())
        val focus = waiting.firstOrNull { it.orderId == Bubble.focus(waiting, previous = null, justPaid = false) }
        assertEquals(BubbleTap.OpenPanel, Bubble.tap(focus, panelOpen = false))
        assertEquals(BubbleTap.LowerPanel, Bubble.tap(focus, panelOpen = true))
        // The last sale ended: the bubble is idle but still about it, so the panel shows it ended.
        val ended = sale(phase = SalePhase.Expired, closedAt = 1)
        assertEquals(BubbleTap.OpenPanel, Bubble.tap(ended, panelOpen = false))
        assertEquals("Lower to the bubble", Bubble.tapLabel(BubbleTap.LowerPanel))
    }

    @Test
    fun `the panel stays on a sale while it shows as paid, then moves to the next one waiting`() {
        val waiting = sale("older")
        val paid = sale("newer", phase = SalePhase.Paid)
        assertEquals("newer", Bubble.focus(listOf(paid, waiting), "newer", justPaid = true))
        assertEquals("older", Bubble.focus(listOf(paid, waiting), "newer", justPaid = false))
        assertEquals("newer", Bubble.focus(listOf(sale("newer"), waiting), "older", justPaid = false), "a newer sale takes over")
        assertEquals("older", Bubble.focus(listOf(sale("newer", closedAt = 1), waiting), "older", justPaid = false), "not a closed one")
        assertEquals("gone", Bubble.focus(emptyList(), "gone", justPaid = false))
    }

    @Test
    fun `the loading mark's glints follow the web's keyframes`() {
        assertEquals(0f, glintAlpha(0f))
        assertEquals(0.95f, glintAlpha(0.05f))
        assertEquals(0f, glintAlpha(0.5f))
        assertEquals(0f, coreAlpha(0.2f))
        assertEquals(0.95f, coreAlpha(0.6f))
        assertEquals(0f, coreAlpha(0.99f))
    }

    @Test
    fun `a countdown is the seconds from now to the code's end, and never below none`() {
        assertEquals(552, secondsLeft(expiresAt = 1_000_552, now = 1_000_000))
        assertEquals(0, secondsLeft(expiresAt = 1_000_000, now = 1_000_000))
        assertEquals(0, secondsLeft(expiresAt = 1_000_000, now = 1_000_009), "a code past its end shows 0:00, not a negative time")
    }

    @Test
    fun `confirming counts up to the store's threshold and no further`() {
        fun words(have: Int?, need: Int?) = SaleText.confirming(sale(phase = SalePhase.Confirming).copy(confirmations = have, confirmationsRequired = need))
        assertEquals("Confirming · 1 of 10", words(1, 10))
        assertEquals("Confirming · 10 of 10", words(12, 10), "a read just before it turns paid never says more than it needs")
        assertEquals("Confirming", words(4, null), "with no threshold there is nothing to count to")
        assertEquals("Confirming", words(null, 10))
        assertEquals("Confirming", words(0, 0), "a store that takes no confirmation is paid on sight")
    }
}
