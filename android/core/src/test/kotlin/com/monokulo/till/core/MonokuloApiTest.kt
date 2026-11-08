package com.monokulo.till.core

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.OkHttpClient
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The client against a stand-in Monokulo that answers as Monokulo's API
 * does: HAL-FORMS with relative links, problem+json errors. The answers
 * themselves are checked against Monokulo's own OpenAPI document in
 * [MonokuloContractTest].
 */
class MonokuloApiTest {
    private val server = MockWebServer()
    private val identity = AppIdentity("till", "Monokulo Till", "0.1.0", "android", "Pixel 8")
    /** The clock the entry is kept by. */
    private var clock = 1_000_000L
    private val api = MonokuloApi(OkHttpClient(), identity) { clock }
    private val token = "ct_" + "ab".repeat(32)
    private val address = "4" + "A".repeat(85) + "Q2xyz789"

    /** What the stand-in answers, by method and path; anything else is a 404 problem. */
    private val routes = mutableMapOf<String, () -> MockResponse>()
    private val seen = CopyOnWriteArrayList<RecordedRequest>()

    @BeforeTest
    fun start() {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                seen += request
                return routes["${request.method} ${request.url.encodedPath}"]?.invoke()
                    ?: problem(404, "Not found", "No such thing.")
            }
        }
        server.start()
        routes["GET /api/v1"] = { entry(ENTRY) }
    }

    @AfterTest fun stop() = server.close()

    /** The server by name (`localhost`), as a till that connected to that name reaches it. */
    private fun base() = "http://localhost:${server.port}"

    private fun connection() = Connection(appKey = "ak_secret", api = "${base()}/api/v1")

    private fun hal(body: String, code: Int = 200) = MockResponse.Builder().code(code)
        .setHeader("Content-Type", "application/prs.hal-forms+json").body(body).build()

    /** The entry as Monokulo answers it: kept per key for an hour, with its version. */
    private fun entry(body: String, etag: String = "\"e1\"") = hal(body).newBuilder()
        .setHeader("Cache-Control", "private, max-age=3600").setHeader("Vary", "Authorization").setHeader("ETag", etag).build()

    private fun problem(status: Int, title: String, detail: String, type: String = "x", extra: String = "") =
        MockResponse.Builder().code(status)
            .setHeader("Content-Type", "application/problem+json")
            .body("""{"type":"https://docs.monokulo.com/problems/$type","title":"$title","status":$status,"detail":"$detail"$extra}""")
            .build()

    private fun RecordedRequest.line() = "$method ${url.encodedPath}"

    private fun order(status: String = "pending") = """{"order_id":"order_1","status":"$status","is_final":false,
        "address":"8Bx","xmr_amount_piconero":500000000000,"amount_received_piconero":0,"amount":"24.50",
        "currency":"USD","merchant_order_id":"#1042","show_merchant_order_id":true,"confirmations":0,
        "confirmations_required":10,"created_at":1,"expires_at":99,
        "_links":{"self":{"href":"/api/v1/orders/order_1"},"mk:order-events":{"href":"/api/v1/orders/order_1/events"}}}"""

    @Test
    fun `connecting redeems the code at the link for a key and the API's entry at the address it used`() = runBlocking {
        routes["POST /apps/connect"] = { MockResponse.Builder().code(200).body("""{"app_key":"ak_x","api":"/api/v1"}""").build() }
        val connection = api.connect(ConnectCode("${base()}/apps/connect", token))
        assertEquals(Connection("ak_x", "${base()}/api/v1"), connection)
        assertNull(connection.onionApi, "a Monokulo with no onion address gives none")
        val request = seen.single()
        assertEquals("till/0.1.0", request.headers["Monokulo-Client"])
        val sent = request.body!!.utf8()
        assertTrue(sent.contains(""""token":"$token"""") && sent.contains(""""device_name":"Pixel 8""""), sent)
    }

    @Test
    fun `a Monokulo with an onion address gives its entry there too`() = runBlocking {
        val onion = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1"
        routes["POST /apps/connect"] = {
            MockResponse.Builder().code(200).body("""{"app_key":"ak_x","api":"/api/v1","onion_api":"$onion"}""").build()
        }
        val connection = api.connect(ConnectCode("${base()}/apps/connect", token))
        assertEquals(onion, connection.onionApi)
        assertEquals("${base()}/api/v1", connection.entry, "the till goes over Tor only when it chooses to")
        assertEquals(onion, connection.copy(tor = true).entry)
    }

    @Test
    fun `a Monokulo under a path is connected and used under it`() = runBlocking {
        routes["POST /monokulo/apps/connect"] = {
            MockResponse.Builder().code(200).body("""{"app_key":"ak_x","api":"/monokulo/api/v1"}""").build()
        }
        routes["GET /monokulo/api/v1"] = { entry(ENTRY.replace("\"/api/v1", "\"/monokulo/api/v1")) }
        routes["GET /monokulo/api/v1/store"] = { hal("""{"name":"Bakery","base_currency":"EUR"}""") }
        routes["POST /monokulo/api/v1/orders"] = { hal(order().replace("\"/api/v1", "\"/monokulo/api/v1"), 201) }
        val connection = api.connect(ConnectCode.parse("${base()}/monokulo/apps/connect#token=$token", allowCleartext = true)!!)
        assertEquals("${base()}/monokulo/api/v1", connection.api)
        api.store(connection)
        val order = api.createOrder(connection, "24.50", "USD", null, "sq:abc")
        assertEquals("${base()}/monokulo/api/v1/orders/order_1", order.url)
        assertEquals(
            listOf("POST /monokulo/apps/connect", "GET /monokulo/api/v1", "GET /monokulo/api/v1/store", "POST /monokulo/api/v1/orders"),
            seen.map { it.line() },
        )
    }

    @Test
    fun `a refused code says why, by its problem`() = runBlocking {
        routes["POST /apps/connect"] = {
            problem(503, "No public address", "This Monokulo has no public address yet. Ask its admin to set one.", "no-public-address")
        }
        val e = assertFailsWith<MonokuloException> { api.connect(ConnectCode("${base()}/apps/connect", token)) }
        assertEquals(503, e.status)
        assertEquals("no-public-address", e.problem)
        assertEquals("This Monokulo has no public address yet. Ask its admin to set one.", e.message)
    }

    @Test
    fun `a busy Monokulo says how long to wait`() = runBlocking {
        routes["POST /apps/connect"] = {
            problem(429, "Too many requests", "Slow down.", "rate-limited", ""","retry_after":30""").newBuilder()
                .setHeader("Retry-After", "30").build()
        }
        val e = assertFailsWith<MonokuloException> { api.connect(ConnectCode("${base()}/apps/connect", token)) }
        assertEquals("rate-limited", e.problem)
        assertEquals(30L, e.retryAfter)
    }

    @Test
    fun `a challenge is solved and the request sent again with its proof`() = runBlocking {
        val challenge = "c0ffee.abc"
        routes["POST /apps/connect"] = {
            val proof = seen.last().headers["Monokulo-Proof"]
            if (proof == null) {
                problem(429, "Prove it", "Solve the challenge.", "challenge-required", ""","challenge":{"challenge":"$challenge","difficulty":8}""")
            } else {
                val nonce = proof.removePrefix("$challenge.")
                val hash = java.security.MessageDigest.getInstance("SHA-256").digest("$challenge$nonce".toByteArray())
                assertTrue(Proof.zeroBits(hash) >= 8, proof)
                MockResponse.Builder().code(200).body("""{"app_key":"ak_x","api":"/api/v1"}""").build()
            }
        }
        assertEquals("ak_x", api.connect(ConnectCode("${base()}/apps/connect", token)).appKey)
        assertEquals(2, seen.size)
        assertEquals(seen[0].body!!.utf8(), seen[1].body!!.utf8(), "the same request again")
    }

    @Test
    fun `the store and its wallet are read live, found from the entry`() = runBlocking {
        routes["GET /api/v1/store"] = {
            hal("""{"name":"Bakery","base_currency":"EUR","wallet":{"name":"Shop","network":"stagenet","address":"$address"},"_links":{}}""")
        }
        val store = api.store(connection())
        assertEquals("Bakery", store.name)
        assertEquals("EUR", store.baseCurrency)
        assertEquals(address, store.wallet?.address)
        assertEquals("4AAAAAAA…Q2xyz789", store.wallet?.addressShort)
        assertEquals(listOf("GET /api/v1", "GET /api/v1/store"), seen.map { "${it.method} ${it.url.encodedPath}" })
        seen.forEach {
            assertEquals("Bearer ak_secret", it.headers["Authorization"])
            assertEquals("localhost:${server.port}", it.headers["Host"], "links resolve against the address the till used")
        }
    }

    @Test
    fun `a store without a wallet says so, by leaving it out`() = runBlocking {
        routes["GET /api/v1/store"] = { hal("""{"name":"Bakery","base_currency":"EUR"}""") }
        assertNull(api.store(connection()).wallet)
    }

    @Test
    fun `the entry is kept for its max-age, then revalidated with its ETag`() = runBlocking {
        routes["GET /api/v1/store"] = { hal("""{"name":"Bakery","base_currency":"EUR"}""") }
        routes["GET /api/v1/app"] = { hal("""{"app_id":"app_1","device_name":"Front counter","platform":"android","connected_at":5}""") }
        api.store(connection())
        assertEquals("Front counter", api.app(connection()).deviceName)
        assertEquals(1, seen.count { it.url.encodedPath == "/api/v1" }, "kept: one read for both")
        clock += 3_600_000
        routes["GET /api/v1"] = {
            if (seen.last().headers["If-None-Match"] == "\"e1\"") {
                MockResponse.Builder().code(304).setHeader("Cache-Control", "private, max-age=3600").setHeader("ETag", "\"e1\"").build()
            } else {
                entry(ENTRY)
            }
        }
        api.store(connection())
        api.store(connection())
        val entries = seen.filter { it.url.encodedPath == "/api/v1" }
        assertEquals(2, entries.size, "asked again once it expired, then kept another hour")
        assertEquals("\"e1\"", entries.last().headers["If-None-Match"])
    }

    @Test
    fun `each key keeps its own entry`() = runBlocking {
        routes["GET /api/v1/store"] = { hal("""{"name":"Bakery","base_currency":"EUR"}""") }
        api.store(connection())
        api.store(connection().copy(appKey = "ak_other"))
        assertEquals(listOf("Bearer ak_secret", "Bearer ak_other"), seen.filter { it.url.encodedPath == "/api/v1" }.map { it.headers["Authorization"] })
    }

    @Test
    fun `a link that answers 404 has the entry read again, and followed anew`() = runBlocking {
        routes["GET /api/v1/store"] = { problem(404, "Not found", "No such thing.", "not-found") }
        routes["GET /api/v1/shop"] = { hal("""{"name":"Bakery","base_currency":"EUR"}""") }
        api.onionApi(connection())
        routes["GET /api/v1"] = { entry(ENTRY.replace("/api/v1/store", "/api/v1/shop"), "\"e2\"") }
        assertEquals("Bakery", api.store(connection()).name)
        assertEquals(listOf("GET /api/v1", "GET /api/v1/store", "GET /api/v1", "GET /api/v1/shop"), seen.map { it.line() })
    }

    @Test
    fun `orders are made with the create-order form, with no description`() = runBlocking {
        routes["POST /api/v1/orders"] = { hal(order(), 201).newBuilder().setHeader("Location", "/api/v1/orders/order_1").build() }
        val order = api.createOrder(connection(), "24.50", "USD", "#1042", "sq:abc")
        assertEquals("monero:8Bx?tx_amount=0.5", order.paymentUri)
        assertEquals("${base()}/api/v1/orders/order_1", order.url)
        assertEquals(SalePhase.Waiting, order.phase)
        val request = seen.last()
        assertEquals("POST", request.method)
        val sent = request.body!!.utf8()
        assertTrue(sent.contains(""""idempotency_key":"sq:abc"""") && sent.contains(""""currency":"USD""""), sent)
        assertFalse(sent.contains("description"), sent)
    }

    @Test
    fun `an order is read again at its link`() = runBlocking {
        routes["GET /api/v1/orders/order_1"] = { hal(order("confirming")) }
        assertEquals(SalePhase.Confirming, api.order(connection(), "${base()}/api/v1/orders/order_1").phase)
        assertEquals(listOf("GET /api/v1/orders/order_1"), seen.map { it.line() }, "no entry needed")
    }

    @Test
    fun `an order Monokulo has no price, reference or end for leaves them out, and reads as none`() = runBlocking {
        routes["GET /api/v1/orders/order_2"] = {
            hal("""{"order_id":"order_2","status":"pending","is_final":false,"address":"8Bx",
                "xmr_amount_piconero":500000000000,"amount_received_piconero":0,"show_merchant_order_id":false,
                "confirmations":0,"confirmations_required":10,"created_at":1,"expires_at":99,
                "_links":{"self":{"href":"/api/v1/orders/order_2"}}}""")
        }
        val order = api.order(connection(), "${base()}/api/v1/orders/order_2")
        assertEquals("order_2", order.orderId)
        assertNull(order.amount)
        assertNull(order.currency)
        assertNull(order.merchantOrderId)
        assertEquals(SalePhase.Waiting, order.phase)
        assertEquals("monero:8Bx?tx_amount=0.5", order.paymentUri)
    }

    @Test
    fun `disconnecting uses the disconnect form`() = runBlocking {
        routes["DELETE /api/v1/app"] = { MockResponse.Builder().code(204).build() }
        api.disconnect(connection())
        assertEquals("DELETE /api/v1/app", seen.last().let { "${it.method} ${it.url.encodedPath}" })
    }

    @Test
    fun `an unknown key hears 401 at the entry, with the problem's detail`() = runBlocking {
        routes["GET /api/v1"] = { problem(401, "No valid key", "This app was disconnected.", "unauthorized") }
        val e = assertFailsWith<MonokuloException> { api.store(connection()) }
        assertEquals(401, e.status)
        assertEquals("This app was disconnected.", e.message)
    }

    @Test
    fun `a form the key may not use isn't tried`() = runBlocking {
        routes["GET /api/v1"] = { hal("""{"_links":{"self":{"href":"/api/v1"}}}""") }
        val e = assertFailsWith<MonokuloException> { api.createOrder(connection(), "1", "USD", null, "k") }
        assertEquals(403, e.status)
        assertEquals(1, seen.size, "only the entry was read")
    }

    @Test
    fun `the event stream gives ready, then sales, skipping what it doesn't know`() = runBlocking {
        val sale = """{"event":"sale.waiting","order_id":"order_1","amount":"1","currency":"XMR","address":"8Bx",
            "xmr_amount_piconero":1000000000000,"expires_at":9,"device_name":"Till","at":1,
            "_links":{"mk:order":{"href":"/api/v1/orders/order_1"}}}""".replace("\n", "")
        routes["GET /api/v1/events"] = {
            MockResponse.Builder().code(200).setHeader("Content-Type", "text/event-stream").body(
                "event: ready\ndata: {\"at\":1}\n\n" +
                    ": keep-alive\n\n" +
                    "event: something.new\ndata: {}\n\n" +
                    "event: sale.waiting\ndata: $sale\n\n" +
                    "event: lagged\ndata: {\"missed\":3}\n\n",
            ).build()
        }
        val events = api.events(connection()).take(3).toList()
        assertEquals(StreamEvent.Ready, events[0])
        assertIs<StreamEvent.Sale>(events[1])
        assertEquals("order_1", (events[1] as StreamEvent.Sale).event.orderId)
        assertEquals("${base()}/api/v1/orders/order_1", (events[1] as StreamEvent.Sale).event.orderUrl, "its mk:order, resolved")
        assertEquals(StreamEvent.Lagged, events[2])
        val stream = seen.last()
        assertEquals("/api/v1/events", stream.url.encodedPath)
        assertEquals("Bearer ak_secret", stream.headers["Authorization"])
    }

    @Test
    fun `a stream refused with 401 says so`() = runBlocking {
        routes["GET /api/v1/events"] = { problem(401, "No valid key", "This app was disconnected.") }
        val e = assertFailsWith<MonokuloException> { api.events(connection()).toList() }
        assertEquals(401, e.status)
    }

    @Test
    fun `over Tor the till reads the onion entry and follows it there, never the web address`() = runBlocking {
        val onion = MockWebServer()
        onion.start()
        try {
            val there = CopyOnWriteArrayList<String>()
            onion.dispatcher = object : Dispatcher() {
                override fun dispatch(request: RecordedRequest): MockResponse {
                    there += request.line()
                    return when (request.line()) {
                        "GET /api/v1" -> entry(ENTRY)
                        "GET /api/v1/store" -> hal("""{"name":"Bakery","base_currency":"EUR"}""")
                        else -> problem(404, "Not found", "No such thing.")
                    }
                }
            }
            routes["GET /api/v1/store"] = { hal("""{"name":"Bakery","base_currency":"EUR"}""") }
            val connection = connection().copy(onionApi = onion.url("/api/v1").toString(), tor = true)
            assertEquals("Bakery", api.store(connection).name)
            assertEquals(listOf("GET /api/v1", "GET /api/v1/store"), there)
            assertTrue(seen.isEmpty(), "nothing went to the web address")
            api.store(connection.copy(tor = false))
            assertEquals(listOf("GET /api/v1", "GET /api/v1/store"), seen.map { it.line() }, "off Tor, the web address")
        } finally {
            onion.close()
        }
    }

    @Test
    fun `the entry says where the onion entry is, while there is one`() = runBlocking {
        assertNull(api.onionApi(connection()))
        val onion = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1"
        routes["GET /api/v1"] = { entry(ENTRY.replace(""""self":{"href":"/api/v1"}""", """"self":{"href":"/api/v1"},"mk:onion-api":{"href":"$onion"}""")) }
        assertEquals(onion, api.onionApi(connection().copy(appKey = "ak_2")))
    }

    companion object {
        /** The entry as Monokulo answers an app's key: relative links, and its forms. */
        val ENTRY = """{
            "_links":{"self":{"href":"/api/v1"},"mk:store":{"href":"/api/v1/store"},"mk:orders":{"href":"/api/v1/orders"},
              "mk:events":{"href":"/api/v1/events","type":"text/event-stream"},"mk:app":{"href":"/api/v1/app"},
              "curies":[{"name":"mk","href":"https://docs.monokulo.com/rels/{rel}","templated":true}]},
            "_templates":{"create-order":{"method":"POST","target":"/api/v1/orders","contentType":"application/json","properties":[]},
              "disconnect":{"method":"DELETE","target":"/api/v1/app","properties":[]}}}"""
    }
}
