package com.monokulo.till.core

import java.net.InetSocketAddress
import java.net.Proxy
import java.net.ServerSocket
import java.net.URI
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class TorTest {
    @Test
    fun `onion addresses go through Tor's SOCKS proxy, and nothing else does`() {
        val proxies = Tor.Proxies()
        val onion = proxies.select(URI("http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1")).single()
        assertEquals(Proxy.Type.SOCKS, onion.type())
        val socks = onion.address() as InetSocketAddress
        assertTrue(socks.isUnresolved && socks.hostString == "127.0.0.1" && socks.port == 9050)
        assertEquals(Proxy.NO_PROXY, proxies.select(URI("https://pay.example/api/v1")).single())
    }

    @Test
    fun `only an onion host is an onion address`() {
        assertTrue(Tor.isOnion("http://abc.onion/api/v1"))
        assertTrue(Tor.isOnion("http://ABC.ONION:8080/x"))
        assertFalse(Tor.isOnion("https://onion.example/api/v1"))
        assertFalse(Tor.isOnion("not a url"))
    }

    @Test
    fun `Tor is running when something listens on its proxy port`() {
        ServerSocket(0).use { listening ->
            assertTrue(Tor.running(InetSocketAddress.createUnresolved("127.0.0.1", listening.localPort)))
        }
        val free = ServerSocket(0).use { it.localPort }
        assertFalse(Tor.running(InetSocketAddress.createUnresolved("127.0.0.1", free)))
    }
}
