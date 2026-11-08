package com.monokulo.till.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.sse.EventSource
import okhttp3.sse.EventSourceListener
import okhttp3.sse.EventSources
import java.util.concurrent.ConcurrentHashMap

/**
 * A Monokulo answer that wasn't a success: its status and what its
 * problem (`application/problem+json`) said, its `detail` else its
 * `title`. A 401 means the store disconnected this device.
 */
class MonokuloException(
    val status: Int,
    message: String,
    /** The problem's name, the end of its `type` (`invalid-connect-code`), when it was one. */
    val problem: String? = null,
    /** Seconds to wait before asking again, for `rate-limited`. */
    val retryAfter: Long? = null,
) : Exception(message)

/** What the app says about itself when it connects and on every request. */
data class AppIdentity(
    val slug: String,
    val name: String,
    val version: String,
    val platform: String,
    /** This device, for when the merchant didn't name it ("Pixel 8"). */
    val deviceName: String,
) {
    /** The `Monokulo-Client` header's value. */
    val client: String get() = "$slug/$version"
}

/** Something the store's event stream said. */
sealed interface StreamEvent {
    /** It's open: re-read the orders on screen. */
    data object Ready : StreamEvent
    /** Some events were missed: re-read the orders on screen. */
    data object Lagged : StreamEvent
    data class Sale(val event: SaleEvent) : StreamEvent
}

/**
 * A store's Monokulo, as a connected app uses it: `POST /apps/connect`,
 * then Monokulo's HTTP API from its entry, following its links and forms
 * (Monokulo's docs, `/api/` and `/apps/`). Every call runs on
 * [Dispatchers.IO]. [now] is the clock the entry is kept by.
 */
