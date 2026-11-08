package com.monokulo.till.sales

import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import uniffi.orf_codec_ffi.receiptFromUri
import java.time.Instant
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class MemoryTokens(var tokens: PosTokens? = null) : TokenStore {
    override fun load() = tokens
    override fun save(tokens: PosTokens?) {
        this.tokens = tokens
    }
}

class MemoryCursor(var sinceValue: String? = null, var handledIds: Set<String> = emptySet()) : CursorStore {
    override fun since() = sinceValue
    override fun handled() = handledIds
    override fun save(since: String, handled: Set<String>) {
        sinceValue = since
        handledIds = handled
    }
}

fun resource(name: String): String =
    SquareTest::class.java.getResource("/square/$name")!!.readText()

/** Square's sign-in and API as the till uses them, against a stand-in Square. */
class SquareTest {
    private val server = MockWebServer()
    private val tokens = MemoryTokens()
    private val now = Instant.parse("2026-10-22T16:10:00Z")
    private lateinit var config: SquareConfig
    private lateinit var auth: SquareAuth

    @BeforeTest
    fun start() {
        server.start()
        config = SquareConfig("sq0idp-app", sandbox = false, redirectUri = "https://till.monokulo.com/square", baseUrl = server.url("/").toString().trimEnd('/'))
        auth = SquareAuth(OkHttpClient(), config, tokens) { now }
    }

    @AfterTest fun stop() = server.close()

    private fun json(body: String) = MockResponse.Builder().code(200).setHeader("Content-Type", "application/json").body(body).build()

    @Test
    fun `the environment is the application's, as a Bell names it`() {
        assertEquals("production", config.environment)
        assertEquals("sandbox", config.copy(applicationId = "sandbox-sq0idb-dev").environment)
        assertEquals("production", SquareConfig.environmentOf("sq0idp-sandbox-lookalike"))
    }

    @Test
    fun `the pkce challenge is RFC 7636's`() {
        val pkce = Pkce("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", "s")
        assertEquals("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM", pkce.challenge)
    }

