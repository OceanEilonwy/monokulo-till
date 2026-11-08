package com.monokulo.till.sales

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import java.time.Instant
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Clover's low-trust sign-in, PKCE on the device, against a stand-in Clover. */
class CloverAuthTest {
    private val server = MockWebServer()
    private val tokens = MemoryTokens()
    private var now = Instant.parse("2026-10-28T09:00:00Z")
    private lateinit var config: CloverConfig
    private lateinit var auth: CloverAuth

    @BeforeTest
    fun start() {
        server.start()
        config = CloverConfig(
            "CLOVERAPP0001", CloverEnvironment.NorthAmerica, "https://till.monokulo.com/clover",
            userAgent = "Till/0.3.0", baseUrl = server.url("/").toString().trimEnd('/'),
        )
        auth = CloverAuth(OkHttpClient(), config, tokens) { now }
    }

    @AfterTest fun stop() = server.close()

    private fun json(body: String) = MockResponse.Builder().code(200).setHeader("Content-Type", "application/json").body(body).build()

    /** Clover's token answer: the expirations in Unix seconds. */
    private fun tokenAnswer(access: String, refresh: String, expiresAt: Long) = json(
        """{"access_token":"$access","access_token_expiration":$expiresAt,"refresh_token":"$refresh","refresh_token_expiration":1824714000}""",
    )

    @Test
    fun `each environment has its own sign-in page and API, and a stand-in replaces both`() {
        val sandbox = CloverConfig("A", CloverEnvironment.Sandbox, "https://till.monokulo.com/clover")
        assertEquals("https://sandbox.dev.clover.com", sandbox.webBase)
        assertEquals("https://apisandbox.dev.clover.com", sandbox.apiBase)
        assertEquals("https://api.eu.clover.com", sandbox.copy(environment = CloverEnvironment.Europe).apiBase)
        assertEquals("https://www.la.clover.com", sandbox.copy(environment = CloverEnvironment.LatinAmerica).webBase)
        assertEquals(config.webBase, config.apiBase)
        assertEquals(CloverEnvironment.NorthAmerica, CloverEnvironment.fromWire("north_america"))
        assertEquals(
            listOf("sandbox", "north_america", "europe", "latin_america"),
            CloverEnvironment.entries.map { it.wire },
            "the Bell's environment names",
        )
        assertNull(CloverEnvironment.fromWire("production"))
    }

    @Test
    fun `the sign-in page gets the PKCE challenge, the redirect and a state, and no secret`() {
        val pkce = Pkce.create()
        val url = auth.authorizeUrl(pkce).toHttpUrl()
        assertEquals("/oauth/v2/authorize", url.encodedPath)
        assertEquals("CLOVERAPP0001", url.queryParameter("client_id"))
        assertEquals("https://till.monokulo.com/clover", url.queryParameter("redirect_uri"))
        assertEquals(pkce.challenge, url.queryParameter("code_challenge"))
        assertEquals("S256", url.queryParameter("code_challenge_method"))
        assertEquals(pkce.state, url.queryParameter("state"))
        assertEquals(
            setOf("client_id", "redirect_uri", "code_challenge", "code_challenge_method", "state"),
            url.queryParameterNames,
        )
    }

    @Test
    fun `finishing posts exactly the code and verifier, and keeps the tokens with the merchant, the expiry as RFC 3339`() = runBlocking {
        val pkce = Pkce.create()
        server.enqueue(tokenAnswer("acc1", "ref1", 1761600000))
        val signedIn = auth.finish(pkce, "MCLOVER00001", "code1", state = null, error = null)
        assertEquals(PosTokens("acc1", "2025-10-27T21:20:00Z", "ref1", "MCLOVER00001"), signedIn, "Unix seconds, never read as milliseconds")
        assertEquals(signedIn, tokens.tokens)
        assertTrue(auth.signedIn)
        val request = server.takeRequest()
        assertEquals("/oauth/v2/token", request.url.encodedPath)
        assertEquals("Till/0.3.0", request.headers["User-Agent"])
        val body = Json.parseToJsonElement(request.body!!.utf8()).jsonObject
        assertEquals(setOf("client_id", "code", "code_verifier"), body.keys, "no secret, no grant type Clover doesn't ask for")
        assertEquals(pkce.verifier, body["code_verifier"]!!.jsonPrimitive.content)
        assertEquals("code1", body["code"]!!.jsonPrimitive.content)
    }

