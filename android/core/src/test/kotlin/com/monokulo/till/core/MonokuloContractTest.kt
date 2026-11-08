package com.monokulo.till.core

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import kotlin.test.fail

/**
 * The client against Monokulo's own OpenAPI document, generated from the
 * pinned Monokulo commit (`monokulo.sha` in `gradle.properties`) by the
 * `monokuloOpenApi` task, never committed. A stand-in answers each request
 * with what Monokulo recorded for that route from a real exchange (the
 * entry and each resource as an app's key sees them, the app handshake
 * and its problems, the event stream), so a renamed route, link, form,
 * field or event fails here.
 */
class MonokuloContractTest {
    private val openapi: JsonObject = run {
        val path = System.getProperty("monokulo.openapi")
            ?: fail("monokulo.openapi isn't set: run the tests with Gradle, which generates Monokulo's OpenAPI document")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }
    private val paths = openapi["paths"]!!.jsonObject
    private val schemas = openapi["components"]!!.jsonObject["schemas"]!!.jsonObject
    private val server = MockWebServer()
    private val api = MonokuloApi(OkHttpClient(), AppIdentity("till", "Monokulo Till", "0.1.0", "android", "Pixel 8"))
    private val seen = CopyOnWriteArrayList<String>()

    /** Which recorded problem each `METHOD path` answers with, by name; any other answers its success. */
    private val problems = java.util.concurrent.ConcurrentHashMap<String, String>()

    /** Whether the Monokulo answering has an onion address: picks the `-with-onion` examples. */
    @Volatile private var onion = false

    /** What went wrong answering, rethrown after the test so it isn't lost as a 500. */
    @Volatile private var failure: Throwable? = null

