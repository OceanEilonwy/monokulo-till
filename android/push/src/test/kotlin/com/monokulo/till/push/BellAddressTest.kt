package com.monokulo.till.push

import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull

/** What Settings › Wake-ups takes as "My own Bell" (acceptance A1.5, decision 98). */
class BellAddressTest {
    @Test
    fun `a bare host is https, and a trailing slash goes`() {
        assertEquals("https://bell.example.org", BellAddress.parse(" bell.example.org/ ")?.url)
        assertEquals("https://bell.example.org", BellAddress.parse("https://bell.example.org")?.url)
    }

    @Test
    fun `a path and a port are kept`() {
        assertEquals("https://example.org/bell", BellAddress.parse("https://example.org/bell/")?.url)
        assertEquals("https://bell.example.org:8445", BellAddress.parse("bell.example.org:8445")?.url)
    }

    @Test
    fun `it is shown as host and path, with a port only when it isn't the scheme's`() {
        assertEquals("bell.example.org", BellAddress.parse("https://bell.example.org:443/")?.display)
        assertEquals("bell.example.org:8445", BellAddress.parse("https://bell.example.org:8445")?.display)
        assertEquals("example.org/bell", BellAddress.parse("https://example.org/bell/")?.display)
        assertEquals("10.0.2.2:8445", BellAddress.parse("http://10.0.2.2:8445", allowHttp = true)?.display)
    }

    @Test
    fun `plain http is for debug builds only`() {
        assertNull(BellAddress.parse("http://bell.example.org"))
        assertEquals(BellAddress.REFUSED, BellAddress.problem("http://bell.example.org"))
        assertEquals("http://10.0.2.2:8445", BellAddress.parse("http://10.0.2.2:8445", allowHttp = true)?.url)
    }

    @Test
    fun `an onion address is refused with its own words`() {
        val onion = "http://pg6mmjiyjmcrsslvykfwnntlaru7p5svn6y2ymmju6nubxndf4pscryd.onion"
        assertNull(BellAddress.parse(onion, allowHttp = true))
        assertEquals(BellAddress.ONION, BellAddress.problem(onion))
        assertEquals(BellAddress.ONION, BellAddress.problem("https://abc.onion/bell"))
    }

    @Test
    fun `nothing, a user name, a query or a fragment is refused`() {
        for (text in listOf("", "   ", "https://user:pw@bell.example.org", "https://bell.example.org/?x=1", "https://bell.example.org/#top", "not a url at all")) {
            assertNull(BellAddress.parse(text), text)
            assertEquals(BellAddress.REFUSED, BellAddress.problem(text), text)
        }
    }

    @Test
    fun `routes hang off the address`() {
        assertEquals("https://example.org/bell/v1/info", BellAddress.parse("https://example.org/bell/")?.route("/v1/info"))
    }

    @Test
    fun `a kept choice restores, and a kept address that isn't one doesn't`() {
        val json = Json { ignoreUnknownKeys = true }
        assertEquals(
            WakeUpChoice.OwnBell(BellAddress("https://bell.example.org")),
            json.decodeFromString(WakeUpChoice.serializer(), """{"type":"own","bell":{"url":"https://bell.example.org"}}"""),
        )
        assertEquals(WakeUpChoice.TimerOnly, json.decodeFromString(WakeUpChoice.serializer(), """{"type":"timer"}"""))
        assertFailsWith<IllegalArgumentException> { BellAddress("javascript:alert(1)") }
        assertFailsWith<IllegalArgumentException> { BellAddress("https://abc.onion") }
    }
}
