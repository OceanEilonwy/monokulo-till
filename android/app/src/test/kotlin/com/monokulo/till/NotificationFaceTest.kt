package com.monokulo.till

import com.monokulo.till.core.SalePhase
import com.monokulo.till.notify.Face
import com.monokulo.till.notify.Ongoing
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.Fixed
import androidx.compose.ui.graphics.toArgb
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class NotificationFaceTest {
    private val receipt = Receipt(
        receipt = Receipt.Info("bP9m", "2026-10-22T16:07:00-07:00"),
        merchant = Receipt.Merchant("The Bean Room", "US"),
        lineItems = listOf(
            Receipt.LineItem("Flat white", 2.0, 4.5, 9.0),
            Receipt.LineItem("Almond croissant", 1.0, 3.75, 3.75),
        ),
        total = 13.88,
        currency = "USD",
    )

    /** "$13.88" in this machine's locale ("US$13.88" where the own dollar is another). */
    private val amount = SaleText.amount(sale())

    private fun sale(
        id: String = "o1",
        phase: SalePhase = SalePhase.Waiting,
        receipt: Receipt? = this.receipt,
        found: Boolean = true,
        closedAt: Long? = null,
    ) = TillSale(
        orderId = id,
        amount = "13.88",
        currency = "USD",
        reference = "bP9m",
        code = PaymentCode("monero:4abc?tx_amount=0.05552", hasReceipt = receipt != null),
        phase = phase,
        expiresAt = 0,
        xmr = "0.05552",
        address = "4abc",
        found = if (found) FoundSale("p1", 1388, "USD", "bP9m", "ord", "2026-10-22T23:07:00Z", null) else null,
        receipt = receipt,
        deviceName = if (found) null else "Back till",
        closedAt = closedAt,
    )

    @Test
    fun `a new sale says what was bought, on the private version only`() {
        val face = Face.of(sale(), "Front Counter")
        assertEquals(R.drawable.ic_mark_q1, face.icon)
        assertEquals(Fixed.Accent.toArgb(), face.color)
        assertEquals("New Monero sale · Front Counter", face.title)
        assertEquals("$amount · 3 items · waiting for payment", face.text)
        assertEquals("2 × Flat white, Almond croissant", face.detail)
        assertEquals("$amount · waiting for payment", face.publicText, "the lock screen never shows an item or a count")
        assertEquals(1, face.step)
        assertTrue(face.fullScreen)
    }

    @Test
    fun `the chip's ring fills a quarter a step`() {
        assertEquals(R.drawable.ic_mark_q1, Face.of(sale(phase = SalePhase.Partial), null).icon)
        val seen = Face.of(sale(phase = SalePhase.Seen), null)
        assertEquals(R.drawable.ic_mark_q2, seen.icon)
        assertEquals("Seen on the network", seen.title)
        assertEquals("$amount · 3 items · waiting for confirmation", seen.text)
        assertFalse(seen.fullScreen, "once seen the customer has scanned")
        val confirming = Face.of(sale(phase = SalePhase.Confirming), null)
        assertEquals(R.drawable.ic_mark_q3, confirming.icon)
        assertEquals("$amount · 3 items · nearly there", confirming.text)
        assertEquals(3, confirming.step)
    }

    @Test
    fun `another till's sale has no items to tell`() {
        val face = Face.of(sale(receipt = null, found = false), "Front Counter")
        assertEquals("$amount · waiting for payment", face.text)
        assertNull(face.detail)
        assertEquals("New Monero sale · Front Counter", face.title)
        assertEquals("New Monero sale · From Back till", Face.of(sale(receipt = null, found = false), null).title)
    }

    @Test
    fun `a long order lists every line and lets Android cut it short`() {
        val long = receipt.copy(lineItems = receipt.lineItems + List(4) { Receipt.LineItem("Chai", 1.0, 4.75, 4.75) })
        assertEquals("2 × Flat white, Almond croissant, Chai, Chai, Chai, Chai", Face.of(sale(receipt = long), null).detail)
    }

    @Test
    fun `whom a sale is for is never on a notification, private version or public`() {
        val named = sale().copy(customerName = "Ana López")
        for (face in listOf(Face.of(named, "Front Counter"), Face.paid(named.copy(phase = SalePhase.Paid)))) {
            assertFalse(listOfNotNull(face.title, face.text, face.detail, face.publicText).any { "Ana" in it }, face.toString())
        }
        assertEquals("Paid · Sale #bP9m", Face.paid(named.copy(phase = SalePhase.Paid)).title)
    }

    @Test
    fun `paid is a green tick and the amount received`() {
        val face = Face.paid(sale(phase = SalePhase.Paid))
        assertEquals(R.drawable.ic_tick, face.icon)
        assertEquals(Fixed.Paid.toArgb(), face.color)
        assertEquals("Paid · Sale #bP9m", face.title)
        assertEquals("$amount · 0.05552 XMR received", face.text)
        assertNull(face.detail)
        assertEquals(4, face.step)
        assertFalse(face.fullScreen)
    }

    @Test
    fun `the ongoing notification follows the newest live sale, a sale just paid, or watching`() {
        val waiting = sale("o2")
        val paid = sale("o1", phase = SalePhase.Paid)
        assertEquals(Ongoing.Paid(paid), Ongoing.of(listOf(waiting, paid), justPaid = "o1"), "the tick first, for a few seconds")
        assertEquals(Ongoing.Sale(waiting), Ongoing.of(listOf(waiting, paid), justPaid = null), "then the sale still waiting")
        assertEquals(Ongoing.Watching, Ongoing.of(listOf(paid), justPaid = null), "then back to watching")
        assertEquals(Ongoing.Watching, Ongoing.of(listOf(sale(closedAt = 5)), justPaid = null), "a closed sale drops the chip at once")
        for (phase in listOf(SalePhase.Expired, SalePhase.Cancelled, SalePhase.DoubleSpend)) {
            assertEquals(Ongoing.Watching, Ongoing.of(listOf(sale(phase = phase)), justPaid = "o1"), "$phase: no chip, and no tick")
        }
    }
}
