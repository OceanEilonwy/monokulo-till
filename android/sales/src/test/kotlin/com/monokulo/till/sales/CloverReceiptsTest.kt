package com.monokulo.till.sales

import com.monokulo.till.receipt.ReceiptCode
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import uniffi.orf_codec_ffi.receiptFromUri
import java.time.Instant
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Clover's JSON as the till reads it, unknown fields passed over; one instance for the tests. */
private val cloverJson = Json { ignoreUnknownKeys = true }

/** A Clover order as the receipt in the code, built on the till. */
class CloverReceiptsTest {
    private val server = MockWebServer()
    private val merchant = CloverMerchant("MCLOVER00001", "The Bean Room", "USD", "America/Los_Angeles", "US")
    private val sale = FoundSale("PAY_MONERO_1", 2205, "USD", null, "ORDER_CL_1", "2026-10-28T08:56:00Z", "DEV_FRONT")
    private val order = cloverJson.decodeFromString(CloverOrder.serializer(), cloverResource("order.json"))

    @BeforeTest fun start() = server.start()

    @AfterTest fun stop() = server.close()

    @Test
    fun `a sale's receipt is read from its order through the source, and fits in the code`() = runBlocking {
        val tokens = MemoryTokens(PosTokens("acc1", "2026-10-28T09:30:00Z", "ref1", "MCLOVER00001"))
        val config = CloverConfig("CLOVERAPP0001", CloverEnvironment.NorthAmerica, "https://till.monokulo.com/clover", baseUrl = server.url("/").toString().trimEnd('/'))
        val now = { Instant.parse("2026-10-28T09:00:00Z") }
        val api = CloverApi(OkHttpClient(), config, CloverAuth(OkHttpClient(), config, tokens, now))
        val source = CloverSource(api, config, merchant, MemoryCursor(), MemoryTenders(), now)
        server.enqueue(MockResponse.Builder().code(200).body(cloverResource("order.json")).build())
        val receipt = assertNotNull(source.order(sale).receipt)
        val request = server.takeRequest()
        assertEquals("/v3/merchants/MCLOVER00001/orders/ORDER_CL_1", request.url.encodedPath)
        assertEquals("lineItems", request.url.queryParameter("expand"))

        val code = ReceiptCode.of("monero:8Bx?tx_amount=0.1", receipt)
        assertTrue(code.hasReceipt, code.why ?: "")
        val decoded = receiptFromUri(code.uri)!!
        assertTrue(decoded.contains("Granola, per 100 g") && decoded.contains("The Bean Room"), decoded)
    }

    @Test
    fun `equal line items are counted onto one line, and a measured one has its decimal quantity`() {
        val receipt = assertNotNull(CloverReceipts.of(order, merchant, sale))
        assertEquals(
            listOf("Flat white", "Almond croissant", "Flat white, oat milk", "Granola, per 100 g"),
            receipt.lineItems.map { it.description },
        )
        assertEquals(listOf(2.0, 1.0, 1.0, 2.5), receipt.lineItems.map { it.quantity })
        assertEquals(listOf(4.5, 3.75, 4.5, 1.2), receipt.lineItems.map { it.unitPrice })
        assertEquals(listOf(9.0, 3.75, 4.5, 3.0), receipt.lineItems.map { it.lineTotal })
    }

    @Test
    fun `the tax is one line, the order's total less its lines, and the total is the order's`() {
        val receipt = assertNotNull(CloverReceipts.of(order, merchant, sale))
        val tax = receipt.tax!!.taxLines.single()
        assertEquals(1.8, tax.taxAmount)
        assertEquals(0.08889, tax.rate, "1.80 on 20.25")
        assertEquals(22.05, receipt.total)
        assertEquals("USD", receipt.currency)
        assertEquals("monero", receipt.payment?.paymentMethod)
        assertNull(receipt.extensions)
    }

    @Test
    fun `the receipt names the merchant, the order and its time in the merchant's zone`() {
        val receipt = assertNotNull(CloverReceipts.of(order, merchant, sale))
        assertEquals("DER_CL_1", receipt.receipt.receiptId, "Clover prints no number on the payment")
        assertEquals("2026-10-28T01:55:58-07:00", receipt.receipt.issueTimestamp)
        assertEquals("The Bean Room", receipt.merchant.legalName)
        assertEquals("US", receipt.merchant.countryCode)
    }

    @Test
    fun `an order for less than its lines took a discount off, and has no tax`() {
        val discounted = order.copy(total = 1900)
        val receipt = assertNotNull(CloverReceipts.of(discounted, merchant, sale))
        assertNull(receipt.tax)
        assertEquals(19.0, receipt.total)
        assertEquals("""{"discount":{"amount":1.25}}""", receipt.extensions.toString())
    }

    @Test
    fun `a currency without cents keeps its own digits`() {
        val yen = CloverOrder(
            "ORDER_JP_0001", "JPY", total = 1100, createdTime = 1793177758000,
            lineItems = Elements(listOf(CloverOrder.LineItem("L1", "Matcha", 500), CloverOrder.LineItem("L2", "Matcha", 500))),
        )
        val receipt = assertNotNull(CloverReceipts.of(yen, merchant.copy(currency = "JPY", timezone = "Asia/Tokyo", country = "JP"), sale))
        assertEquals(500.0, receipt.lineItems.single().unitPrice)
        assertEquals(2.0, receipt.lineItems.single().quantity)
        assertEquals(100.0, receipt.tax!!.taxLines.single().taxAmount)
        assertEquals(0.1, receipt.tax!!.taxLines.single().rate)
    }

    @Test
    fun `an order of as many line items as Clover expands may be cut short, so it gets the plain code`() {
        // 140 coffees rung up, of which Clover sends the first 100: the other 40's money isn't tax.
        val coffees = List(CloverReceipts.EXPANDED_MAX) { CloverOrder.LineItem("L$it", "Drip coffee", 300) }
        val cut = CloverOrder("ORDER_BIG_01", "USD", total = 140 * 300, createdTime = 1793177758000, lineItems = Elements(coffees))
        assertNull(CloverReceipts.of(cut, merchant, sale))
        val whole = cut.copy(lineItems = Elements(coffees.dropLast(1)), total = 99 * 300)
        assertEquals(99.0, assertNotNull(CloverReceipts.of(whole, merchant, sale)).lineItems.single().quantity, "one fewer is the whole order")
    }
}
