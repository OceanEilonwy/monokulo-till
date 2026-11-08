package com.monokulo.till.sales

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.time.Duration
import java.time.Instant

/**
 * Clover answered with an error, or the sign-in is gone and must be done
 * again. A `429` says how long Clover asked the till to wait
 * ([retryAfterSeconds]).
 */
class CloverException(
    status: Int,
    message: String,
    signInAgain: Boolean = false,
    val retryAfterSeconds: Long? = null,
) : PosException(status, message, signInAgain)

/**
 * Clover's v2 OAuth for a low-trust app: the authorization code with PKCE,
 * on the device, and no secret anywhere (decision 127). Access tokens last
 * about 30 minutes and are refreshed when under [REFRESH_BEFORE] is left;
 * a refresh token is single-use, so the new pair is saved before anything
 * else, and Clover caps how many a merchant's app may hold, so a refused
 * refresh means signing in again (decision 128).
 */
class CloverAuth(
    private val http: OkHttpClient,
    private val config: CloverConfig,
    private val store: TokenStore,
    private val now: () -> Instant = Instant::now,
) {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }
    private val lock = Mutex()

    /** Clover's token answer: the expirations are Unix seconds, where the rest of Clover is milliseconds. */
    @Serializable
    private data class TokenAnswer(
        @SerialName("access_token") val accessToken: String,
        @SerialName("access_token_expiration") val accessTokenExpiration: Long? = null,
        @SerialName("refresh_token") val refreshToken: String? = null,
    )

    /**
     * The page the merchant signs in on. Clover documents no `state`: one is
     * sent all the same, and [finish] checks it when Clover sends it back.
     */
    fun authorizeUrl(pkce: Pkce): String = "${config.webBase}/oauth/v2/authorize".toHttpUrl().newBuilder()
        .addQueryParameter("client_id", config.applicationId)
        .addQueryParameter("redirect_uri", config.redirectUri)
        .addQueryParameter("code_challenge", pkce.challenge)
        .addQueryParameter("code_challenge_method", "S256")
        .addQueryParameter("state", pkce.state)
        .build().toString()

    val signedIn: Boolean get() = store.load() != null

    /**
     * Clover's redirect, back in the app: the code for this attempt's [pkce]
     * at the merchant the cashier chose. [merchantId] and [code] are needed;
     * [state] must be the attempt's when Clover echoes one; [error] ends it.
     * The tokens are saved with the merchant before they're answered.
     */
    suspend fun finish(pkce: Pkce, merchantId: String?, code: String?, state: String?, error: String?): PosTokens {
        if (error != null) throw CloverException(0, "Clover sign-in was cancelled or refused ($error).")
        if (state != null && state != pkce.state) throw CloverException(0, "That sign-in wasn't the one this app started. Try again.")
        if (code.isNullOrEmpty() || merchantId == null || !MERCHANT_ID.matches(merchantId)) {
            throw CloverException(0, "Clover didn't say which merchant signed in. Try again.")
        }
        val answer = token(
            "oauth/v2/token",
            mapOf("client_id" to config.applicationId, "code" to code, "code_verifier" to pkce.verifier),
        )
        val tokens = answer.toTokens(merchantId)
        lock.withLock { store.save(tokens) }
        return tokens
    }

    /**
     * An access token with at least [REFRESH_BEFORE] left, refreshed first
     * when it hasn't. A refresh Clover refuses (a `4xx` other than `429`)
     * signs out and says to sign in again; a busy or unreachable Clover
     * leaves the sign-in as it was, for the next try.
     */
    suspend fun accessToken(): String = lock.withLock {
        val tokens = signedInTokens()
        val expires = runCatching { Instant.parse(tokens.expiresAt) }.getOrDefault(Instant.EPOCH)
        if (expires.isAfter(now().plus(REFRESH_BEFORE))) return@withLock tokens.accessToken
        refresh(tokens)
    }

    /**
     * Clover refused [refused] before its time (it ended early, or the
     * merchant's tokens were reset): a new one, refreshed now, unless
     * another request already did. The same rules as [accessToken].
     */
    suspend fun refreshAfterRefusal(refused: String): String = lock.withLock {
        val tokens = signedInTokens()
        if (tokens.accessToken != refused) return@withLock tokens.accessToken
        refresh(tokens)
    }

    private fun signedInTokens(): PosTokens = store.load() ?: throw CloverException(401, "Sign in to Clover.", signInAgain = true)

    private suspend fun refresh(tokens: PosTokens): String {
        val refresh = tokens.refreshToken ?: throw signedOut(401)
        val renewed = try {
            token("oauth/v2/refresh", mapOf("client_id" to config.applicationId, "refresh_token" to refresh))
        } catch (e: CloverException) {
            if (e.status in 400..499 && e.status != 429) throw signedOut(e.status)
            throw e
        }
        // The old refresh token is spent: keep the new one first.
        val saved = renewed.toTokens(tokens.merchantId)
        store.save(saved)
        return saved.accessToken
    }

    fun signOut() = store.save(null)

    private fun signedOut(status: Int): CloverException {
        store.save(null)
        return CloverException(status, "Clover's sign-in has ended. Sign in again.", signInAgain = true)
    }

    private fun TokenAnswer.toTokens(merchantId: String) = PosTokens(
        accessToken = accessToken,
        // Seconds, never milliseconds read as seconds; Clover says 30 minutes when it says nothing.
        expiresAt = (accessTokenExpiration?.let(Instant::ofEpochSecond) ?: now().plus(Duration.ofMinutes(30))).toString(),
        refreshToken = refreshToken,
        merchantId = merchantId,
    )

    private suspend fun token(path: String, body: Map<String, String>): TokenAnswer = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url("${config.apiBase}/$path")
            .header("User-Agent", config.userAgent)
            .header("Accept", "application/json")
            .post(JsonObject(body.mapValues { JsonPrimitive(it.value) }).toString().toRequestBody("application/json".toMediaType()))
            .build()
        http.newCall(request).execute().use { response ->
            val text = response.body.string()
            if (!response.isSuccessful) {
                throw CloverException(response.code, cloverError(text) ?: "Clover said ${response.code}.", retryAfterSeconds = retryAfter(response.header("Retry-After")))
            }
            runCatching { json.decodeFromString(TokenAnswer.serializer(), text) }
                .getOrElse { throw CloverException(response.code, "Clover's sign-in answer couldn't be read.") }
        }
    }

    companion object {
        /** An access token with less than this left is refreshed first. */
        val REFRESH_BEFORE: Duration = Duration.ofMinutes(5)

        /** A Clover merchant id: 13 letters and digits in practice; it goes into every `/v3` path. */
        internal val MERCHANT_ID = Regex("[A-Za-z0-9]{1,64}")
    }
}

/** Clover's `message`, from an error body. */
internal fun cloverError(body: String): String? = runCatching {
    ((Json.parseToJsonElement(body) as JsonObject)["message"] as JsonPrimitive).content.takeIf { it.isNotBlank() }
}.getOrNull()

/** A `Retry-After` in seconds, the only form Clover sends; null when absent or unreadable. */
internal fun retryAfter(header: String?): Long? = header?.trim()?.toLongOrNull()?.coerceAtLeast(0)