    @Test
    fun `signing in asks for the till's scopes and keeps the tokens`() = runBlocking {
        val pkce = Pkce.create()
        val url = auth.authorizeUrl(pkce)
        assertTrue(url.contains("client_id=sq0idp-app"), url)
        assertTrue(url.contains("code_challenge=${pkce.challenge}"), url)
        assertTrue(url.contains("scope=PAYMENTS_READ%20ORDERS_READ%20MERCHANT_PROFILE_READ%20PAYMENTS_WRITE"), url)
        assertTrue(url.contains("redirect_uri=https%3A%2F%2Ftill.monokulo.com%2Fsquare"), url)
        assertTrue(url.contains("session=false"), url)

        // Another attempt's state is refused before Square is asked.
        assertFailsWith<SquareException> { auth.finish(pkce, "other", "code", null) }
        server.enqueue(json("""{"access_token":"EAAA1","expires_at":"2026-11-21T16:10:00Z","refresh_token":"r1","merchant_id":"M1"}"""))
        auth.finish(pkce, pkce.state, "code1", null)
        assertEquals("EAAA1", tokens.tokens?.accessToken)
        val sent = server.takeRequest().body!!.utf8()
        assertTrue(sent.contains(""""code_verifier":"${pkce.verifier}"""") && !sent.contains("secret"), sent)
    }

    @Test
    fun `the sandbox's sign-in leaves out session, which it doesn't support`() {
        val sandbox = SquareAuth(OkHttpClient(), config.copy(sandbox = true), tokens) { now }
        val url = sandbox.authorizeUrl(Pkce.create())
        assertFalse(url.contains("session="), url)
        assertTrue(url.contains("code_challenge_method=S256"), url)
    }

    @Test
    fun `a token about to expire is refreshed and the new refresh token kept`() = runBlocking {
        tokens.tokens = PosTokens("old", "2026-10-22T20:00:00Z", "r1", "M1")
        server.enqueue(json("""{"access_token":"new","expires_at":"2026-11-21T16:10:00Z","refresh_token":"r2","merchant_id":"M1"}"""))
        assertEquals("new", auth.accessToken())
        assertEquals("r2", tokens.tokens?.refreshToken)
        assertTrue(server.takeRequest().body!!.utf8().contains(""""refresh_token":"r1""""))
        // Good for a while now: no request.
        assertEquals("new", auth.accessToken())
    }

    @Test
    fun `a refused refresh means signing in again`() = runBlocking {
        tokens.tokens = PosTokens("old", "2026-10-22T20:00:00Z", "r1", "M1")
        server.enqueue(MockResponse.Builder().code(401).body("""{"errors":[{"code":"UNAUTHORIZED","detail":"Refresh token expired"}]}""").build())
        val e = assertFailsWith<SquareException> { auth.accessToken() }
        assertTrue(e.signInAgain)
        assertNull(tokens.tokens)
    }

    private fun source(cursor: MemoryCursor): SquareSource {
        tokens.tokens = PosTokens("EAAA1", "2026-11-21T16:10:00Z", "r1", "M1")
        val api = SquareApi(OkHttpClient(), config, auth)
        val location = SquareLocation("L1", "Front Counter", "The Bean Room", "US", "USD", "America/Los_Angeles")
        return SquareSource(api, config, location, cursor) { now }
    }

    @Test
    fun `only completed Monero tenders are sales, and handled ones aren't given again`() = runBlocking {
        val cursor = MemoryCursor("2026-10-22T16:05:00Z")
        val source = source(cursor)
        server.enqueue(json(resource("payments.json")))
        val sales = source.newSales()
        assertEquals(listOf("pay_monero"), sales.map { it.id })
        val sale = sales.single()
        assertEquals(1388, sale.amountMinor)
        assertEquals("bP9m", sale.reference)
        assertEquals("dev_front", sale.deviceId)
        val request = server.takeRequest()
        assertEquals("L1", request.url.queryParameter("location_id"))
        assertEquals("2026-10-22T16:03:00Z", request.url.queryParameter("begin_time"), "two minutes of overlap")
        assertEquals(SquareConfig.API_VERSION, request.headers["Square-Version"])

        source.handled(sale)
        assertEquals(setOf("pay_monero"), cursor.handledIds)
        assertEquals("2026-10-22T16:07:00.512Z", cursor.sinceValue)
        server.enqueue(json(resource("payments.json")))
        assertTrue(source.newSales().isEmpty())
    }

    @Test
    fun `a sale's receipt is made from its order, on the till, and fits in the code`() = runBlocking {
        val source = source(MemoryCursor("2026-10-22T16:05:00Z"))
        server.enqueue(json(resource("payments.json")))
        val sale = source.newSales().single()
        server.enqueue(json(resource("order.json")))
        val receipt = assertNotNull(source.order(sale).receipt)
        assertEquals("/v2/orders/ord_sq_1", server.takeRequest().let { server.takeRequest() }.url.encodedPath)
        assertEquals("bP9m", receipt.receipt.receiptId)
        assertEquals("2026-10-22T09:06:41-07:00", receipt.receipt.issueTimestamp)
        assertEquals("The Bean Room", receipt.merchant.legalName)
        assertEquals("Front Counter", receipt.merchant.displayName)
        assertEquals(
            listOf("Flat white, Oat milk", "Almond croissant warmed (Large)"),
            receipt.lineItems.map { it.description },
        )
        val flatWhite = receipt.lineItems[0]
        assertEquals(4.5, flatWhite.unitPrice, "the price that makes 2 × it the line's total")
        assertEquals(9.0, flatWhite.lineTotal)
        assertEquals(0.08875, receipt.tax!!.taxLines.single().rate)

        val code = com.monokulo.till.receipt.ReceiptCode.of("monero:8Bx?tx_amount=0.1", receipt)
        assertTrue(code.hasReceipt, code.why ?: "")
        assertTrue(receiptFromUri(code.uri)!!.contains("Almond croissant warmed (Large)"))
    }

    @Test
    fun `a sale's order names whom it's for when a pickup names them, from the one read`() = runBlocking {
        val source = source(MemoryCursor("2026-10-22T16:05:00Z"))
        server.enqueue(json(resource("payments.json")))
        val sale = source.newSales().single()
        server.enqueue(json(resource("order-pickup.json")))
        val read = source.order(sale)
        assertEquals("Ana López", read.customerName, "on one line, trimmed")
        assertNotNull(read.receipt)
        assertFalse(com.monokulo.till.receipt.ReceiptCode.of("monero:8Bx?tx_amount=0.1", read.receipt!!).uri.contains("Ana"), "the name is never in the code")
        assertEquals(2, server.requestCount, "the payments and the order: no Customers lookup")
        server.enqueue(json(resource("order.json")))
        assertNull(source.order(sale).customerName, "an order with no fulfillment names no one")
    }

    @Test
    fun `a recipient's name is read defensively, and a strange shape never costs the receipt`() {
        val codec = kotlinx.serialization.json.Json { ignoreUnknownKeys = true }
        fun name(fulfillments: String) = codec.decodeFromString(
            SquareOrder.serializer(),
            """{"id":"o","location_id":"L1","created_at":"2026-10-22T16:06:41Z","total_money":{"amount":1,"currency":"USD"},"fulfillments":$fulfillments}""",
        ).customerName
        assertEquals("Sam", name("""[{"delivery_details":{"recipient":{"display_name":"Sam"}}}]"""))
        assertEquals("Kai", name("""[{"shipment_details":{"recipient":{"display_name":"Kai"}}}]"""))
        assertEquals("Lee", name("""[{"pickup_details":{"recipient":{"display_name":" "}}},{"pickup_details":{"recipient":{"display_name":"Lee"}}}]"""), "a blank name is skipped")
        assertEquals("x".repeat(80), name("""[{"pickup_details":{"recipient":{"display_name":"${"x".repeat(500)}"}}}]"""))
        assertNull(name("""[{"pickup_details":{"recipient":{"display_name":42}}}]"""), "a number isn't a name")
        assertNull(name("""[{"pickup_details":{"recipient":"Ana"}}]"""))
        assertNull(name("""["Ana", null, 3]"""))
        assertNull(name("""{"pickup_details":{}}"""))
        assertNull(name("null"))
    }

    @Test
    fun `a refund is the whole sale, once`() = runBlocking {
        val source = source(MemoryCursor("2026-10-22T16:05:00Z"))
        server.enqueue(json("""{"refund":{"id":"r"}}"""))
        source.refunds.refund(FoundSale("pay_monero", 1388, "USD", "bP9m", "ord_sq_1", "2026-10-22T16:07:00Z", null))
        val body = server.takeRequest().body!!.utf8()
        assertTrue(body.contains(""""payment_id":"pay_monero"""") && body.contains(""""amount":1388"""), body)
        assertTrue(body.contains(""""idempotency_key":"till-refund-pay_monero""""), body)
    }

    @Test
    fun `amounts are decimals in the currency's own digits`() {
        assertEquals("24.50", decimalAmount(2450, "USD"))
        assertEquals("1200", decimalAmount(1200, "JPY"))
        assertEquals("0.05", decimalAmount(5, "EUR"))
    }
}
