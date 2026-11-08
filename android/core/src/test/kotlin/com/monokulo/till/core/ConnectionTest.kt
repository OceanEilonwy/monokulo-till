package com.monokulo.till.core

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** The connection as the app keeps it on the device (`Stores`). */
class ConnectionTest {
    @Test
    fun `a connection is kept as its key and the API's entry, nothing about the store`() {
        val connection = Connection("ak_x", "https://pay.example/api/v1")
        val saved = Connection.save(connection)
        assertEquals("""{"app_key":"ak_x","api":"https://pay.example/api/v1"}""", saved)
        assertEquals(connection, Connection.restore(saved))
    }

    @Test
    fun `a connection kept before Monokulo gave onion addresses carries on, off Tor`() {
        val kept = Connection.restore("""{"app_key":"ak_x","api":"https://pay.example/api/v1"}""")!!
        assertNull(kept.onionApi)
        assertFalse(kept.tor)
        assertEquals("https://pay.example/api/v1", kept.entry)
    }

    @Test
    fun `the onion entry and the choice of Tor are kept with it`() {
        val onion = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/api/v1"
        val connection = Connection("ak_x", "https://pay.example/api/v1", onion, tor = true)
        assertEquals(connection, Connection.restore(Connection.save(connection)))
        assertEquals(onion, connection.entry)
        assertTrue(connection.overTor)
        assertFalse(connection.copy(tor = false).overTor)
        assertTrue(Connection("ak_x", onion).overTor, "connected at the onion address")
    }

    @Test
    fun `nothing usable restores nothing`() {
        assertNull(Connection.restore("not json"))
        assertNull(Connection.restore("""{"api":"https://pay.example/api/v1"}"""))
        assertNull(Connection.restore("""{"app_key":"ak_x"}"""))
    }

    @Test
    fun `the host and whether it's encrypted come from the API's address`() {
        val tls = Connection("ak_x", "https://pay.example/api/v1")
        assertEquals("pay.example", tls.host)
        assertTrue(tls.encrypted)
        val local = Connection("ak_x", "http://10.0.2.2:8080/api/v1")
        assertEquals("10.0.2.2:8080", local.host)
        assertFalse(local.encrypted)
    }

    @Test
    fun `a wallet address is shortened on the device to its first and last 8`() {
        assertEquals("4AdUndXH…NDXhdS7s", Store.shorten("4AdUndXHHZ6cfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2bNDXhdS7s"))
        assertEquals("short", Store.shorten("short"))
    }
}
