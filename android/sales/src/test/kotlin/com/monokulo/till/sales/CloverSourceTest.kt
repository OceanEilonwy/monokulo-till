package com.monokulo.till.sales

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import java.time.Instant
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class MemoryTenders(val ids: MutableMap<String, String> = mutableMapOf()) : TenderStore {
    override fun tenderId(merchantId: String) = ids[merchantId]
    override fun save(merchantId: String, tenderId: String?) {
        if (tenderId == null) ids.remove(merchantId) else ids[merchantId] = tenderId
    }
}

fun cloverResource(name: String): String =
    CloverSourceTest::class.java.getResource("/clover/$name")!!.readText()

/** Clover's API through the till's Clover source, against a stand-in Clover. */
class CloverSourceTest {
    private val server = MockWebServer()
    private val tokens = MemoryTokens(PosTokens("acc1", "2026-10-28T09:30:00Z", "ref1", "MCLOVER00001"))
    private var now = Instant.parse("2026-10-28T09:00:00Z")
    private lateinit var config: CloverConfig
    private lateinit var api: CloverApi
    private val merchant = CloverMerchant("MCLOVER00001", "The Bean Room", "USD", "America/Los_Angeles", "US")
    private val tenders = MemoryTenders()

    @BeforeTest
    fun start() {
        server.start()
        config = CloverConfig(
            "CLOVERAPP0001", CloverEnvironment.NorthAmerica, "https://till.monokulo.com/clover",
            userAgent = "Till/0.3.0", baseUrl = server.url("/").toString().trimEnd('/'),
        )
        api = CloverApi(OkHttpClient(), config, CloverAuth(OkHttpClient(), config, tokens) { now })
    }

    @AfterTest fun stop() = server.close()

    private fun json(body: String) = MockResponse.Builder().code(200).setHeader("Content-Type", "application/json").body(body).build()

    private fun source(cursor: CursorStore = MemoryCursor("2026-10-28T08:57:00Z"), at: CloverMerchant = merchant) =
        CloverSource(api, config, at, cursor, tenders) { now }

    @Test
    fun `the merchant is read with its address and properties, and every request names the till`() = runBlocking {
        server.enqueue(json(cloverResource("merchant.json")))
        server.enqueue(json(cloverResource("properties.json")))
        assertEquals(merchant, api.merchant("MCLOVER00001"))
        val first = server.takeRequest()
        assertEquals("/v3/merchants/MCLOVER00001", first.url.encodedPath)
        assertEquals("address", first.url.queryParameter("expand"))
        assertEquals("Bearer acc1", first.headers["Authorization"])
        assertEquals("Till/0.3.0", first.headers["User-Agent"])
        assertEquals("/v3/merchants/MCLOVER00001/properties", server.takeRequest().url.encodedPath)
    }

    @Test
    fun `a merchant answer for another merchant is refused`() = runBlocking {
        server.enqueue(json("""{"id":"MOTHER000001","name":"Elsewhere"}"""))
        val e = assertFailsWith<CloverException> { api.merchant("MCLOVER00001") }
        assertTrue(e.signInAgain)
    }

    @Test
    fun `the country comes from the address, else the locale`() {
        assertEquals("US", CloverApi.country("us", null))
        assertEquals("FR", CloverApi.country(null, "fr_FR"))
        assertEquals("MX", CloverApi.country("", "es-MX"))
        assertNull(CloverApi.country(null, "en"))
        assertNull(CloverApi.country("United States", null))
    }

    @Test
    fun `the Monero tender is found by its label, whatever its case and spaces, and remembered`() = runBlocking {
        server.enqueue(json(cloverResource("tenders.json")))
        val found = assertIs<TenderState.Ready>(source().ensureTender())
        assertEquals("T_MONERO", found.tender.id)
        assertEquals("T_MONERO", tenders.ids["MCLOVER00001"])
        server.enqueue(json(cloverResource("tenders.json")))
        assertEquals(found, source().ensureTender())
        assertEquals(listOf("GET", "GET"), List(server.requestCount) { server.takeRequest().method }, "nothing made")
    }

    @Test
    fun `with no Monero tender one is made as an external payment, and remembered`() = runBlocking {
        server.enqueue(json("""{"elements":[{"id":"T_CASH","label":"Cash","labelKey":"com.clover.tender.cash"}]}"""))
        server.enqueue(json("""{"id":"T_NEW","label":"Monero","labelKey":"com.clover.tender.external_payment","enabled":true,"visible":true}"""))
        val made = assertIs<TenderState.Ready>(source().ensureTender())
        assertEquals("T_NEW", made.tender.id)
        assertEquals("T_NEW", tenders.ids["MCLOVER00001"])
        server.takeRequest()
        val post = server.takeRequest()
        assertEquals("POST", post.method)
        assertEquals("/v3/merchants/MCLOVER00001/tenders", post.url.encodedPath)
        val body = Json.parseToJsonElement(post.body!!.utf8()).jsonObject
        assertEquals("Monero", body["label"]!!.jsonPrimitive.content)
        assertEquals("com.clover.tender.external_payment", body["labelKey"]!!.jsonPrimitive.content)
        assertEquals("false", body["opensCashDrawer"]!!.jsonPrimitive.content)
        assertEquals("false", body["supportsTipping"]!!.jsonPrimitive.content)
    }