class MonokuloApi(
    private val http: OkHttpClient,
    private val identity: AppIdentity,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }
    private val jsonType = "application/json".toMediaType()

    /** Each key's entry, kept as its `Cache-Control` says, by key and where it was read. */
    private val entries = ConcurrentHashMap<String, Kept>()

    @Serializable
    private data class Redeem(
        val token: String,
        val slug: String,
        val name: String,
        @SerialName("device_name") val deviceName: String,
        val platform: String,
    )

    @Serializable
    private data class Redeemed(
        @SerialName("app_key") val appKey: String,
        val api: String,
        @SerialName("onion_api") val onionApi: String? = null,
    )

    @Serializable
    private data class NewOrder(
        val amount: String,
        val currency: String,
        @SerialName("merchant_order_id") val merchantOrderId: String? = null,
        @SerialName("show_merchant_order_id") val showMerchantOrderId: Boolean = true,
        @SerialName("idempotency_key") val idempotencyKey: String,
    )

    /** A form from `_templates`: how to send it and where, resolved. */
    private data class Form(val method: String, val target: HttpUrl)

    /** The API's entry: where things are, for this key. */
    private data class Entry(
        val store: HttpUrl?,
        val events: HttpUrl?,
        val app: HttpUrl?,
        val onionApi: HttpUrl?,
        val createOrder: Form?,
        val disconnect: Form?,
    )

    /** An entry as read, its `ETag`, and until when (ms) it may be used without asking. */
    private data class Kept(val entry: Entry, val etag: String?, val freshUntil: Long)

    /**
     * Redeems [code], once, at the link's own address, for this device's
     * key, where the store's API is, and where it is on the onion address
     * when the store has one. Going over Tor is left for the caller to
     * choose ([Connection.tor]).
     */
    suspend fun connect(code: ConnectCode): Connection {
        val body = json.encodeToString(
            Redeem.serializer(),
            Redeem(code.token, identity.slug, identity.name, identity.deviceName, identity.platform),
        )
        val request = Request.Builder()
            .url(code.connectUrl)
            .header("Monokulo-Client", identity.client)
            .post(body.toRequestBody(jsonType))
            .build()
        return call(request) { text, used ->
            val redeemed = json.decodeFromString(Redeemed.serializer(), text)
            val api = used.resolve(redeemed.api) ?: throw MonokuloException(0, "The store's answer didn't say where its API is.")
            Connection(redeemed.appKey, api.toString(), redeemed.onionApi?.let(used::resolve)?.toString())
        }
    }

    /** The store as it is now: its name, base currency and wallet. */
    suspend fun store(connection: Connection): Store = followed(connection) { entry ->
        val url = entry.store ?: throw notAllowed("read the store")
        call(keyed(connection, url).get().build()) { text, _ -> json.decodeFromString(Store.serializer(), text) }
    }

    /** This device's connection as the store has it: its name, above all. */
    suspend fun app(connection: Connection): AppConnection = followed(connection) { entry ->
        val url = entry.app ?: throw notAllowed("read its connection")
        call(keyed(connection, url).get().build()) { text, _ -> json.decodeFromString(AppConnection.serializer(), text) }
    }

    /** The entry on the store's onion address (`mk:onion-api`), or null while it has none. */
    suspend fun onionApi(connection: Connection): String? = entry(connection).onionApi?.toString()

    /**
     * Makes an order with the `create-order` form. [idempotencyKey] makes a
     * retry, or a second till finding the same sale, get the same order.
     * Never a description: it would carry what was bought
     * (`docs/design.md` §4).
     */
    suspend fun createOrder(
        connection: Connection,
        amount: String,
        currency: String,
        merchantOrderId: String?,
        idempotencyKey: String,
    ): Order = followed(connection) { entry ->
        val form = entry.createOrder ?: throw notAllowed("make orders")
        val body = json.encodeToString(
            NewOrder.serializer(),
            NewOrder(amount, currency, merchantOrderId, idempotencyKey = idempotencyKey),
        ).toRequestBody(jsonType)
        call(keyed(connection, form.target).method(form.method, body).build(), ::order)
    }

    /** Reads an order again at [url]: its `self` link, or the `mk:order` link of a sale event. */
    suspend fun order(connection: Connection, url: String): Order = try {
        call(keyed(connection, url.toHttpUrl()).get().build(), ::order)
    } catch (e: MonokuloException) {
        if (e.status in STALE) entries.remove(keyOf(connection))
        throw e
    }

    /** Disconnects this device with the `disconnect` form: its key stops working at once. */
    suspend fun disconnect(connection: Connection) {
        try {
            followed(connection) { entry ->
                val form = entry.disconnect ?: throw notAllowed("disconnect itself")
                call(keyed(connection, form.target).method(form.method, null).build()) { _, _ -> }
            }
        } finally {
            entries.remove(keyOf(connection))
        }
    }

    /**
     * The store's sale events (`mk:events`), for as long as it's collected.
     * Ends with a [MonokuloException] when the stream fails; the caller
     * reconnects, reading the entry again first if the stream answered
     * 401, 404 or 405.
     */
    fun events(connection: Connection): Flow<StreamEvent> = callbackFlow {
        val url = entry(connection).events ?: throw notAllowed("follow the store's sales")
        val request = keyed(connection, url).header("Accept", "text/event-stream").get().build()
        val listener = object : EventSourceListener() {
            override fun onEvent(eventSource: EventSource, id: String?, type: String?, data: String) {
                val event = when (type) {
                    "ready" -> StreamEvent.Ready
                    "lagged" -> StreamEvent.Lagged
                    null -> null
                    else -> if (SalePhase.ofEvent(type) != null) {
                        runCatching { StreamEvent.Sale(SaleEvent.decode(data, url)) }.getOrNull()
                    } else {
                        null
                    }
                }
                if (event != null) trySend(event)
            }

            override fun onClosed(eventSource: EventSource) {
                close(MonokuloException(0, "the store closed its event stream"))
            }

            override fun onFailure(eventSource: EventSource, t: Throwable?, response: Response?) {
                val failed = response?.let(::failure)
                if (failed != null && failed.status in STALE) entries.remove(keyOf(connection))
                close(failed ?: MonokuloException(0, t?.message ?: "the event stream failed"))
            }
        }
        val source = EventSources.createFactory(http).newEventSource(request, listener)
        awaitClose { source.cancel() }
    }

    /**
     * Runs [use] with the entry; when something followed from it answers
     * 401, 404 or 405, reads the entry again and tries once more (the key
     * changed, or the API moved on). The entry itself answering 401 is the
     * store having disconnected this device.
     */
    private suspend fun <T> followed(connection: Connection, use: suspend (Entry) -> T): T {
        val entry = entry(connection)
        return try {
            use(entry)
        } catch (e: MonokuloException) {
            if (e.status !in STALE) throw e
            entries.remove(keyOf(connection))
            use(entry(connection))
        }
    }

    private fun keyOf(connection: Connection) = "${connection.appKey} ${connection.entry}"

    /**
     * The entry for [connection]: the copy kept while it's fresh, else read
     * again, with its `ETag` so an unchanged one is a `304` with no body.
     */
    private suspend fun entry(connection: Connection): Entry {
        val key = keyOf(connection)
        val kept = entries[key]
        if (kept != null && now() < kept.freshUntil) return kept.entry
        val request = keyed(connection, connection.entry.toHttpUrl()).get()
            .apply { kept?.etag?.let { header("If-None-Match", it) } }
            .build()
        return withContext(Dispatchers.IO) {
            send(request).use { response ->
                if (response.code == 304 && kept != null) {
                    entries[key] = kept.copy(freshUntil = now() + maxAge(response))
                    return@use kept.entry
                }
                if (!response.isSuccessful) throw failure(response)
                val entry = readEntry(response.body.string(), response.request.url)
                entries[key] = Kept(entry, response.header("ETag"), now() + maxAge(response))
                entry
            }
        }
    }

    private fun readEntry(text: String, used: HttpUrl): Entry {
        val body = json.parseToJsonElement(text).jsonObject
        val links = body["_links"] as? JsonObject
        val templates = body["_templates"] as? JsonObject
        fun link(rel: String) = (links?.get(rel) as? JsonObject)?.text("href")?.let(used::resolve)
        fun form(name: String) = (templates?.get(name) as? JsonObject)?.let { form ->
            val target = form.text("target")?.let(used::resolve) ?: return@let null
            Form(form.text("method")?.uppercase() ?: "POST", target)
        }
        return Entry(
            store = link("mk:store"),
            events = link("mk:events"),
            app = link("mk:app"),
            onionApi = link("mk:onion-api"),
            createOrder = form("create-order"),
            disconnect = form("disconnect"),
        )
    }

    /** How long (ms) an answer may be kept: its `max-age`, else not at all. */
    private fun maxAge(response: Response): Long =
        response.cacheControl.maxAgeSeconds.takeIf { it > 0 && !response.cacheControl.noStore }?.times(1000L) ?: 0L

    private fun order(text: String, used: HttpUrl): Order {
        val body = json.parseToJsonElement(text).jsonObject
        val self = ((body["_links"] as? JsonObject)?.get("self") as? JsonObject)?.text("href")
        val url = self?.let(used::resolve) ?: used
        return json.decodeFromJsonElement(Order.serializer(), body).copy(url = url.toString())
    }

    private fun JsonObject.text(name: String): String? =
        runCatching { get(name)?.jsonPrimitive?.contentOrNull }.getOrNull()

    private fun notAllowed(what: String) =
        MonokuloException(403, "The store's API doesn't let this device $what.")

    private fun keyed(connection: Connection, url: HttpUrl): Request.Builder = Request.Builder()
        .url(url)
        .header("Authorization", "Bearer ${connection.appKey}")
        .header("Monokulo-Client", identity.client)

    /** Runs [request]; [read] gets the body and the URL it came from, which links resolve against. */
    private suspend fun <T> call(request: Request, read: (String, HttpUrl) -> T): T = withContext(Dispatchers.IO) {
        send(request).use { response ->
            if (!response.isSuccessful) throw failure(response)
            read(response.body.string(), response.request.url)
        }
    }

    /**
     * Sends [request], and once more with a proof of work when Monokulo
     * answers `challenge-required` (a busy Monokulo asks one of anyone).
     */
    private fun send(request: Request): Response {
        val response = http.newCall(request).execute()
        if (response.code != 429) return response
        val challenge = runCatching {
            val body = json.parseToJsonElement(response.peekBody(PROBLEM_BYTES).string()).jsonObject
            body["challenge"] as? JsonObject
        }.getOrNull()
        val token = challenge?.text("challenge") ?: return response
        val difficulty = challenge["difficulty"]?.jsonPrimitive?.longOrNull?.toInt() ?: return response
        response.close()
        return http.newCall(request.newBuilder().header("Monokulo-Proof", Proof.solve(token, difficulty)).build()).execute()
    }

    /** What a failed answer said, as its problem has it (RFC 9457). */
    private fun failure(response: Response): MonokuloException {
        val body = runCatching { json.parseToJsonElement(response.peekBody(PROBLEM_BYTES).string()).jsonObject }.getOrNull()
        val said = body?.text("detail") ?: body?.text("title") ?: "HTTP ${response.code}"
        val type = body?.text("type")?.substringAfterLast('/')
        val retryAfter = body?.get("retry_after")?.let { runCatching { it.jsonPrimitive.longOrNull }.getOrNull() }
            ?: response.header("Retry-After")?.toLongOrNull()
        return MonokuloException(response.code, said, type, retryAfter)
    }

    private companion object {
        /** What something followed from the entry answers when the entry should be read again. */
        val STALE = setOf(401, 404, 405)
        const val PROBLEM_BYTES = 16_384L
    }
}