    @Test
    fun `an echoed state must be the attempt's, and a redirect needs its merchant and code`() = runBlocking {
        val pkce = Pkce.create()
        val other = assertFailsWith<CloverException> { auth.finish(pkce, "MCLOVER00001", "code", state = "other", error = null) }
        assertEquals("That sign-in wasn't the one this app started. Try again.", other.message)
        assertFailsWith<CloverException> { auth.finish(pkce, null, "code", null, null) }
        assertFailsWith<CloverException> { auth.finish(pkce, "MCLOVER00001", null, null, null) }
        assertFailsWith<CloverException> { auth.finish(pkce, "../tenders", "code", null, null) }
        val refused = assertFailsWith<CloverException> { auth.finish(pkce, null, null, null, "access_denied") }
        assertEquals("Clover sign-in was cancelled or refused (access_denied).", refused.message)
        assertEquals(0, server.requestCount, "Clover isn't asked")
        assertFalse(auth.signedIn)

        server.enqueue(tokenAnswer("acc1", "ref1", 1793179800))
        auth.finish(pkce, "MCLOVER00001", "code", state = pkce.state, error = null)
        assertEquals("2026-10-28T09:30:00Z", tokens.tokens?.expiresAt)
    }

    @Test
    fun `a token with under five minutes left is refreshed first and the new pair kept`() = runBlocking {
        tokens.tokens = PosTokens("old", "2026-10-28T09:04:59Z", "ref1", "MCLOVER00001")
        server.enqueue(tokenAnswer("new", "ref2", 1793179800))
        assertEquals("new", auth.accessToken())
        assertEquals(PosTokens("new", "2026-10-28T09:30:00Z", "ref2", "MCLOVER00001"), tokens.tokens)
        val request = server.takeRequest()
        assertEquals("/oauth/v2/refresh", request.url.encodedPath)
        val body = Json.parseToJsonElement(request.body!!.utf8()).jsonObject
        assertEquals(setOf("client_id", "refresh_token"), body.keys)
        assertEquals("ref1", body["refresh_token"]!!.jsonPrimitive.content)
        // 25 minutes left now: no request.
        now = now.plusSeconds(60)
        assertEquals("new", auth.accessToken())
        assertEquals(1, server.requestCount)
    }

    @Test
    fun `a refused refresh signs out and says to sign in again, once`() = runBlocking {
        tokens.tokens = PosTokens("old", "2026-10-28T09:01:00Z", "ref1", "MCLOVER00001")
        server.enqueue(MockResponse.Builder().code(401).body("""{"message":"Invalid refresh token"}""").build())
        val e = assertFailsWith<CloverException> { auth.accessToken() }
        assertTrue(e.signInAgain)
        assertEquals("Clover's sign-in has ended. Sign in again.", e.message)
        assertNull(tokens.tokens)
        assertFalse(auth.signedIn)
        val again = assertFailsWith<CloverException> { auth.accessToken() }
        assertTrue(again.signInAgain)
        assertEquals(1, server.requestCount, "no second refresh with a spent token")
    }

    @Test
    fun `a busy or failing Clover leaves the sign-in for the next try`() = runBlocking {
        val kept = PosTokens("old", "2026-10-28T09:01:00Z", "ref1", "MCLOVER00001")
        tokens.tokens = kept
        server.enqueue(MockResponse.Builder().code(503).body("""{"message":"Service unavailable"}""").build())
        val down = assertFailsWith<CloverException> { auth.accessToken() }
        assertFalse(down.signInAgain)
        assertEquals("Service unavailable", down.message)
        assertEquals(kept, tokens.tokens)
        server.enqueue(MockResponse.Builder().code(429).setHeader("Retry-After", "3").build())
        val busy = assertFailsWith<CloverException> { auth.accessToken() }
        assertFalse(busy.signInAgain, "a 429 is a 4xx that isn't a refusal")
        assertEquals(3, busy.retryAfterSeconds)
        assertEquals(kept, tokens.tokens)
    }

    @Test
    fun `signing out forgets the tokens`() {
        tokens.tokens = PosTokens("acc", "2026-10-28T09:30:00Z", "ref", "MCLOVER00001")
        auth.signOut()
        assertNull(tokens.tokens)
    }
}
