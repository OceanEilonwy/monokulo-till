package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import java.net.Proxy
import java.util.Base64
import java.util.concurrent.TimeUnit
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/** What `POST /v1/register` answers (`docs/bell.md`). */
@Serializable
data class BellRegistered(
    @SerialName("registration_id") val registrationId: String,
)

/** The Bell's three routes a till uses, and nothing else. [BellWakeUps] drives it; tests fake it. */
interface BellClient {
    /** `GET /v1/info`. */
    suspend fun info(bell: BellAddress): BellInfo

    /**
     * `POST /v1/register` for [pointOfSale] in [environment] (decision 305): the one request
     * that carries the point of sale's token (A9.1). [locationId] is the Square location, or
     * the Clover merchant (a Clover merchant is its own location: acceptance D1).
     */
    suspend fun register(
        bell: BellAddress,
        pointOfSale: PointOfSale,
        environment: String,
        accessToken: String,
        locationId: String,
        subscription: PushSubscription,
    ): BellRegistered

    /** `DELETE /v1/register/{id}`: a `404` (taken back already) is as good as a `204`. */
    suspend fun unregister(bell: BellAddress, registrationId: String)
}

/** The Bell answered with an error: its problem's [type] (the slug of `docs/bell.md`'s section), [detail] and [retryAfter] seconds. */
class BellException(
    val status: Int,
    val type: String?,
    val detail: String?,
    val retryAfter: Long?,
) : IOException("the Bell answered $status${type?.let { " $it" }.orEmpty()}")

/** An answer that isn't a Bell's: a web page at a wrong address, say. */
class NotABellException : IOException("not a Bell")

/**
 * The Bell's HTTP API (`docs/bell.md`), over the app's client. The Bell is
 * reached directly, never through Orbot (decision 98), and every call ends
 * within [timeout]. No request carries an `Authorization` header; the
 * point of sale's token goes in [register]'s body and nowhere else (A9.1, A9.2).
 */
class BellApi(http: OkHttpClient, timeout: Duration = 10.seconds) : BellClient {
    private val http = http.newBuilder()
        .proxy(Proxy.NO_PROXY)
        .callTimeout(timeout.inWholeMilliseconds, TimeUnit.MILLISECONDS)
        .build()
    private val json = Json { ignoreUnknownKeys = true }

    override suspend fun info(bell: BellAddress): BellInfo = call(Request.Builder().url(bell.route("/v1/info")).get()) { response, text ->
        if (!response.isSuccessful) throw failure(response, text, notABell = response.code < 500)
        val wire = runCatching { json.decodeFromString(Info.serializer(), text) }.getOrNull() ?: throw NotABellException()
        if (wire.pointsOfSale.isEmpty() || !isP256Point(wire.vapidPublicKey)) throw NotABellException()
        // The till reads its own point of sale's own environment only (BellWakeUps): a Bell may run a point of
        // sale or an environment this build doesn't know, in a shape it doesn't know, and that is never read.
        BellInfo(wire.pointsOfSale.mapValues { (_, environments) -> applications(environments) }, wire.vapidPublicKey)
    }

    override suspend fun register(
        bell: BellAddress,
        pointOfSale: PointOfSale,
        environment: String,
        accessToken: String,
        locationId: String,
        subscription: PushSubscription,
    ): BellRegistered {
        val body = RegisterBody(
            pointOfSale = pointOfSale.wire,
            environment = environment,
            accessToken = accessToken,
            locationId = locationId,
            subscription = RegisterBody.Subscription(subscription.endpoint, RegisterBody.Keys(subscription.p256dh, subscription.auth)),
        )
        val request = Request.Builder().url(bell.route("/v1/register"))
            .post(json.encodeToString(RegisterBody.serializer(), body).toRequestBody(JSON))
        return call(request) { response, text ->
            if (!response.isSuccessful) throw failure(response, text, notABell = false)
            runCatching { json.decodeFromString(BellRegistered.serializer(), text) }.getOrNull() ?: throw NotABellException()
        }
    }

    override suspend fun unregister(bell: BellAddress, registrationId: String) =
        call(Request.Builder().url(bell.route("/v1/register/$registrationId")).delete()) { response, text ->
            if (!response.isSuccessful && response.code != 404) throw failure(response, text, notABell = false)
        }

    private suspend fun <T> call(request: Request.Builder, read: (Response, String) -> T): T = withContext(Dispatchers.IO) {
        http.newCall(request.header("Accept", "application/json").build()).execute().use { response ->
            read(response, response.body.string())
        }
    }

    /** An error answer: the Bell's problem when it gave one, else [NotABellException] when [notABell] or a bare status. */
    private fun failure(response: Response, text: String, notABell: Boolean): IOException {
        val problem = runCatching { json.decodeFromString(Problem.serializer(), text) }.getOrNull()
            ?.takeIf { response.header("Content-Type").orEmpty().startsWith("application/problem+json") }
        if (problem == null && notABell) return NotABellException()
        return BellException(
            status = response.code,
            // The type is the section's address on the Bell's page: `…/docs/bell.md#rate-limited`.
            type = problem?.type?.substringAfterLast('#')?.takeIf { it.isNotEmpty() },
            detail = problem?.detail?.takeIf { it.isNotBlank() },
            retryAfter = response.header("Retry-After")?.trim()?.toLongOrNull() ?: problem?.retryAfter,
        )
    }

    /** `GET /v1/info` as it comes: each point of sale's environments are read by [applications]. */
    @Serializable
    private data class Info(
        @SerialName("points_of_sale") val pointsOfSale: Map<String, JsonElement>,
        @SerialName("vapid_public_key") val vapidPublicKey: String,
    )

    /** One shape for every point of sale (`docs/bell.md`, acceptance D1); the Bell refuses a field it doesn't know. */
    @Serializable
    private data class RegisterBody(
        @SerialName("point_of_sale") val pointOfSale: String,
        val environment: String,
        @SerialName("access_token") val accessToken: String,
        @SerialName("location_id") val locationId: String,
        val subscription: Subscription,
    ) {
        @Serializable
        data class Subscription(val endpoint: String, val keys: Keys)

        @Serializable
        data class Keys(val p256dh: String, val auth: String)
    }

    @Serializable
    private data class Problem(
        val type: String? = null,
        val detail: String? = null,
        @SerialName("retry_after") val retryAfter: Long? = null,
    )

    private companion object {
        val JSON = "application/json".toMediaType()

        /** A point of sale's `{environment: {"application_id": …}}` as environment to application; an entry of another shape is left out. */
        fun applications(environments: JsonElement): Map<String, String> = (environments as? JsonObject).orEmpty().mapNotNull { (environment, entry) ->
            val id = ((entry as? JsonObject)?.get("application_id") as? JsonPrimitive)?.takeIf { it.isString }?.content
            id?.let { environment to it }
        }.toMap()

        /** An uncompressed P-256 public point, base64url: 65 bytes, the first 4 (what a VAPID key is). */
        fun isP256Point(key: String): Boolean {
            val bytes = runCatching { Base64.getUrlDecoder().decode(key.trimEnd('=')) }.getOrNull() ?: return false
            return bytes.size == 65 && bytes[0] == 4.toByte()
        }
    }
}
