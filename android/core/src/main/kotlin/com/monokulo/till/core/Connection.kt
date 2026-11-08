package com.monokulo.till.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * This device's connection to one store: its key and where the store's
 * API is. The app keeps it (encrypted) until it is disconnected, and
 * nothing else: what the store is called and where its payments go are
 * read from the store each time ([MonokuloApi.store]), never kept
 * (`docs/decisions/android.md` 50).
 */
@Serializable
data class Connection(
    /** The app's own key: `Authorization: Bearer ak_…`. */
    @SerialName("app_key") val appKey: String,
    /**
     * The API's entry, absolute (`https://pay.example/api/v1`, or
     * `https://example.com/monokulo/api/v1` for a Monokulo under a path):
     * the `api` path `POST /apps/connect` answered, resolved against the
     * link the code was redeemed at, so a till that connected over an onion
     * address stays on it.
     */
    val api: String,
    /**
     * The same entry on the store's onion address
     * (`http://….onion/api/v1`), while it has one: the handshake's
     * `onion_api`, or the entry's `mk:onion-api` for a till that connected
     * before Monokulo gave it.
     */
    @SerialName("onion_api") val onionApi: String? = null,
    /** Reach the store over Tor, at [onionApi] (decision 55). */
    val tor: Boolean = false,
) {
    /** Where the app reads the entry: on the onion address when it goes over Tor. */
    val entry: String get() = onionApi?.takeIf { tor } ?: api

    /** Whether the app reaches the store over Tor, by choice or because it connected at an onion address. */
    val overTor: Boolean get() = Tor.isOnion(entry)

    /** The store's host, as the app shows it (`pay.example`). */
    val host: String
        get() = api.toHttpUrlOrNull()?.let { url ->
            if (url.port == defaultPort(url.scheme)) url.host else "${url.host}:${url.port}"
        } ?: api

    /** Whether the connection is encrypted on the way: https, or Tor to an onion address. */
    val encrypted: Boolean get() = overTor || api.startsWith("https://")

    companion object {
        private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

        private fun defaultPort(scheme: String) = if (scheme == "https") 443 else 80

        /** What the app writes to keep [connection]. */
        fun save(connection: Connection): String = json.encodeToString(serializer(), connection)

        /** The connection [saved] holds, or null for none. */
        fun restore(saved: String): Connection? =
            runCatching { json.decodeFromString(serializer(), saved) }.getOrNull()
    }
}
