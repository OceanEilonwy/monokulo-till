package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.CodeText
import com.monokulo.till.ui.OrderText
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.time.ZoneId
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class OrderPageTest {
    /** The design's order: 15 items on 8 lines. */
    private val receipt = Receipt(
        receipt = Receipt.Info("k4Rt", "2026-10-22T16:07:00-07:00"),
        merchant = Receipt.Merchant("The Bean Room", "US"),
        lineItems = listOf(
            Receipt.LineItem("Flat white", 2.0, 4.5, 9.0),
            Receipt.LineItem("Oat latte", 1.0, 5.25, 5.25),
            Receipt.LineItem("Almond croissant", 3.0, 3.75, 11.25),
            Receipt.LineItem("Banana bread", 1.0, 4.5, 4.5),
            Receipt.LineItem("Cold brew", 2.0, 5.0, 10.0),
            Receipt.LineItem("Chai", 1.0, 4.75, 4.75),
            Receipt.LineItem("Choc chip cookie", 4.0, 2.5, 10.0),
            Receipt.LineItem("House blend beans, 250 g", 1.0, 18.0, 18.0),
        ),
        tax = Receipt.Tax(listOf(Receipt.TaxLine(0.08875, 6.46))),
        total = 79.21,
        currency = "USD",
    )

    private fun sale(phase: SalePhase = SalePhase.Waiting, reference: String? = "k4Rt") = TillSale(
        orderId = "o1",
        amount = "79.21",
        currency = "USD",
        reference = reference,
        code = PaymentCode("monero:4abc", hasReceipt = true),
        phase = phase,
        expiresAt = 0,
        xmr = "0.31684",
        address = "4abc",
        found = FoundSale("p1", 7921, "USD", reference, "ord", "2026-10-22T23:07:00Z", null),
        receipt = receipt,
    )

    @Test
    fun `the order counts items and lines`() {
        assertEquals("15 items", SaleText.items(receipt))
        assertEquals("8 lines", OrderText.lines(receipt))
        val one = receipt.copy(lineItems = receipt.lineItems.take(2).drop(1))
        assertEquals("1 item", SaleText.items(one))
        assertEquals("1 line", OrderText.lines(one))
        assertEquals("1 ×", OrderText.quantity(receipt.lineItems[1]), "the order's column says 1 × too")
        assertEquals("0.5 ×", OrderText.quantity(Receipt.LineItem("Beans", 0.5, 30.0, 15.0)))
    }

    @Test
    fun `tax rates print to three decimals at most`() {
        assertEquals("8.875%", OrderText.rate(0.08875))
        assertEquals("10%", OrderText.rate(0.1))
        assertEquals("8.25%", OrderText.rate(0.0825))
        assertEquals("7.123%", OrderText.rate(0.0712345))
    }

    @Test
    fun `the totals are the receipt's`() {
        assertEquals("72.75", SaleText.plain(OrderText.subtotal(receipt), "USD"))
        assertEquals("79.21 USD", OrderText.total(receipt))
        assertEquals(emptyList(), OrderText.extras(receipt))
        val extras = JsonObject(
            mapOf(
                "tip" to JsonObject(mapOf("amount" to JsonPrimitive(2.0))),
                "discount" to JsonObject(mapOf("amount" to JsonPrimitive(1.5))),
            ),
        )
        assertEquals(listOf("Tip" to "2.00", "Discount" to "−1.50"), OrderText.extras(receipt.copy(extensions = extras)))
        val yen = receipt.copy(lineItems = listOf(Receipt.LineItem("Matcha", 1.0, 1200.0, 1200.0)), tax = null, total = 1200.0, currency = "JPY")
        assertEquals("1200 JPY", OrderText.total(yen))
    }

    @Test
    fun `the subtitle names the sale, its Square receipt and when it was issued`() {
        val la = ZoneId.of("America/Los_Angeles")
        assertEquals("Sale #k4Rt · Square receipt k4Rt · 4:07 PM", OrderText.subtitle(sale(), receipt, PointOfSale.Square, Locale.US, la))
        assertEquals("Sale #k4Rt · Square receipt k4Rt · 16:07", OrderText.subtitle(sale(), receipt, PointOfSale.Square, Locale.UK, la))
        assertEquals("Monero sale · 4:07 PM", OrderText.subtitle(sale(reference = null), receipt, PointOfSale.Square, Locale.US, la), "no Square reference, no Square receipt")
    }

    @Test
    fun `under the code, the time left until the payment is seen`() {
        assertEquals("14:52 left", CodeText.left(sale(), 892))
        assertEquals("9:12 left", CodeText.left(sale(SalePhase.Partial), 552))
        assertNull(CodeText.left(sale(SalePhase.Seen), 892))
        assertEquals("0.31684 XMR · 14:52 left", CodeText.under(sale(), PointOfSale.Square, 892))
        assertEquals("0.31684 XMR · Seen on the network", CodeText.under(sale(SalePhase.Seen), PointOfSale.Square, 892))
        assertEquals("0.31684 XMR · Confirming", CodeText.under(sale(SalePhase.Confirming), PointOfSale.Square, 892))
    }
}
