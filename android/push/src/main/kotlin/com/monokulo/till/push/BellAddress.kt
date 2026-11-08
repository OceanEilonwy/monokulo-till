package com.monokulo.till.push

import kotlinx.serialization.Serializable
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * Where a Monokulo Bell is (`docs/design.md` §3): `https://bell.example`,
 * with a path when the Bell is served under one. Made by [parse], which
 * refuses anything else (decision 98), so a kept one is always a Bell the
 * app would talk to. A Bell is reached directly, never over Tor: an onion
 * address is refused.
 */
@Serializable
data class BellAddress(val url: String) {
    init {
        require(problem(url, allowHttp = true) == null) { "not a Bell address: $url" }
    }

    /** The host (with its port when it isn't the scheme's own) and the path, no scheme: what the screens show. */
    val display: String
        get() {
            val parsed = url.toHttpUrlOrNull() ?: return url
            val host = if (parsed.port == HttpUrl.defaultPort(parsed.scheme)) parsed.host else "${parsed.host}:${parsed.port}"
            return host + parsed.encodedPath.trimEnd('/')
        }

    /** The route's address: [url] followed by [path] (`/v1/info`). */
    fun route(path: String): String = url + path

    override fun toString(): String = url

    companion object {
        /** What the field says under a refused address (acceptance A1.5). */
        const val REFUSED = "Enter the Bell's https address, such as bell.example.org"
        const val ONION = "A Bell can't be an onion address: the Bell is never reached over Tor"

        /**
         * The Bell address [text] names, or null when it isn't one ([problem]
         * says why). A missing scheme means https; a trailing slash is
         * dropped. Plain http is taken in a debug build only ([allowHttp]: a
         * Bell on the developer's machine).
         */
        fun parse(text: String, allowHttp: Boolean = false): BellAddress? {
            if (problem(text, allowHttp) != null) return null
            val parsed = parsed(text) ?: return null
            return BellAddress(parsed.toString().trimEnd('/'))
        }

        /** Why [text] isn't a Bell address, in the cashier's words, or null when it is one. */
        fun problem(text: String, allowHttp: Boolean = false): String? {
            val parsed = parsed(text) ?: return REFUSED
            if (parsed.host.lowercase().endsWith(".onion")) return ONION
            if (parsed.scheme == "http" && !allowHttp) return REFUSED
            if (parsed.username.isNotEmpty() || parsed.password.isNotEmpty()) return REFUSED
            if (parsed.query != null || parsed.fragment != null) return REFUSED
            return null
        }

        /** [text] as a web address, https when it names no scheme; null for nothing or not one. */
        private fun parsed(text: String): HttpUrl? {
            val trimmed = text.trim()
            if (trimmed.isEmpty()) return null
            return (if ("://" in trimmed) trimmed else "https://$trimmed").toHttpUrlOrNull()
        }
    }
}
