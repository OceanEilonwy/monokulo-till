package com.monokulo.till.core

import java.net.URI

/**
 * A connect code from a store's Monokulo dashboard (`docs/design.md` §2):
 * `https://pay.yourshop.com/apps/connect#token=ct_…`, scanned from its QR
 * code or pasted.
 *
 * The link is on the store's own Monokulo, so the app learns where to
 * connect from it, and the token travels in the fragment, which a browser
 * never sends to a server.
 */
data class ConnectCode(
    /**
     * Where the token is redeemed: the link without its `#` part,
     * `https://host[:port][/prefix]/apps/connect`. A Monokulo served under
     * a path (`https://example.com/monokulo`) keeps it here.
     */
    val connectUrl: String,
    val token: String,
) {
    /** The host shown on the confirm screen. */
    val host: String get() = URI(connectUrl).authority

    /** Whether the way there is encrypted: https, or Tor to an onion address. */
    val encrypted: Boolean get() = connectUrl.startsWith("https://") || Tor.isOnion(connectUrl)

    companion object {
        private const val ROUTE = "/apps/connect"
        private val TOKEN = Regex("ct_[0-9a-f]{64}")

        /**
         * The code in [text], or null for anything that isn't one. Plain
         * `http` is only taken for an onion address, or with [allowCleartext]
         * (debug builds, for a Monokulo on the developer's machine).
         */
        fun parse(text: String, allowCleartext: Boolean = false): ConnectCode? {
            val uri = runCatching { URI(text.trim()) }.getOrNull() ?: return null
            val host = uri.host?.lowercase() ?: return null
            val secure = when (uri.scheme?.lowercase()) {
                "https" -> true
                "http" -> host.endsWith(".onion") || allowCleartext
                else -> false
            }
            if (!secure || uri.rawUserInfo != null || uri.rawQuery != null) return null
            // `/apps/connect`, after the Monokulo's own path when it's served under one.
            val path = uri.rawPath?.takeIf { it.endsWith(ROUTE) } ?: return null
            val token = uri.rawFragment
                ?.split('&')
                ?.firstNotNullOfOrNull { it.removePrefix("token=").takeIf { v -> v != it } }
                ?.takeIf { TOKEN.matches(it) }
                ?: return null
            val port = if (uri.port == -1) "" else ":${uri.port}"
            return ConnectCode("${uri.scheme.lowercase()}://$host$port$path", token)
        }
    }
}
