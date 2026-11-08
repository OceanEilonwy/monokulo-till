package com.monokulo.till.core

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ConnectCodeTest {
    private val token = "ct_" + "0123456789abcdef".repeat(4)

    @Test
    fun `a code from the dashboard gives the store and the token`() {
        val code = ConnectCode.parse("https://pay.yourshop.com/apps/connect#token=$token")!!
        assertEquals("https://pay.yourshop.com/apps/connect", code.connectUrl)
        assertEquals("pay.yourshop.com", code.host)
        assertEquals(token, code.token)
        // Pasted with spaces, a port, and the host's case.
        val pasted = ConnectCode.parse("  https://Pay.Example:8443/apps/connect#token=$token \n")!!
        assertEquals("https://pay.example:8443/apps/connect", pasted.connectUrl)
    }

    @Test
    fun `a Monokulo under a path keeps it`() {
        val code = ConnectCode.parse("https://example.com/monokulo/apps/connect#token=$token")!!
        assertEquals("https://example.com/monokulo/apps/connect", code.connectUrl)
        assertEquals("example.com", code.host)
    }

    @Test
    fun `anything else is not a code`() {
        listOf(
            "https://pay.example/apps/connect",
            "https://pay.example/apps/connect#token=ct_short",
            "https://pay.example/connect/till#token=$token",
            "https://pay.example/myapps/connect#token=$token",
            "https://pay.example/apps/connect/x#token=$token",
            "https://pay.example/apps/connect?x=1#token=$token",
            "https://user@pay.example/apps/connect#token=$token",
            "ftp://pay.example/apps/connect#token=$token",
            "monero:4Abc?tx_amount=1",
            "not a link",
        ).forEach { assertNull(ConnectCode.parse(it), it) }
    }

    @Test
    fun `plain http is for onion addresses, or a debug build's own machine`() {
        val onion = "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvw.onion/apps/connect#token=$token"
        assertEquals(token, ConnectCode.parse(onion)?.token)
        assertTrue(ConnectCode.parse(onion)!!.encrypted, "Tor encrypts the way to an onion address")
        val local = "http://10.0.2.2:8080/apps/connect#token=$token"
        assertNull(ConnectCode.parse(local))
        assertEquals("http://10.0.2.2:8080/apps/connect", ConnectCode.parse(local, allowCleartext = true)?.connectUrl)
        assertFalse(ConnectCode.parse(local, allowCleartext = true)!!.encrypted)
    }
}
