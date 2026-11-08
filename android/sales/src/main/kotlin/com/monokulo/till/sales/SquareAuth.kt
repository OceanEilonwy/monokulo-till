package com.monokulo.till.sales

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.time.Duration
import java.time.Instant

/** Square answered with an error, or the sign-in is gone and must be done again. */
class SquareException(status: Int, message: String, signInAgain: Boolean = false) : PosException(status, message, signInAgain)

/**
 * Square OAuth with PKCE, on the device (`docs/design.md` §1): no secret
 * anywhere. PKCE refresh tokens are single-use and last 90 days, so each
 * refresh saves the new one before anything else, and a lapsed one means
 * signing in again.
 */
class SquareAuth(
    private val http: OkHttpClient,
    private val config: SquareConfig,
    private val store: TokenStore,
    private val now: () -> Instant = Instant::now,
) {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }
    private val lock = Mutex()

    /** The page the cashier signs in on. */
    fun authorizeUrl(pkce: Pkce): String = "${config.connectBase}/oauth2/authorize".toHttpUrl().newBuilder()
        .addQueryParameter("client_id", config.applicationId)
        .addQueryParameter("scope", SquareConfig.SCOPES.joinToString(" "))
        // Signs the seller in afresh each time; the sandbox refuses it.
        .apply { if (!config.sandbox) addQueryParameter("session", "false") }
        .addQueryParameter("state", pkce.state)
        .addQueryParameter("code_challenge", pkce.challenge)
        .addQueryParameter("code_challenge_method", "S256")
        .addQueryParameter("redirect_uri", config.redirectUri)
        .build().toString()

    val signedIn: Boolean get() = store.load() != null

    /** Square's redirect, back in the app: the code for this attempt's [pkce]. */
    suspend fun finish(pkce: Pkce, state: String?, code: String?, error: String?): PosTokens {
        if (error != null) throw SquareException(0, "Square sign-in was cancelled or refused ($error).")
        if (state != pkce.state || code.isNullOrEmpty()) {
            throw SquareException(0, "That sign-in wasn't the one this app started. Try again.")
        }
        val tokens = token(
            mapOf(
                "client_id" to config.applicationId,
                "grant_type" to "authorization_code",
                "code" to code,
                "redirect_uri" to config.redirectUri,
                "code_verifier" to pkce.verifier,
            ),
        )
        lock.withLock { store.save(tokens) }
        return tokens
    }

    /** An access token good for at least a day, refreshing first when it isn't. */
    suspend fun accessToken(): String = lock.withLock {
        val tokens = store.load() ?: throw SquareException(401, "Sign in to Square.", signInAgain = true)
        val expires = runCatching { Instant.parse(tokens.expiresAt) }.getOrDefault(Instant.EPOCH)
        if (expires.isAfter(now().plus(Duration.ofDays(1)))) return@withLock tokens.accessToken
        val refresh = tokens.refreshToken
            ?: throw SquareException(401, "Square's sign-in has ended. Sign in again.", signInAgain = true)
        val renewed = try {
            token(mapOf("client_id" to config.applicationId, "grant_type" to "refresh_token", "refresh_token" to refresh))
        } catch (e: SquareException) {
            if (e.status in 400..499) {
                store.save(null)
                throw SquareException(e.status, "Square's sign-in has ended. Sign in again.", signInAgain = true)
            }
            throw e
        }
        // The old refresh token is spent: keep the new one first.
        store.save(renewed)
        renewed.accessToken
    }

    fun signOut() = store.save(null)

    private suspend fun token(body: Map<String, String>): PosTokens = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url("${config.connectBase}/oauth2/token")
            .header("Square-Version", SquareConfig.API_VERSION)
            .post(JsonObject(body.mapValues { JsonPrimitive(it.value) }).toString().toRequestBody("application/json".toMediaType()))
            .build()
        http.newCall(request).execute().use { response ->
            val text = response.body.string()
            if (!response.isSuccessful) throw SquareException(response.code, squareError(text) ?: "Square said ${response.code}.")
            json.decodeFromString(PosTokens.serializer(), text)
        }
    }
}

/** The first error Square gave, from its `errors` array. */
internal fun squareError(body: String): String? = runCatching {
    val errors = (Json.parseToJsonElement(body) as JsonObject)["errors"] as JsonArray
    val first = errors.first() as JsonObject
    ((first["detail"] ?: first["code"]) as JsonPrimitive).content
}.getOrNull()