    @BeforeTest
    fun start() {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = try {
                answer(request)
            } catch (e: Throwable) {
                failure = e
                MockResponse.Builder().code(500).build()
            }
        }
        server.start()
    }

    @AfterTest
    fun stop() {
        server.close()
        failure?.let { throw it }
    }

    private fun connection() = Connection("ak_secret", server.url("/api/v1").toString())

    /** The OpenAPI path template [path] matches, or null. */
    private fun template(path: String): String? = paths.keys.firstOrNull { template ->
        Regex("^" + template.replace(Regex("\\{[^}]+}"), "[^/]+") + "$").matches(path)
    }

    private fun operation(method: String, path: String): JsonObject? =
        template(path)?.let { paths[it]?.jsonObject?.get(method.lowercase()) as? JsonObject }

    /**
     * A response's content type and its recorded example, as [key] (`ak`, or
     * `no-key` for the handshake) sees it: `ak` from a Monokulo without an
     * onion address, `ak-with-onion` from one with ([onion]; a route whose
     * answer doesn't change with it has only the first).
     */
    private fun recorded(response: JsonObject, key: String): Pair<String, JsonElement?>? {
        val (type, content) = (response["content"] as? JsonObject)?.entries?.firstOrNull() ?: return null
        val media = content.jsonObject
        val example = media["example"] ?: (media["examples"] as? JsonObject)?.let { examples ->
            val named = (if (onion) examples["$key-with-onion"] else null) ?: examples[key]
            assertNotNull(named, "Monokulo recorded no $key example; it has ${examples.keys}").jsonObject["value"]
        }
        return type to example
    }

    private fun success(op: JsonObject): Pair<Int, JsonObject> {
        val (code, response) = op["responses"]!!.jsonObject.entries.first { it.key.startsWith("2") }
        return code.toInt() to response.jsonObject
    }

    /**
     * The problem [name] as Monokulo recorded it for [op]: its status, the
     * headers that response lists (`Retry-After` as the example's
     * `retry_after`) and the example body.
     */
    private fun recordedProblem(op: JsonObject, name: String): MockResponse {
        for ((code, response) in op["responses"]!!.jsonObject) {
            val examples = (response.jsonObject["content"] as? JsonObject)?.get("application/problem+json")
                ?.jsonObject?.get("examples") as? JsonObject ?: continue
            val body = examples[name]?.jsonObject?.get("value")?.jsonObject ?: continue
            val headers = (response.jsonObject["headers"] as? JsonObject)?.keys.orEmpty()
            return MockResponse.Builder().code(code.toInt()).apply {
                setHeader("Content-Type", "application/problem+json")
                if ("Retry-After" in headers) body["retry_after"]?.let { setHeader("Retry-After", it.jsonPrimitive.content) }
                body(body.toString())
            }.build()
        }
        fail("Monokulo recorded no $name problem for this route")
    }

    private fun answer(request: RecordedRequest): MockResponse {
        val path = request.url.encodedPath
        seen += "${request.method} $path"
        val op = operation(request.method, path) ?: return MockResponse.Builder().code(404).build()
        if (request.method == "POST") checkSent(op, request.body!!.utf8())
        problems["${request.method} $path"]?.let { name -> return recordedProblem(op, name) }
        val (code, response) = success(op)
        val headers = (response["headers"] as? JsonObject)?.keys.orEmpty()
        if ("ETag" in headers && request.headers["If-None-Match"] == ETAG) {
            assertNotNull(op["responses"]!!.jsonObject["304"], "Monokulo answers 304 here")
            return MockResponse.Builder().code(304).setHeader("ETag", ETAG).setHeader("Cache-Control", "private, max-age=3600").build()
        }
        val (type, example) = recorded(response, if (path == "/apps/connect") "no-key" else "ak") ?: return MockResponse.Builder().code(code).build()
        return MockResponse.Builder().code(code).apply {
            if ("ETag" in headers) setHeader("ETag", ETAG)
            if ("Cache-Control" in headers) setHeader("Cache-Control", "private, max-age=3600")
            setHeader("Content-Type", type)
            when (example) {
                null -> Unit
                is JsonPrimitive -> body(example.content)
                else -> body(example.toString())
            }
        }.build()
    }

    /** What the app sends is what the operation takes: every required field, nothing it doesn't know. */
    private fun checkSent(op: JsonObject, sent: String) {
        val ref = op["requestBody"]!!.jsonObject["content"]!!.jsonObject["application/json"]!!
            .jsonObject["schema"]!!.jsonObject["\$ref"]!!.jsonPrimitive.content
        val schema = schemas[ref.substringAfterLast('/')]!!.jsonObject
        val known = schema["properties"]!!.jsonObject.keys
        val required = (schema["required"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty()
        val fields = Json.parseToJsonElement(sent).jsonObject.keys
        assertTrue(known.containsAll(fields), "sent $fields; Monokulo takes $known")
        assertTrue(fields.containsAll(required), "sent $fields; Monokulo needs $required")
    }

    private fun code() = ConnectCode(server.url("/apps/connect").toString(), "ct_" + "0".repeat(63) + "1")

    @Test
    fun `connecting sends what the handshake takes and keeps the key and the entry`() = runBlocking {
        val connection = api.connect(code())
        assertTrue(connection.appKey.startsWith("ak_"))
        assertEquals(server.url("/api/v1").toString(), connection.api)
        assertEquals(null, connection.onionApi, "a Monokulo without an onion address names none")
        assertEquals(listOf("POST /apps/connect"), seen)
    }

    @Test
    fun `connecting to a Monokulo with an onion address keeps its onion entry too`() = runBlocking {
        onion = true
        val connection = api.connect(code())
        assertTrue(connection.appKey.startsWith("ak_"))
        assertEquals(server.url("/api/v1").toString(), connection.api)
        val onion = assertNotNull(connection.onionApi, "Monokulo recorded the handshake with an onion address")
        assertTrue(Tor.isOnion(onion), onion)
        assertEquals(listOf("POST /apps/connect"), seen)
    }

    @Test
    fun `the handshake's problems come back by name, with their detail`() = runBlocking {
        for ((name, status) in listOf("invalid-connect-code" to 401, "no-public-address" to 503, "invalid-request" to 400)) {
            problems["POST /apps/connect"] = name
            val e = assertFailsWith<MonokuloException> { api.connect(code()) }
            assertEquals(status, e.status)
            assertEquals(name, e.problem)
            assertTrue(e.message!!.isNotBlank())
        }
    }

    @Test
    fun `the store, its wallet and the app read as Monokulo writes them, from one entry`() = runBlocking {
        val store = api.store(connection())
        assertTrue(store.name.isNotEmpty() && store.baseCurrency.isNotEmpty())
        val app = api.app(connection())
        assertTrue(app.deviceName.isNotEmpty() && app.appId.isNotEmpty())
        assertEquals(listOf("GET /api/v1", "GET /api/v1/store", "GET /api/v1/app"), seen)
    }

    @Test
    fun `the entry an app's key reads links the onion entry only when there is one`() = runBlocking {
        assertEquals(null, api.onionApi(connection()))
        onion = true
        val api = MonokuloApi(OkHttpClient(), AppIdentity("till", "Monokulo Till", "0.1.0", "android", "Pixel 8"))
        val linked = assertNotNull(api.onionApi(connection()))
        assertTrue(Tor.isOnion(linked), linked)
    }

    @Test
    fun `an order is made with the create-order form, read as Monokulo writes it, and read again at its self link`() = runBlocking {
        val order = api.createOrder(connection(), "24.50", "USD", "#1042", "sq:abc")
        assertTrue(order.address.isNotEmpty() && order.piconero > 0 && order.expiresAt > 0)
        assertTrue(order.paymentUri.startsWith("monero:${order.address}?tx_amount="))
        assertNotNull(template(order.url.toHttpUrl().encodedPath), "its self link is a route")
        assertEquals(order.orderId, api.order(connection(), order.url).orderId, "the order made is the one read")
    }

    @Test
    fun `an order says how many confirmations its payment has of the store's threshold`() = runBlocking {
        val made = api.createOrder(connection(), "24.50", "USD", "#1042", "sq:abc")
        val read = api.order(connection(), made.url)
        for (order in listOf(made, read)) {
            assertEquals(0, order.confirmations, "nothing paid yet")
            assertTrue((order.confirmationsRequired ?: 0) > 0, "the threshold the ring and \"4 of 10\" count to: ${order.confirmationsRequired}")
        }
    }

    @Test
    fun `making an order hears its recorded problems by name, with their detail`() = runBlocking {
        for ((name, status) in listOf("rates-unavailable" to 503, "engine-unavailable" to 503, "idempotency-conflict" to 409)) {
            problems["POST /api/v1/orders"] = name
            val e = assertFailsWith<MonokuloException> { api.createOrder(connection(), "24.50", "USD", "#1042", "sq:abc") }
            assertEquals(status, e.status, name)
            assertEquals(name, e.problem)
            assertTrue(e.message!!.isNotBlank(), name)
        }
    }

    /**
     * Each route the app calls, as the stand-in sees it, and the call that
     * reaches it through a client of its own (so the entry isn't kept).
     */
    private val routes: List<Triple<String, String, suspend (MonokuloApi) -> Unit>> = listOf(
        Triple("POST", "/apps/connect") { it.connect(code()) },
        Triple("GET", "/api/v1") { it.onionApi(connection()) },
        Triple("GET", "/api/v1/store") { it.store(connection()) },
        Triple("GET", "/api/v1/app") { it.app(connection()) },
        Triple("DELETE", "/api/v1/app") { it.disconnect(connection()) },
        Triple("POST", "/api/v1/orders") { it.createOrder(connection(), "24.50", "USD", "#1042", "sq:abc") },
        Triple("GET", "/api/v1/orders/ord_1") { it.order(connection(), server.url("/api/v1/orders/ord_1").toString()) },
        Triple("GET", "/api/v1/events") { it.events(connection()).take(1).toList() },
    )

    @Test
    fun `every problem Monokulo lists for a route the app calls comes back by name, and the app has words for it`() = runBlocking {
        var heard = 0
        for ((method, path, call) in routes) {
            val op = assertNotNull(operation(method, path), "Monokulo has no $method $path")
            for ((code, response) in op["responses"]!!.jsonObject) {
                val examples = (response.jsonObject["content"] as? JsonObject)?.get("application/problem+json")
                    ?.jsonObject?.get("examples") as? JsonObject ?: continue
                for ((name, example) in examples) {
                    problems.clear()
                    problems["$method $path"] = name
                    val e = assertFailsWith<MonokuloException>("$method $path $name") {
                        call(MonokuloApi(OkHttpClient(), AppIdentity("till", "Monokulo Till", "0.1.0", "android", "Pixel 8")))
                    }
                    assertEquals(code.toInt(), e.status, "$method $path $name")
                    assertEquals(name, e.problem, "$method $path")
                    assertTrue(Problems.knows(name), "the app has no words for $name, which $method $path can answer")
                    assertTrue(Problems.say(e).isNotBlank(), name)
                    example.jsonObject["value"]!!.jsonObject["retry_after"]?.let {
                        assertEquals(it.jsonPrimitive.content.toLong(), e.retryAfter, "$method $path $name waits as long as it says")
                    }
                    heard++
                }
            }
        }
        assertTrue(heard > 30, "heard only $heard problems")
    }

    @Test
    fun `every sale event Monokulo sends has its phase, and links its order`() = runBlocking {
        val stream = operation("GET", "/api/v1/events")!!["responses"]!!.jsonObject["200"]!!
            .jsonObject["content"]!!.jsonObject["text/event-stream"]!!.jsonObject
        val names = stream["x-events"]!!.jsonObject.keys
        assertEquals(setOf("ready", "lagged"), names.filter { SalePhase.ofEvent(it) == null }.toSet(), "every other event is a sale's")
        val events = api.events(connection()).take(2).toList()
        assertEquals(StreamEvent.Ready, events[0])
        val sale = assertIs<StreamEvent.Sale>(events[1]).event
        val url = assertNotNull(sale.orderUrl, "a sale event links its order")
        assertNotNull(template(url.toHttpUrl().encodedPath), "its mk:order link is a route")
        val order = api.order(connection(), url)
        assertEquals(sale.orderId, order.orderId, "the event's order is the one it links")
        assertEquals(api.createOrder(connection(), "24.50", "USD", "#1042", "sq:abc").orderId, order.orderId, "and the one an app made")
    }

    @Test
    fun `disconnecting is the entry's disconnect form`() = runBlocking {
        api.disconnect(connection())
        assertEquals("DELETE /api/v1/app", seen.last())
    }

    private companion object {
        const val ETAG = "\"entry-1\""
    }
}
