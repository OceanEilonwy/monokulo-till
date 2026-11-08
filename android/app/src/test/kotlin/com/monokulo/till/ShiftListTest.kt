package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.overlay.Panel
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live
import kotlinx.serialization.json.Json
import java.time.Instant
import java.time.ZoneId
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ShiftListTest {
    private val zone = ZoneId.of("America/Los_Angeles")
    private val at = Instant.parse("2026-10-22T22:58:00Z").epochSecond // 3:58 PM there

    private fun sale(
        id: String,
        phase: SalePhase = SalePhase.Waiting,
        mine: Boolean = true,
        amount: String = "8.40",
        currency: String = "USD",
        closedAt: Long? = null,
        refundedAt: Long? = null,
        paidAt: Long? = null,
        endedAt: Long? = null,
        device: String? = "Back till",
    ) = TillSale(
        orderId = id,
        amount = amount,
        currency = currency,
        reference = id,
        code = PaymentCode("monero:4abc?tx_amount=0.1", hasReceipt = mine),
        phase = phase,
        expiresAt = 0,
        xmr = "0.0336",
        address = "4abc",
        found = if (mine) FoundSale("p_$id", 840, currency, id, "o", "2026-10-22T22:50:00Z", null) else null,
        deviceName = if (mine) null else device,
        createdAt = at - 600,
        paidAt = paidAt,
        endedAt = endedAt,
        closedAt = closedAt,
        refundedAt = refundedAt,
    )

    /** acceptance §12's fixtures, newest first. */
    private val shift = listOf(
        sale("waiting"),
        sale("closedHere", closedAt = at),
        sale("closedTheirs", mine = false, closedAt = at),
        sale("expired", phase = SalePhase.Expired, endedAt = at),
        sale("expiredTheirs", phase = SalePhase.Expired, mine = false, endedAt = at),
        sale("reversed", phase = SalePhase.DoubleSpend, endedAt = at),
        sale("refunded", phase = SalePhase.Expired, closedAt = at - 60, refundedAt = at),
        sale("paid", phase = SalePhase.Paid, amount = "24.50", paidAt = at),
        sale("latePaid", phase = SalePhase.Paid, amount = "6.00", closedAt = at - 900, paidAt = at),
        sale("paidAfterRefund", phase = SalePhase.Paid, amount = "11.00", closedAt = at - 900, refundedAt = at - 800, paidAt = at),
        sale("cancelled", phase = SalePhase.Cancelled, endedAt = at),
    )

    private fun ids(sales: List<TillSale>) = sales.map { it.orderId }

    @Test
    fun `needs action counts what this till can still refund, and a payment after the refund`() {
        assertEquals(5, ShiftList.needsAction(shift))
        assertEquals(
            listOf("closedHere", "expired", "reversed", "paidAfterRefund", "cancelled"),
            ids(ShiftList.rows(shift, ShiftFilter.NeedsAction)),
            "never another till's, a refunded, a paid or a waiting sale",
        )
    }

    @Test
    fun `a new shift keeps the last one's sales that still need action, and only those`() {
        assertEquals(ids(ShiftList.rows(shift, ShiftFilter.NeedsAction)), ids(ShiftList.carried(shift)))
        assertEquals(emptyList(), ShiftList.carried(listOf(sale("w"))))
    }

    @Test
    fun `the filters keep the shift's order, newest first`() {
        assertEquals(ids(shift), ids(ShiftList.rows(shift, ShiftFilter.All)))
        assertEquals(listOf("paid", "latePaid", "paidAfterRefund"), ids(ShiftList.rows(shift, ShiftFilter.Paid)))
        assertEquals(emptyList(), ShiftList.rows(listOf(sale("w")), ShiftFilter.NeedsAction))
        assertEquals("Nothing needs action", ShiftList.empty(ShiftFilter.NeedsAction))
        assertEquals("Nothing paid yet", ShiftList.empty(ShiftFilter.Paid))
    }

    @Test
    fun `the summary counts paid sales, sums them per currency and says since when`() {
        val started = Instant.parse("2026-10-22T16:02:00Z").epochSecond
        assertEquals("3 paid · $41.50 · since 9:02 AM", ShiftList.summary(shift, started, Locale.US, zone))
        assertEquals("Nothing paid yet · since 9:02 AM", ShiftList.summary(listOf(sale("w")), started, Locale.US, zone))
        assertEquals("Nothing paid yet", ShiftList.summary(emptyList(), null, Locale.US, zone))
        val mixed = listOf(sale("a", SalePhase.Paid, amount = "1.25"), sale("b", SalePhase.Paid, amount = "500", currency = "JPY"), sale("c", SalePhase.Paid, amount = "2.75"))
        assertEquals("3 paid · $4.00 · ¥500", ShiftList.summary(mixed, null, Locale.US, zone))
        // A sale the last shift carried over, paid after its refund, isn't this shift's takings.
        val carried = sale("paidAfterRefund", SalePhase.Paid, amount = "11.00", closedAt = started - 900, refundedAt = started - 800, paidAt = started - 60)
        assertEquals("1 paid · $24.50 · since 9:02 AM", ShiftList.summary(listOf(carried, sale("paid", SalePhase.Paid, amount = "24.50", paidAt = at)), started, Locale.US, zone))
    }

    @Test
    fun `a live sale's row opens its code, every other row its page`() {
        val live = listOf(sale("w"), sale("s", SalePhase.Seen), sale("c", SalePhase.Confirming), sale("p", SalePhase.Partial), sale("t", mine = false))
        assertEquals(List(5) { RowOpens.Code }, live.map(ShiftList::opens))
        assertEquals(List(10) { RowOpens.Details }, shift.drop(1).map(ShiftList::opens))
    }

    @Test
    fun `each row says how its sale stands, in its state's colour`() {
        fun row(sale: TillSale) = ShiftList.row(sale, PointOfSale.Square, secondsLeft = 552, Locale.US, zone).let { Triple(it.sub, it.tone, it.icon) }
        assertEquals(Triple("Waiting · 9:12 left", Tone.Link, StateIcon.Ring), row(sale("w")))
        assertEquals(Triple("Seen on the network", Tone.Paid, StateIcon.Ring), row(sale("w", SalePhase.Seen)), "the money on its way is green")
        assertEquals(Triple("Confirming", Tone.Paid, StateIcon.Ring), row(sale("w", SalePhase.Confirming)), "until Monokulo says how many")
        val counted = sale("w", SalePhase.Confirming).copy(confirmations = 4, confirmationsRequired = 10)
        assertEquals(Triple("Confirming · 4 of 10", Tone.Paid, StateIcon.Ring), row(counted))
        assertEquals(Triple("Part paid · 9:12 left", Tone.Link, StateIcon.Ring), row(sale("w", SalePhase.Partial)))
        val rows = shift.associate { it.orderId to row(it) }
        assertEquals(Triple("Closed · not paid · refund it in Square", Tone.Warn, StateIcon.Clock), rows["closedHere"])
        assertEquals(Triple("Closed · not paid · refund it on Back till", Tone.Muted, StateIcon.Close), rows["closedTheirs"])
        assertEquals(Triple("Code expired · not paid", Tone.Warn, StateIcon.Clock), rows["expired"])
        assertEquals(Triple("Code expired · not paid · on Back till", Tone.Muted, StateIcon.Close), rows["expiredTheirs"])
        assertEquals(Triple("Cancelled · not paid", Tone.Warn, StateIcon.Clock), rows["cancelled"])
        assertEquals(Triple("Payment reversed · don't hand it over", Tone.Danger, StateIcon.Warning), rows["reversed"])
        assertEquals(Triple("Refunded in Square · 3:58 PM", Tone.Muted, StateIcon.Refund), rows["refunded"])
        assertEquals(Triple("Paid · 3:58 PM", Tone.Muted, StateIcon.Tick), rows["paid"])
        assertEquals(Triple("Paid after you closed it · 3:58 PM", Tone.Paid, StateIcon.Tick), rows["latePaid"])
        assertEquals(Triple("Paid after the refund · 3:58 PM", Tone.Warn, StateIcon.Tick), rows["paidAfterRefund"])
        assertEquals(
            Triple("Paid after the code expired · 3:58 PM", Tone.Paid, StateIcon.Tick),
            row(sale("x", SalePhase.Paid, endedAt = at - 60, paidAt = at)),
        )
        assertEquals(
            Triple("Code expired · not paid · on another till", Tone.Muted, StateIcon.Close),
            row(sale("x", SalePhase.Expired, mine = false, device = null)),
        )
        val refunded = ShiftList.row(shift.single { it.orderId == "refunded" }, PointOfSale.Square, 0, Locale.US, zone)
        assertEquals("Sale #refunded · ${SaleText.amount(sale("refunded"))}", refunded.title, "the amount as the locale writes it")
        assertEquals(true, refunded.quiet, "a refunded sale's title is muted too")
        assertEquals(false, ShiftList.row(sale("w"), PointOfSale.Square, 0).quiet)
    }

    @Test
    fun `a Clover till's rows and pages name Clover, and a sale to refund in Clover needs action`() {
        val clover = PointOfSale.Clover
        fun sub(sale: TillSale) = ShiftList.row(sale, clover, 0, Locale.US, zone).sub
        assertEquals("Closed · not paid · refund it in Clover", sub(sale("c", closedAt = at)))
        assertEquals("Refunded in Clover · 3:58 PM", sub(sale("r", SalePhase.Expired, closedAt = at - 60, refundedAt = at)))
        val pending = sale("p", SalePhase.Expired, endedAt = at - 60).copy(refundPendingAt = at)
        val row = ShiftList.row(pending, clover, 0, Locale.US, zone)
        assertEquals("Refund it in Clover · order o… · 3:58 PM", row.sub)
        assertEquals(Tone.Warn, row.tone)
        assertEquals(StateIcon.Refund, row.icon)
        assertEquals(1, ShiftList.needsAction(listOf(pending)), "told to refund it and not yet seen: it needs the cashier")
        assertEquals("Not paid · refund it in Clover", SaleText.status(pending, clover))
        val page = SalePage.of(pending, clover, now = at, Locale.US, zone)
        assertEquals(listOf(PageAction.Refund), page.actions)
        assertEquals(
            "Refund this sale in Clover, then tap Refund to say it's done. While the shift is on, Till notices by itself too.",
            page.note?.text,
        )
        assertEquals("Told to refund it", page.facts.last().label)
        assertEquals(
            "Clover has this sale as Monero. If the customer isn't paying, refund it. Monokulo keeps watching for a late payment for a while: " +
                "if one arrives, this sale turns paid.",
            SalePage.of(sale("c", closedAt = at), clover, now = at, Locale.US, zone).note?.text,
        )
        assertEquals("No: refund it in Clover", Closing.title(CloseChoice.RefundIt, sale("w"), clover))
        assertEquals(
            "Till closes it and shows you how to refund it in Clover.",
            Closing.detail(CloseChoice.RefundIt, sale("w"), clover, source = clover, secondsLeft = 0),
        )
        assertEquals("Till refunds the sale and closes it.", Closing.detail(CloseChoice.RefundIt, sale("w"), PointOfSale.Square, PointOfSale.Square, 0))
        assertEquals(
            "Till closes it and shows you how to refund it in Square.",
            Closing.detail(CloseChoice.RefundIt, sale("w"), PointOfSale.Square, source = clover, secondsLeft = 0),
            "a Square sale on a till now on Clover: refunded in Square, by the cashier",
        )
    }

    @Test
    fun `an order's title at the point of sale is the page's first fact`() {
        val titled = sale("t", closedAt = at).let { it.copy(found = it.found!!.copy(title = "Table 4")) }
        val page = SalePage.of(titled, PointOfSale.Clover, now = at, Locale.US, zone)
        assertEquals(Fact("Order", "Table 4"), page.facts.first())
        assertEquals(listOf("Rung up", "Closed"), SalePage.of(sale("u", closedAt = at), PointOfSale.Clover, now = at, Locale.US, zone).facts.map { it.label })
    }

    @Test
    fun `a Clover sale on a till changed to Square keeps Clover's name on its screens, read back from the saved shift too`() {
        val clover = sale("c").let { it.copy(found = it.found!!.copy(pointOfSale = PointOfSale.Clover)) }
        // The shift as Stores saves and reads it again (a reload, a relaunch).
        val json = Json { ignoreUnknownKeys = true; explicitNulls = false }
        val saved = json.decodeFromString(TillSale.serializer(), json.encodeToString(TillSale.serializer(), clover))
        for (shown in listOf(clover, saved)) {
            val header = SaleHeader.of(shown, PointOfSale.Square, location = "Front Counter")
            assertEquals(PointOfSale.Clover, header.pos, "the receipt is Clover's order")
            assertTrue(header.elsewhere)
            assertEquals("Clover · this till", header.place, "not the Square location the till watches now")
            assertFalse(header.backToPos, "no Back to Square for a Clover sale")
            assertEquals("Clover order T5XWHYQG", PosText.receiptLabel(header.pos, "T5XWHYQG"))
            val page = SalePage.of(shown.copy(closedAt = at), header.pos, now = at, Locale.US, zone, elsewhere = header.elsewhere)
            assertEquals(listOf(PageAction.ShowCodeAgain), page.actions)
            assertTrue(Closing.subtitle(shown, header.pos).endsWith(" · Clover has it as Monero"))
        }
        val square = SaleHeader.of(sale("s"), PointOfSale.Square, location = "Front Counter")
        assertEquals(SaleHeader(PointOfSale.Square, elsewhere = false, place = "Front Counter · this till", backToPos = true), square)
        val theirs = SaleHeader.of(sale("t", mine = false), PointOfSale.Square, location = "Front Counter")
        assertEquals(SaleHeader(PointOfSale.Square, elsewhere = false, place = "From Back till", backToPos = true), theirs)
        assertFalse(SaleHeader.of(sale("s"), PointOfSale.Clover, location = "The Bean Room").backToPos, "Clover's device is its own: no Back to")
    }

    @Test
    fun `a sale found before the point of sale changed keeps the name of the one that took it`() {
        val before = sale("old", closedAt = at, refundedAt = at).let { it.copy(found = it.found!!.copy(pointOfSale = PointOfSale.Clover)) }
        val after = sale("new", closedAt = at, refundedAt = at)
        assertEquals(PointOfSale.Clover, PosText.of(before, PointOfSale.Square))
        assertEquals(PointOfSale.Square, PosText.of(after, PointOfSale.Clover), "found at Square: Square's, whatever the till's now")
        assertEquals(PointOfSale.Clover, PosText.of(sale("theirs", mine = false), PointOfSale.Clover), "another till's: the till's own")
        assertEquals(null, PosText.of(sale("theirs", mine = false), null))
        assertEquals("Refunded in Clover", SaleText.status(before, PosText.of(before, PointOfSale.Square)))
        val closed = SalePage.of(sale("c", closedAt = at), PointOfSale.Clover, now = at, Locale.US, zone, elsewhere = true)
        assertEquals(listOf(PageAction.ShowCodeAgain), closed.actions, "this till can't refund it, or see its refund")
        assertEquals(
            "This sale was rung up in Clover before this till changed its point of sale. If the customer isn't paying, refund it in Clover.",
            closed.note?.text,
        )
        val told = sale("t", SalePhase.Expired, endedAt = at).copy(refundPendingAt = at)
        val pendingThere = SalePage.of(told, PointOfSale.Clover, now = at, Locale.US, zone, elsewhere = true)
        assertEquals(listOf(PageAction.Refund), pendingThere.actions, "already told to refund it there: Refund takes the cashier's word")
        assertEquals(
            "This sale was rung up in Clover before this till changed its point of sale. Refund it in Clover, then tap Refund to say it's done.",
            pendingThere.note?.text,
        )
        assertEquals(listOf(CloseChoice.CloseForNow, CloseChoice.KeepUp), Closing.choices(sale("w"), elsewhere = true))
        assertEquals(CloseChoice.entries, Closing.choices(sale("w")))
    }

    @Test
    fun `a till with no point of sale says so in plain words`() {
        assertEquals("Closed · not paid · refund it in your point of sale", ShiftList.row(sale("c", closedAt = at), null, 0, Locale.US, zone).sub)
        assertEquals("Refund it in your point of sale on Back till.", Panel.endedLine(sale("t", SalePhase.Expired, mine = false), null))
    }

    @Test
    fun `the close sheet's answers close, refund and go where the design says`() {
        assertEquals(CloseOutcome(close = true, refund = true, then = CloseOutcome.Then.Sale), Closing.outcome(CloseChoice.RefundIt))
        assertEquals(CloseOutcome(close = true, refund = false, then = CloseOutcome.Then.Shift), Closing.outcome(CloseChoice.CloseForNow))
        assertEquals(CloseOutcome(close = false, refund = false, then = CloseOutcome.Then.Stay), Closing.outcome(CloseChoice.KeepUp))
        assertEquals(CloseChoice.entries, Closing.choices(sale("w")))
        val theirs = sale("t", mine = false)
        assertEquals(listOf(CloseChoice.CloseForNow, CloseChoice.KeepUp), Closing.choices(theirs), "this till can't refund another's")
        assertEquals("No: close it", Closing.title(CloseChoice.CloseForNow, theirs, PointOfSale.Square))
        assertEquals("Refund it in Square on Back till. A late payment still shows here.", Closing.detail(CloseChoice.CloseForNow, theirs, PointOfSale.Square, PointOfSale.Square, 0))
        assertEquals("Not sure: close it for now", Closing.title(CloseChoice.CloseForNow, sale("w"), PointOfSale.Square))
        assertEquals("14:52 left to pay.", Closing.detail(CloseChoice.KeepUp, sale("w"), PointOfSale.Square, PointOfSale.Square, 892))
        assertEquals(emptyList(), Closing.choices(shift.single { it.orderId == "refunded" }), "nothing to ask: no X")
        assertEquals(emptyList(), Closing.choices(sale("w", closedAt = 1)))
        assertTrue(Closing.applies(sale("w")), "the code is still up: the answer stands")
        assertFalse(Closing.applies(sale("p", SalePhase.Paid)), "paid as the cashier tapped: the answer is dropped")
        assertFalse(Closing.applies(sale("w", closedAt = 1)))
        assertFalse(Closing.applies(null), "gone from the shift")
        val amount = SaleText.amount(sale("w"))
        assertEquals("Sale #w · $amount · Square has it as Monero", Closing.subtitle(sale("w"), PointOfSale.Square))
        assertEquals("Sale #t · $amount · From Back till", Closing.subtitle(theirs, PointOfSale.Square))
    }

    @Test
    fun `a late payment says how late, in words`() {
        assertEquals("under a minute ", SalePage.after(100, 159))
        assertEquals("1 minute ", SalePage.after(100, 160))
        assertEquals("15 minutes ", SalePage.after(100, 100 + 15 * 60 + 30))
        assertEquals("", SalePage.after(null, 100))
        val late = SalePage.of(shift.single { it.orderId == "latePaid" }, PointOfSale.Square, now = at, Locale.US, zone)
        assertEquals(
            "The customer paid 15 minutes after the sale was closed. Square already has it as Monero, so there's nothing to do. Hand over the order if they're still here.",
            late.note?.text,
        )
        assertEquals(listOf("Rung up", "Closed", "Paid"), late.facts.map { it.label })
        val expiredThenPaid = SalePage.of(sale("x", SalePhase.Paid, endedAt = at - 60, paidAt = at), PointOfSale.Square, now = at, Locale.US, zone)
        assertEquals("Paid after the code expired", expiredThenPaid.heading)
        assertEquals(true, expiredThenPaid.note!!.text.startsWith("The customer paid 1 minute after the code ran out."))
        assertEquals(listOf("Rung up", "Code expired", "Paid"), expiredThenPaid.facts.map { it.label })
    }

    @Test
    fun `each page says what the design says, without times the app doesn't know`() {
        fun page(id: String) = SalePage.of(shift.single { it.orderId == id }, PointOfSale.Square, now = at, Locale.US, zone)
        val paid = page("paid")
        assertEquals("Paid", paid.heading)
        assertEquals("0.0336 XMR received · 3:58 PM", paid.line)
        assertNull(paid.note)
        assertEquals(listOf(Fact("Rung up", "3:48 PM"), Fact("Paid", "3:58 PM")), paid.facts, "no seen row when it wasn't seen; no confirmations")
        assertEquals(emptyList(), paid.actions)

        val closed = page("closedHere")
        assertEquals(listOf(PageAction.Refund, PageAction.ShowCodeAgain), closed.actions)
        assertEquals(
            "Square has this sale as Monero. If the customer isn't paying, refund it. Monokulo keeps watching for a late payment for a while: if one arrives, this sale turns paid.",
            closed.note?.text,
        )
        val stillLive = SalePage.of(sale("c", closedAt = at).copy(expiresAt = at + 1440), PointOfSale.Square, now = at, Locale.US, zone)
        assertEquals(true, stillLive.note!!.text.contains("The code is still live until 4:22 PM, and Monokulo keeps watching"))
        val settled = SalePage.of(sale("c", SalePhase.Expired, endedAt = at).copy(settled = true), PointOfSale.Square, now = at, Locale.US, zone)
        assertEquals(true, settled.note!!.text.endsWith("Monokulo has stopped watching it."))
        assertEquals(listOf(PageAction.Refund), settled.actions, "an expired code can't be shown again")

        val reversed = page("reversed")
        assertEquals(Ground.Danger, reversed.note?.ground)
        assertEquals("0.0336 XMR · no longer counts", reversed.line)

        val refunded = page("refunded")
        assertEquals("Refunded 3:58 PM · nothing was paid", refunded.line)
        assertEquals(true, refunded.amountMuted)
        assertEquals(listOf("Rung up", "Closed: customer not paying", "Refunded in Square"), refunded.facts.map { it.label })

        val afterRefund = page("paidAfterRefund")
        assertEquals("Paid after the refund", afterRefund.heading)
        assertEquals(true, afterRefund.headingWarn)
        assertEquals(listOf("Rung up", "Closed: customer not paying", "Refunded in Square", "Paid"), afterRefund.facts.map { it.label })

        val theirs = page("expiredTheirs")
        assertEquals(false, theirs.order, "what was bought stays on that till")
        assertEquals(emptyList(), theirs.actions)
        assertEquals(listOf(Fact("Till", "Back till"), Fact("Rung up", "3:48 PM"), Fact("Code expired", "3:58 PM")), theirs.facts)
        assertEquals(true, theirs.note!!.text.startsWith("Back till rang this sale up."))
        assertEquals(listOf("Till", "Rung up", "Closed here"), page("closedTheirs").facts.map { it.label })

        val cancelled = page("cancelled")
        assertEquals("The order was cancelled in Monokulo. Square has this sale as Monero: refund it if the customer isn't paying.", cancelled.note?.text)
        assertEquals("Cancelled", cancelled.facts.last().label)
    }

    @Test
    fun `the till brings up a new live sale, keeps a sale on screen, and steps back from one paid on it`() {
        val waiting = sale("w")
        fun onShift(sales: List<TillSale>, left: Set<String> = emptySet(), liveBefore: Set<String> = emptySet()) =
            TillNav.follow(Screen.Till, sales, left, liveBefore, restingOnShift = true)
        assertEquals(Screen.Sale("w"), onShift(listOf(waiting)))
        assertEquals(Screen.Till, onShift(listOf(waiting), left = setOf("w")), "the cashier left it")
        assertEquals(Screen.Sale("w2"), onShift(listOf(sale("w2"), waiting), left = setOf("w")), "the newest they haven't left")
        assertEquals(Screen.Sale("waiting"), onShift(shift.reversed()), "past the paid and ended ones")
        assertEquals(Screen.Till, onShift(shift.drop(1)), "closed and ended sales never come up on their own")
        assertEquals(Screen.Sale("paid"), TillNav.follow(Screen.Sale("paid"), listOf(waiting) + shift, emptySet(), emptySet(), true), "a sale on screen stays")
        assertEquals(Screen.Code("w"), TillNav.follow(Screen.Code("w"), listOf(waiting), emptySet(), emptySet(), true))
        assertEquals(Screen.Sale("paid"), TillNav.follow(Screen.Code("paid"), shift, emptySet(), emptySet(), false), "the enlarged code gives way to the page")
        assertEquals(Screen.Sale("expired"), TillNav.follow(Screen.Order("expired"), shift, emptySet(), emptySet(), false))
        assertEquals(Screen.Settings, TillNav.follow(Screen.Settings, listOf(waiting), emptySet(), emptySet(), true))
        val paid = waiting.copy(phase = SalePhase.Paid)
        assertEquals("w", TillNav.paidOnScreen(Screen.Sale("w"), listOf(paid), liveBefore = setOf("w")))
        assertNull(TillNav.paidOnScreen(Screen.Sale("w"), listOf(paid), liveBefore = emptySet()), "opened from the shift once paid: it stays")
        assertNull(TillNav.paidOnScreen(Screen.Till, listOf(paid), liveBefore = setOf("w")))
    }

    @Test
    fun `the order card lists four lines, else three and how many more`() {
        fun receipt(lines: Int) = Receipt(
            receipt = Receipt.Info("k4Rt", "2026-10-22T16:07:00-07:00"),
            merchant = Receipt.Merchant("The Bean Room", "US"),
            lineItems = List(lines) { Receipt.LineItem("Item $it", 1.0, 1.0, 1.0) },
            total = lines.toDouble(),
            currency = "USD",
        )
        assertEquals(4 to 0, SaleText.shownLines(receipt(4)).let { it.first.size to it.second })
        assertEquals(3 to 5, SaleText.shownLines(receipt(8)).let { it.first.size to it.second })
        assertNull(SaleText.tax(receipt(1)), "no tax lines: the last row is the total")
        assertEquals("1.00 USD", SaleText.total(receipt(1)))
        assertEquals("Square receipt k4Rt · 8 items · 8.00 USD", SaleText.receiptSummary(receipt(8), PointOfSale.Square))
        assertEquals("Clover order k4Rt · 1 item · 1.00 USD", SaleText.receiptSummary(receipt(1), PointOfSale.Clover), "folded, the card says whose and how much")
        val taxed = receipt(2).copy(tax = Receipt.Tax(listOf(Receipt.TaxLine(0.05, 0.5), Receipt.TaxLine(0.08875, 0.63))))
        assertEquals("Tax 1.13", SaleText.tax(taxed))
        assertEquals("Scan with any Monero wallet · 14:52 left", SaleText.hint(sale("w"), 892))
        assertEquals("Ask the customer to pay the rest to the same code · 9:12 left", SaleText.hint(sale("w", SalePhase.Partial), 552))
        assertEquals("The customer can go: it's being confirmed", SaleText.hint(sale("w", SalePhase.Seen), 0))
        val paid = sale("w", SalePhase.Paid, paidAt = 990)
        assertEquals("${SaleText.xmr(paid)} received", SaleText.hint(paid, 0), "paid, the foot says what came in, not the clock")
    }

    @Test
    fun `the rows make one card, rounded at its top and bottom`() {
        assertEquals(RowEnds.Only, ShiftList.ends(0, 0), "one row is the whole card")
        assertEquals(RowEnds.First, ShiftList.ends(0, 2))
        assertEquals(RowEnds.Middle, ShiftList.ends(1, 2))
        assertEquals(RowEnds.Last, ShiftList.ends(2, 2))
        assertEquals(RowEnds.Last, ShiftList.ends(1, 1), "of two, the second is the bottom")
    }

    @Test
    fun aPaymentBuzzesOnceOnTheSurfaceTheCashierIsLookingAt() {
        assertEquals(PaidBuzz.Surface.App, PaidBuzz.by(appVisible = true), "the app on screen: the sale turning paid, or its row")
        assertEquals(PaidBuzz.Surface.Bubble, PaidBuzz.by(appVisible = false), "over Square: the bubble")
        // One surface for any answer, so a payment shown by the sale screen and a bubble still leaving buzzes once.
        for (visible in listOf(true, false)) assertEquals(1, PaidBuzz.Surface.entries.count { it == PaidBuzz.by(visible) })
        assertTrue(PaidBuzz.bubble(appVisible = false))
        assertFalse(PaidBuzz.bubble(appVisible = true), "the sale screen in front buzzes instead")
    }

    @Test
    fun aPaymentLandsWhenASaleThatWasLiveIsPaid() {
        val paid = sale("w", phase = SalePhase.Paid, paidAt = at)
        assertTrue(TillNav.paymentLanded(listOf(paid), liveBefore = setOf("w")))
        assertFalse(TillNav.paymentLanded(listOf(paid), liveBefore = emptySet()), "a sale already paid at the last look: nothing new")
        assertFalse(TillNav.paymentLanded(listOf(sale("w")), liveBefore = setOf("w")), "still waiting")
        assertFalse(
            TillNav.paymentLanded(listOf(sale("w", phase = SalePhase.Expired, endedAt = at)), liveBefore = setOf("w")),
            "ended unpaid is no payment",
        )
    }

    /** [n] live sales, newest first, then [ended] that aren't: as the till keeps a busy shift. */
    private fun busy(n: Int, ended: List<TillSale> = listOf(sale("paid", SalePhase.Paid, paidAt = at), sale("expired", SalePhase.Expired, endedAt = at))) =
        List(n) { sale("w${n - it}") } + ended

    /** The items in words, so a test reads as the list does. */
    private fun read(items: List<ShiftItem>) = items.map {
        when (it) {
            is ShiftItem.TrayHeader -> "header ${it.count}"
            is ShiftItem.Row -> (if (it.tray) "tray " else "") + it.sale.orderId + " " + it.ends
            is ShiftItem.Fold -> ShiftList.foldLabel(it)
            ShiftItem.Earlier -> "earlier"
        }
    }

    @Test
    fun theTrayFoldsAfterThreeRowsAndSaysHowManyMoreWait() {
        assertEquals(
            listOf("header 4", "tray w4 First", "tray w3 Middle", "tray w2 Last", "+1", "earlier", "paid First", "expired Last"),
            read(ShiftList.sections(busy(4), ShiftFilter.All, folded = true)),
        )
        val many = ShiftList.sections(busy(21), ShiftFilter.All, folded = true)
        assertEquals(ShiftItem.TrayHeader(21), many.first(), "the header counts every live sale, folded ones too")
        assertEquals(3, many.count { it is ShiftItem.Row && it.tray })
        assertEquals("+18", ShiftList.foldLabel(many.filterIsInstance<ShiftItem.Fold>().single()))
    }

    @Test
    fun theTrayOpensInPlaceAndSaysShowFewer() {
        val open = ShiftList.sections(busy(21), ShiftFilter.All, folded = false)
        val tray = open.filterIsInstance<ShiftItem.Row>().filter { it.tray }
        assertEquals((21 downTo 1).map { "w$it" }, tray.map { it.sale.orderId }, "every live sale, newest first")
        assertEquals(listOf(RowEnds.First) + List(19) { RowEnds.Middle } + RowEnds.Last, tray.map { it.ends })
        val fold = open.filterIsInstance<ShiftItem.Fold>().single()
        assertEquals(ShiftItem.Fold(hidden = 18, open = true), fold)
        assertEquals("Show fewer", ShiftList.foldLabel(fold))
        assertEquals(open.indexOf(tray.last()) + 1, open.indexOf(fold), "the fold row closes the tray")
    }

    @Test
    fun twoWaitingSalesHaveNoFold() {
        assertEquals(
            listOf("header 2", "tray w2 First", "tray w1 Last", "earlier", "paid First", "expired Last"),
            read(ShiftList.sections(busy(2), ShiftFilter.All, folded = true)),
        )
        assertEquals(listOf("header 3", "tray w3 First", "tray w2 Middle", "tray w1 Last"), read(ShiftList.sections(busy(3, ended = emptyList()), ShiftFilter.All, folded = true)))
        assertEquals(listOf("header 1", "tray w1 Only"), read(ShiftList.sections(busy(1, ended = emptyList()), ShiftFilter.All, folded = true)), "and no Earlier with nothing under it")
    }

    @Test
    fun withNoLiveSaleTheListReadsAsItAlwaysHas() {
        val ended = shift.drop(1)
        for (filter in ShiftFilter.entries) {
            val items = ShiftList.sections(ended, filter, folded = true)
            assertTrue(items.all { it is ShiftItem.Row && !it.tray }, "$filter: no header, fold or Earlier")
            val rows = ShiftList.rows(ended, filter)
            assertEquals(rows.mapIndexed { i, s -> ShiftItem.Row(s, ShiftList.ends(i, rows.lastIndex), tray = false) }, items)
        }
        assertEquals(emptyList(), ShiftList.sections(emptyList(), ShiftFilter.All, folded = true))
    }

    @Test
    fun needsActionShowsTheWholeTrayThenWhatEndedUnpaid() {
        val items = ShiftList.sections(busy(5), ShiftFilter.NeedsAction, folded = true)
        assertEquals(
            listOf("header 5", "tray w5 First", "tray w4 Middle", "tray w3 Middle", "tray w2 Middle", "tray w1 Last", "earlier", "expired Only"),
            read(items),
            "unfolded, with no fold row, whatever the fold's state; the paid sale isn't one that needs action",
        )
        assertEquals(read(items), read(ShiftList.sections(busy(5), ShiftFilter.NeedsAction, folded = false)))
    }

    @Test
    fun paidHasNoTrayAndNoHeaders() {
        assertEquals(listOf("paid Only"), read(ShiftList.sections(busy(5), ShiftFilter.Paid, folded = true)))
    }

    @Test
    fun bothSectionsKeepTheShiftsOrderNewestFirst() {
        // The till keeps the shift newest first; a sale paid since lands at the top of Earlier, a new one at the top of the tray.
        val sales = listOf(sale("new"), sale("justPaid", SalePhase.Paid, paidAt = at), sale("old"), sale("expired", SalePhase.Expired, endedAt = at))
        assertEquals(
            listOf("header 2", "tray new First", "tray old Last", "earlier", "justPaid First", "expired Last"),
            read(ShiftList.sections(sales, ShiftFilter.All, folded = true)),
        )
    }

    @Test
    fun talkBackCountsTheTrayAndTheFoldInWords() {
        assertEquals("Waiting for payment, 1 sale", ShiftList.trayHeaderSaid(1))
        assertEquals("Waiting for payment, 21 sales", ShiftList.trayHeaderSaid(21))
        assertEquals("Show 1 more waiting sale", ShiftList.foldSaid(ShiftItem.Fold(1, open = false)))
        assertEquals("Show 18 more waiting sales", ShiftList.foldSaid(ShiftItem.Fold(18, open = false)))
        assertEquals("Show fewer", ShiftList.foldSaid(ShiftItem.Fold(18, open = true)))
    }

    /** Decision 266: Back always lands on the shift; only a sale that arrives while the shift rests on screen rises. */
    @Test
    fun aSaleRisesOnlyIfItArrivesWhileTheShiftRests() {
        val old = sale("old")
        val new = sale("new")
        assertEquals(Screen.Sale("new"), TillNav.follow(Screen.Till, listOf(new, old), emptySet(), liveBefore = setOf("old"), restingOnShift = true), "new since the last look, on the resting shift")
        assertEquals(Screen.Till, TillNav.follow(Screen.Till, listOf(new, old), emptySet(), liveBefore = setOf("new", "old"), restingOnShift = true), "seen at the last look (it came while the cashier was elsewhere): it waits in the tray")
        assertEquals(Screen.Till, TillNav.follow(Screen.Till, listOf(new, old), emptySet(), liveBefore = setOf("old"), restingOnShift = false), "arriving under a Back still landing: into the tray, not over the move")
        assertEquals(Screen.Till, TillNav.follow(Screen.Till, listOf(new, old), setOf("new"), liveBefore = setOf("old"), restingOnShift = true), "a sale the cashier left never rises")
    }

    /** What the model does with a sale that arrives on another screen: at the next look it is known, so it never rises later. */
    @Test
    fun aSaleThatArrivedElsewhereNeverRisesLater() {
        val a = sale("a")
        val b = sale("b")
        var liveBefore = emptySet<String>()
        fun look(screen: Screen, sales: List<TillSale>, resting: Boolean): Screen =
            TillNav.follow(screen, sales, setOf("a"), liveBefore, resting).also { liveBefore = sales.filter { it.live }.map { it.orderId }.toSet() }
        for (elsewhere in listOf(Screen.Sale("a"), Screen.Settings, Screen.Code("a"), Screen.Order("a"))) {
            liveBefore = setOf("a")
            assertEquals(elsewhere, look(elsewhere, listOf(b, a), resting = true), "on $elsewhere b doesn't come up")
            // Back on the shift, at rest, with another update: b was seen, so it stays in the tray.
            assertEquals(Screen.Till, look(Screen.Till, listOf(b, a), resting = true), "after $elsewhere")
        }
        liveBefore = setOf("a")
        assertEquals(Screen.Till, look(Screen.Till, listOf(b, a), resting = false), "mid-Back")
        assertEquals(Screen.Till, look(Screen.Till, listOf(b, a), resting = true), "and not on landing either")
    }
}
