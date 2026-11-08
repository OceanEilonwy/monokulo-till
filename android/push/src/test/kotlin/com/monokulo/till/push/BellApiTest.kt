package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.OkHttpClient
import java.io.IOException
import java.io.InterruptedIOException
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.time.Duration.Companion.milliseconds

/** The Bell's client against a stand-in Bell: exactly the three requests, and the point of sale's token in one of them only (A9.1, A9.2, T5.1). */
class BellApiTest {
    private val server = MockWebServer()
    private val seen = CopyOnWriteArrayList<RecordedRequest>()
    private val routes = mutableMapOf<String, () -> MockResponse>()
    private val api = BellApi(OkHttpClient())
    private lateinit var bell: BellAddress

    private val subscription = PushSubscription("https://ntfy.sh/upAbc?up=1", "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM", "tBHItJI5svbpez7KI4CCXg")

    @BeforeTest
    fun start() {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                seen += request
                return routes["${request.method} ${request.url.encodedPath}"]?.invoke() ?: MockResponse.Builder().code(404).build()
            }
        }
        server.start()
        bell = BellAddress.parse(server.url("/bell").toString(), allowHttp = true)!!
    }

    @AfterTest fun stop() = server.close()

    private fun json(code: Int, body: String) = MockResponse.Builder().code(code).setHeader("Content-Type", "application/json").body(body).build()

    private fun problem(code: Int, slug: String, detail: String, retryAfter: Long? = null) = MockResponse.Builder().code(code)
        .setHeader("Content-Type", "application/problem+json")
        .apply { if (retryAfter != null) setHeader("Retry-After", retryAfter.toString()) }
        .body(
            """{"type":"https://github.com/OceanEilonwy/monokular/blob/main/docs/bell.md#$slug","title":"t","status":$code,"detail":"$detail"""" +
                (retryAfter?.let { ""","retry_after":$it""" } ?: "") + "}",
        )
        .build()

    /** A P-256 point, as a Bell's VAPID key is. */
    private val vapid = subscription.p256dh

    /** A Bell ringing both of Square's environments and one of Clover's (decision 305). */
    private val info = """{"points_of_sale":{"square":{"production":{"application_id":"sq0idp-live"},"sandbox":{"application_id":"sandbox-sq0idb-dev"}},
        "clover":{"north_america":{"application_id":"MONOKULARBELL"}}},"vapid_public_key":"$vapid","version":"0.3.0"}"""

    private val registered = """{"registration_id":"br_1","point_of_sale":"square","environment":"sandbox","application_id":"sandbox-sq0idb-dev"}"""

    @Test
    fun `info reads each point of sale's environments and their applications, with no Authorization and no body`() = runBlocking<Unit> {
        routes["GET /bell/v1/info"] = { json(200, info) }
        val read = api.info(bell)
        assertEquals(
            BellInfo(
                mapOf(
                    "square" to mapOf("production" to "sq0idp-live", "sandbox" to "sandbox-sq0idb-dev"),
                    "clover" to mapOf("north_america" to "MONOKULARBELL"),
                ),
                vapid,
            ),
            read,
        )
        assertEquals(mapOf("north_america" to "MONOKULARBELL"), read[PointOfSale.Clover])
        val request = seen.single()
        assertNull(request.headers["Authorization"])
        assertEquals(0L, request.bodySize)
    }

    @Test
    fun `a Bell running one point of sale lists it alone, and one this build doesn't know is kept unread`() = runBlocking<Unit> {
        routes["GET /bell/v1/info"] = {
            json(
                200,
                """{"points_of_sale":{"clover":{"sandbox":{"application_id":"MONOKULARBELL"}},"toast":{"us":{"application_id":"t"}}},"vapid_public_key":"$vapid"}""",
            )
        }
        val read = api.info(bell)
        assertNull(read[PointOfSale.Square])
        assertEquals(mapOf("sandbox" to "MONOKULARBELL"), read[PointOfSale.Clover])
    }

    @Test
    fun `an environment this build doesn't know, or one in a shape it doesn't know, is passed over rather than failing the Bell`() = runBlocking<Unit> {
        routes["GET /bell/v1/info"] = {
            json(
                200,
                """{"points_of_sale":{"square":{"production":{"application_id":"sq0idp-live","region":"us"},"staging":{"application_id":"sq0idp-stg"},
                   "beta":{"applications":["x"]},"canary":"sq0idp-c"},"toast":["x"]},"vapid_public_key":"$vapid"}""",
            )
        }
        val read = api.info(bell)
        assertEquals(mapOf("production" to "sq0idp-live", "staging" to "sq0idp-stg"), read[PointOfSale.Square])
        assertEquals(emptyMap(), read.pointsOfSale["toast"])
    }

    /** The body as the Bell gets it, its keys checked at every level (acceptance B5, T5.1): the Bell refuses a field it doesn't know. */
    private fun sentBody(request: RecordedRequest) = Json.parseToJsonElement(request.body!!.utf8()).jsonObject.also { sent ->
        assertEquals(setOf("point_of_sale", "environment", "access_token", "location_id", "subscription"), sent.keys)
        assertEquals(setOf("endpoint", "keys"), sent["subscription"]!!.jsonObject.keys)
        assertEquals(setOf("p256dh", "auth"), sent["subscription"]!!.jsonObject["keys"]!!.jsonObject.keys)
    }

    @Test
    fun `register sends exactly the point of sale, its environment, the token, the location and the subscription's public half`() = runBlocking<Unit> {
        routes["POST /bell/v1/register"] = { json(201, registered) }
        assertEquals(BellRegistered("br_1"), api.register(bell, PointOfSale.Square, "sandbox", "EAAAtoken", "L1", subscription))
        val request = seen.single()
        assertNull(request.headers["Authorization"])
        assertEquals(
            Json.parseToJsonElement(
                """{"point_of_sale":"square","environment":"sandbox","access_token":"EAAAtoken","location_id":"L1",
                   "subscription":{"endpoint":"${subscription.endpoint}","keys":{"p256dh":"${subscription.p256dh}","auth":"${subscription.auth}"}}}""",
            ),
            sentBody(request),
        )
    }

    @Test
    fun `a Clover till registers its merchant as the location and its region as the environment, in the same shape`() = runBlocking<Unit> {
        routes["POST /bell/v1/register"] = {
            json(201, """{"registration_id":"br_2","point_of_sale":"clover","environment":"europe","application_id":"MONOKULARBELL"}""")
        }
        assertEquals(BellRegistered("br_2"), api.register(bell, PointOfSale.Clover, "europe", "clv-token", "MCLOVERDEV001", subscription))
        val request = seen.single()
        assertEquals(
            Json.parseToJsonElement(
                """{"point_of_sale":"clover","environment":"europe","access_token":"clv-token","location_id":"MCLOVERDEV001",
                   "subscription":{"endpoint":"${subscription.endpoint}","keys":{"p256dh":"${subscription.p256dh}","auth":"${subscription.auth}"}}}""",
            ),
            sentBody(request),
        )
        assertFalse("square_access_token" in request.body!!.utf8(), "the old field is never sent")
    }

    @Test
    fun `unregister deletes by the id alone, and a 404 is as good as a 204`() = runBlocking<Unit> {
        routes["DELETE /bell/v1/register/br_1"] = { MockResponse.Builder().code(204).build() }
        api.unregister(bell, "br_1")
        api.unregister(bell, "br_gone")
        assertEquals(listOf("DELETE /bell/v1/register/br_1", "DELETE /bell/v1/register/br_gone"), seen.map { "${it.method} ${it.url.encodedPath}" })
        seen.forEach {
            assertNull(it.headers["Authorization"])
            assertEquals(0L, it.bodySize)
        }
    }

    @Test
    fun `the token is in one request of the three`() = runBlocking<Unit> {
        routes["GET /bell/v1/info"] = { json(200, info) }
        routes["POST /bell/v1/register"] = { json(201, registered) }
        routes["DELETE /bell/v1/register/br_1"] = { MockResponse.Builder().code(204).build() }
        api.info(bell)
        api.register(bell, PointOfSale.Square, "sandbox", "EAAAtoken", "L1", subscription)
        api.unregister(bell, "br_1")
        val carrying = seen.filter { request -> "EAAAtoken" in request.body?.utf8().orEmpty() || request.headers.any { "EAAAtoken" in it.second } }
        assertEquals(listOf("POST"), carrying.map { it.method })
    }

    @Test
    fun `a problem is read by its type's slug, its detail and Retry-After`() = runBlocking<Unit> {
        routes["POST /bell/v1/register"] = { problem(429, "rate-limited", "slow down", retryAfter = 42) }
        val e = assertFailsWith<BellException> { api.register(bell, PointOfSale.Square, "sandbox", "EAAAtoken", "L1", subscription) }
        assertEquals(429, e.status)
        assertEquals("rate-limited", e.type)
        assertEquals("slow down", e.detail)
        assertEquals(42L, e.retryAfter)
    }

    @Test
    fun `an error without a problem is its status alone`() = runBlocking<Unit> {
        routes["POST /bell/v1/register"] = { MockResponse.Builder().code(502).body("<html>Bad gateway</html>").build() }
        val e = assertFailsWith<BellException> { api.register(bell, PointOfSale.Square, "sandbox", "EAAAtoken", "L1", subscription) }
        assertEquals(502, e.status)
        assertNull(e.type)
        assertNull(e.detail)
    }

    @Test
    fun `info from something that isn't a Bell says so`() = runBlocking<Unit> {
        routes["GET /bell/v1/info"] = { MockResponse.Builder().code(200).body("<html>Hello</html>").build() }
        assertFailsWith<NotABellException> { api.info(bell) }
        routes["GET /bell/v1/info"] = { json(200, """{"payments":[]}""") }
        assertFailsWith<NotABellException> { api.info(bell) }
        routes["GET /bell/v1/info"] = { json(200, """{"points_of_sale":{},"vapid_public_key":"$vapid"}""") }
        assertFailsWith<NotABellException> { api.info(bell) }
        routes["GET /bell/v1/info"] = { json(200, """{"vapid_public_key":"$vapid"}""") }
        assertFailsWith<NotABellException> { api.info(bell) }
        routes["GET /bell/v1/info"] = { json(200, info.replace(vapid, "BOS-key")) }
        assertFailsWith<NotABellException> { api.info(bell) }
        routes["GET /bell/v1/info"] = { json(200, info.replace(vapid, vapid.replaceFirst('B', 'A'))) }
        assertFailsWith<NotABellException> { api.info(bell) }
        // The Bell before points of sale: its shape isn't one this till reads (acceptance D2).
        routes["GET /bell/v1/info"] = {
            json(200, """{"square_application_id":"sandbox-sq0idb-dev","environment":"sandbox","vapid_public_key":"$vapid","version":"0.1.0"}""")
        }
        assertFailsWith<NotABellException> { api.info(bell) }
        routes.remove("GET /bell/v1/info")
        assertFailsWith<NotABellException> { api.info(bell) }
    }

    @Test
    fun `a Bell from before environments reads as ringing none of the till's`() = runBlocking<Unit> {
        // One application per point of sale, its environment beside it: no environment maps to an application.
        routes["GET /bell/v1/info"] = {
            json(200, """{"points_of_sale":{"square":{"application_id":"sandbox-sq0idb-dev","environment":"sandbox"}},"vapid_public_key":"$vapid"}""")
        }
        assertEquals(emptyMap(), api.info(bell)[PointOfSale.Square])
    }

    @Test
    fun `info from a Bell that can't answer is its status`() = runBlocking<Unit> {
        routes["GET /bell/v1/info"] = { MockResponse.Builder().code(503).build() }
        assertEquals(503, assertFailsWith<BellException> { api.info(bell) }.status)
    }

    @Test
    fun `a Bell that doesn't answer in time is a timeout`() = runBlocking<Unit> {
        val slow = BellApi(OkHttpClient(), timeout = 200.milliseconds)
        routes["GET /bell/v1/info"] = { MockResponse.Builder().code(200).body(info).headersDelay(2, java.util.concurrent.TimeUnit.SECONDS).build() }
        assertFailsWith<InterruptedIOException> { slow.info(bell) }
    }

    @Test
    fun `a Bell that isn't there is an IOException`() = runBlocking<Unit> {
        server.close()
        val e = assertFailsWith<IOException> { api.info(bell) }
        assertFalse(e is BellException || e is NotABellException || e is InterruptedIOException, "no answer at all: $e")
    }
}
