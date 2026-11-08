package com.monokulo.till.core

import okhttp3.HttpUrl.Companion.toHttpUrl
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class SaleEventTest {
    @Test
    fun `an event from the stream decodes, ignoring fields it doesn't know, its order link resolved`() {
        val event = SaleEvent.decode(
            """{"event":"sale.paid","order_id":"ord_1","amount":"24.50","currency":"USD",
               "address":"8Bx","xmr_amount_piconero":141900000000,"merchant_order_id":"#1042",
               "expires_at":1761312000,"device_name":"Front counter","at":1761310260,"later":true,
               "_links":{"mk:order":{"href":"/monokulo/api/v1/orders/ord_1"}}}""",
            "https://example.com/monokulo/api/v1/events".toHttpUrl(),
        )
        assertEquals("https://example.com/monokulo/api/v1/orders/ord_1", event.orderUrl)
        assertEquals(SalePhase.Paid, event.phase)
        assertEquals("#1042", event.merchantOrderId)
        assertTrue(event.phase!!.isFinal)
    }

    @Test
    fun `every event name has its phase`() {
        mapOf(
            "sale.waiting" to SalePhase.Waiting,
            "sale.seen" to SalePhase.Seen,
            "sale.confirming" to SalePhase.Confirming,
            "sale.paid" to SalePhase.Paid,
            "sale.partial" to SalePhase.Partial,
            "sale.expired" to SalePhase.Expired,
            "sale.double_spend" to SalePhase.DoubleSpend,
        ).forEach { (name, phase) -> assertEquals(phase, SalePhase.ofEvent(name)) }
        assertNull(SalePhase.ofEvent("ready"))
        assertNull(SalePhase.ofEvent("sale.cancelled"), "Monokulo never sends one: only the POS cancels, and only its own orders")
    }

    @Test
    fun `a code counts down until some of its money is seen, and part paid still counts down`() {
        assertEquals(setOf(SalePhase.Waiting, SalePhase.Partial), SalePhase.entries.filter { it.countsDown }.toSet())
    }

    @Test
    fun `a status puts a sale in its phase`() {
        mapOf(
            "pending" to SalePhase.Waiting,
            "unconfirmed" to SalePhase.Seen,
            "confirming" to SalePhase.Confirming,
            "overpaid" to SalePhase.Paid,
            "paid" to SalePhase.Paid,
            "partial" to SalePhase.Partial,
            "expired" to SalePhase.Expired,
            "cancelled" to SalePhase.Cancelled,
        ).forEach { (status, phase) -> assertEquals(phase, Order.phaseOf(status), status) }
    }

    @Test
    fun `payment uris carry the amount in XMR without trailing zeros`() {
        assertEquals("0.080370584829", PaymentUri.xmr(80_370_584_829))
        assertEquals("2", PaymentUri.xmr(2_000_000_000_000))
        assertEquals("0.5", PaymentUri.xmr(500_000_000_000))
        assertEquals("monero:8Bx?tx_amount=0.5", PaymentUri.build("8Bx", 500_000_000_000))
    }
}