    @Test
    fun `a refused tender is added in Clover by the cashier, and the till never makes it on its own`() = runBlocking {
        server.enqueue(json("""{"elements":[]}"""))
        server.enqueue(MockResponse.Builder().code(403).body("""{"message":"Forbidden: missing MERCHANT_W"}""").build())
        val add = assertIs<TenderState.AddInClover>(source().ensureTender())
        assertEquals("Forbidden: missing MERCHANT_W", add.reason)
        assertNull(tenders.ids["MCLOVER00001"])
        // The timer: tenders looked at, never posted, and no payments asked for without one.
        server.enqueue(json("""{"elements":[]}"""))
        assertEquals(emptyList(), source().newSales())
        assertEquals(listOf("GET", "POST", "GET"), List(server.requestCount) { server.takeRequest().method })
        // The cashier added it in Clover: the next look finds it and reads payments.
        server.enqueue(json(cloverResource("tenders.json")))
        server.enqueue(json("""{"elements":[]}"""))
        source().newSales()
        assertEquals("T_MONERO", tenders.ids["MCLOVER00001"])
    }

    @Test
    fun `a remembered tender deleted in Clover is found again`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_GONE"
        server.enqueue(json(cloverResource("tenders.json")))
        assertEquals("T_MONERO", assertIs<TenderState.Ready>(source().ensureTender()).tender.id)
        assertEquals("T_MONERO", tenders.ids["MCLOVER00001"])
    }

    @Test
    fun `only successful Monero payments not voided, refunded or handled are sales, oldest first`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        val cursor = MemoryCursor("2026-10-28T08:57:00Z")
        val source = source(cursor)
        server.enqueue(json(cloverResource("payments.json")))
        val sales = source.newSales()
        assertEquals(listOf("PAY_MONERO_0", "PAY_MONERO_1", "PAY_MONERO_2"), sales.map { it.id }, "a voided refund doesn't count")
        val sale = sales[1]
        assertEquals(FoundSale("PAY_MONERO_1", 2205, "USD", null, "ORDER_CL_1", "2026-10-28T08:56:00Z", "DEV_FRONT", "Table 4", PointOfSale.Clover), sale)
        assertEquals(listOf(null, "Table 4", null), sales.map { it.title }, "an order with no title has none")
        val request = server.takeRequest()
        assertEquals("/v3/merchants/MCLOVER00001/payments", request.url.encodedPath)
        assertEquals(
            listOf("tender.id=T_MONERO", "createdTime>=${Instant.parse("2026-10-28T08:55:00Z").toEpochMilli()}"),
            request.url.queryParameterValues("filter"),
            "two minutes of overlap",
        )
        assertEquals("tender,order,refunds", request.url.queryParameter("expand"))
        assertEquals("createdTime ASC", request.url.queryParameter("orderBy"))
        assertEquals("100", request.url.queryParameter("limit"))
        assertEquals("0", request.url.queryParameter("offset"))

        source.handled(sale)
        assertEquals(setOf("PAY_MONERO_1"), cursor.handledIds)
        assertEquals("2026-10-28T08:57:00Z", cursor.sinceValue, "never back")
        server.enqueue(json(cloverResource("payments.json")))
        assertEquals(listOf("PAY_MONERO_0", "PAY_MONERO_2"), source.newSales().map { it.id })
    }

    @Test
    fun `payments are read page by page until a short one, and never further back than Clover allows`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        fun page(from: Int, count: Int) = json(
            (from until from + count).joinToString(",", """{"elements":[""", "]}") {
                """{"id":"P$it","amount":100,"result":"SUCCESS","createdTime":${1793177760000 + it},"tender":{"id":"T_MONERO"}}"""
            },
        )
        server.enqueue(page(0, 100))
        server.enqueue(page(100, 3))
        val sales = source(MemoryCursor("2026-01-01T00:00:00Z")).newSales()
        assertEquals(103, sales.size)
        val first = server.takeRequest()
        assertEquals(
            "createdTime>=${now.minus(java.time.Duration.ofDays(89)).toEpochMilli()}",
            first.url.queryParameterValues("filter")[1],
            "Clover filters payments 90 days back at most",
        )
        assertEquals("100", server.takeRequest().url.queryParameter("offset"))
        assertEquals(2, server.requestCount)
    }

    @Test
    fun `a first look starts watching from now`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        val cursor = MemoryCursor()
        server.enqueue(json("""{"elements":[]}"""))
        source(cursor).newSales()
        assertEquals("2026-10-28T09:00:00Z", cursor.sinceValue)
    }

    @Test
    fun `Clover asking the till to slow down holds the next look for its retry-after`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        val source = source()
        server.enqueue(MockResponse.Builder().code(429).setHeader("Retry-After", "30").build())
        val e = assertFailsWith<CloverException> { source.newSales() }
        assertEquals("Clover asked the till to slow down.", e.message)
        assertEquals(30, e.retryAfterSeconds)
        now = now.plusSeconds(29)
        assertEquals(emptyList(), source.newSales())
        assertEquals(1, server.requestCount, "nothing asked while held")
        now = now.plusSeconds(1)
        server.enqueue(json("""{"elements":[]}"""))
        source.newSales()
        assertEquals(2, server.requestCount)
    }

    @Test
    fun `a 429 without retry-after holds for a second`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        server.enqueue(MockResponse.Builder().code(429).build())
        assertEquals(1, assertFailsWith<CloverException> { source().newSales() }.retryAfterSeconds)
    }

    @Test
    fun `a token Clover ended early is refreshed and the request asked again`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        server.enqueue(MockResponse.Builder().code(401).body("""{"message":"401 Unauthorized"}""").build())
        server.enqueue(json("""{"access_token":"acc2","access_token_expiration":1793179800,"refresh_token":"ref2"}"""))
        server.enqueue(json("""{"elements":[]}"""))
        assertEquals(emptyList(), source().newSales())
        assertEquals("acc2", tokens.tokens?.accessToken)
        assertEquals("ref2", tokens.tokens?.refreshToken)
        assertEquals("Bearer acc1", server.takeRequest().headers["Authorization"])
        assertEquals("/oauth/v2/refresh", server.takeRequest().url.encodedPath)
        assertEquals("Bearer acc2", server.takeRequest().headers["Authorization"])
    }

    @Test
    fun `a fresh token refused too means signing in again, with no refresh on the next pass`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        server.enqueue(MockResponse.Builder().code(401).build())
        server.enqueue(json("""{"access_token":"acc2","access_token_expiration":1793179800,"refresh_token":"ref2"}"""))
        server.enqueue(MockResponse.Builder().code(401).build())
        val e: PosException = assertFailsWith<CloverException> { source().newSales() }
        assertTrue(e.signInAgain, "the till catches one type for every point of sale")
        assertNull(tokens.tokens)
        assertTrue(assertFailsWith<CloverException> { source().newSales() }.signInAgain)
        assertEquals(3, server.requestCount)
    }

    @Test
    fun `a refresh refused after an early end signs out`() = runBlocking {
        tenders.ids["MCLOVER00001"] = "T_MONERO"
        server.enqueue(MockResponse.Builder().code(401).build())
        server.enqueue(MockResponse.Builder().code(401).body("""{"message":"Refresh token revoked"}""").build())
        val e = assertFailsWith<CloverException> { source().newSales() }
        assertEquals("Clover's sign-in has ended. Sign in again.", e.message)
        assertNull(tokens.tokens)
    }

    @Test
    fun `a sale is refunded when its refunds reach its amount, it was voided, or its order reads refunded`() = runBlocking {
        val refunds = source().refunds
        val sale = FoundSale("PAY_MONERO_1", 2205, "USD", null, "ORDER_CL_1", "2026-10-28T08:56:00Z", "DEV_FRONT")

        server.enqueue(json(cloverResource("payment-refunded.json")))
        assertTrue(refunds.refunded(sale), "1000 + 1205, the voided one left out")
        val request = server.takeRequest()
        assertEquals("/v3/merchants/MCLOVER00001/payments/PAY_MONERO_1", request.url.encodedPath)
        assertEquals("tender,order,refunds", request.url.queryParameter("expand"))

        val partly = """{"id":"PAY_MONERO_1","amount":2205,"result":"SUCCESS","createdTime":1793177760000,"order":{"id":"ORDER_CL_1"},
            "refunds":{"elements":[{"id":"R","amount":1000}]}}"""
        server.enqueue(json(partly))
        server.enqueue(json(cloverResource("order.json")))
        assertFalse(refunds.refunded(sale), "a part refund isn't the sale refunded")
        server.enqueue(json(partly))
        server.enqueue(json(cloverResource("order.json").replace(""""paymentState": "PAID"""", """"paymentState": "REFUNDED"""")))
        assertTrue(refunds.refunded(sale))

        server.enqueue(json("""{"id":"PAY_MONERO_1","amount":2205,"result":"VOIDED","voided":true,"createdTime":1793177760000}"""))
        assertTrue(refunds.refunded(sale), "voided in Clover is undone too")
    }

    @Test
    fun `a merchant whose country isn't known gets the plain code`() = runBlocking {
        val sale = FoundSale("PAY_MONERO_1", 2205, "USD", null, "ORDER_CL_1", "2026-10-28T08:56:00Z", null)
        assertNull(source(at = merchant.copy(country = null)).order(sale).receipt)
        assertNull(source().order(sale.copy(orderId = null)).receipt)
        assertEquals(0, server.requestCount)
    }
}
