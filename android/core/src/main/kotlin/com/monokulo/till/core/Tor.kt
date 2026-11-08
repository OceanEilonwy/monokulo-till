package com.monokulo.till.core

import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.ProxySelector
import java.net.Socket
import java.net.SocketAddress
import java.net.URI

/**
 * Tor, as the app reaches it: through a Tor app on the device (Orbot)
 * and its SOCKS proxy, `127.0.0.1:9050` (`docs/decisions/android.md` 55).
 * Till has no Tor of its own.
 */
object Tor {
    /** Orbot's SOCKS proxy, where it listens unless told otherwise. */
    val SOCKS: InetSocketAddress = InetSocketAddress.createUnresolved("127.0.0.1", 9050)

    /** Whether [url] is on an onion address: only Tor reaches it. */
    fun isOnion(url: String): Boolean =
        url.toHttpUrlOrNull()?.host?.lowercase()?.endsWith(".onion") == true

    /**
     * Sends onion addresses through Tor's SOCKS proxy at [socks], which
     * looks the name up itself, and everything else straight out.
     */
    class Proxies(private val socks: InetSocketAddress = SOCKS) : ProxySelector() {
        override fun select(uri: URI): List<Proxy> =
            if (uri.host?.lowercase()?.endsWith(".onion") == true) {
                listOf(Proxy(Proxy.Type.SOCKS, socks))
            } else {
                listOf(Proxy.NO_PROXY)
            }

        override fun connectFailed(uri: URI?, sa: SocketAddress?, ioe: IOException?) = Unit
    }

    /** Whether something listens at [socks] (Orbot is running), within [timeoutMs]. */
    fun running(socks: InetSocketAddress = SOCKS, timeoutMs: Int = 500): Boolean = runCatching {
        Socket().use { it.connect(InetSocketAddress(socks.hostString, socks.port), timeoutMs) }
    }.isSuccess
}
