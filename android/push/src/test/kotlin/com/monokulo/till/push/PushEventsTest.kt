package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** What the connector's callbacks mean: only a decrypted `{"wake":"<the till's point of sale>"}` is a ding (acceptance A8.35, A8.36; decision 133). */
class PushEventsTest {
    private fun ding(text: String, decrypted: Boolean = true, pos: PointOfSale? = PointOfSale.Square) =
        PushEvents.isDing(text.toByteArray(), decrypted, pos)

    @Test
    fun `a ding is the decrypted object with wake square and nothing else`() {
        assertTrue(ding("""{"wake":"square"}"""))
        assertTrue(ding(""" { "wake" : "square" } """), "whitespace doesn't matter")
    }

    @Test
    fun `a Clover till takes wake clover, and each point of sale drops the other's`() {
        assertTrue(ding("""{"wake":"clover"}""", pos = PointOfSale.Clover))
        assertFalse(ding("""{"wake":"square"}""", pos = PointOfSale.Clover), "Square's ding on a Clover till")
        assertFalse(ding("""{"wake":"clover"}""", pos = PointOfSale.Square), "Clover's ding on a Square till")
        assertFalse(ding("""{"wake":"Clover"}""", pos = PointOfSale.Clover), "the wire name is lowercase")
        assertFalse(ding("""{"wake":"clover"}""", decrypted = false, pos = PointOfSale.Clover), "not decrypted")
    }

    @Test
    fun `a till with no point of sale takes no ding`() {
        PointOfSale.entries.forEach { assertFalse(ding("""{"wake":"${it.wire}"}""", pos = null), it.wire) }
    }

    @Test
    fun `anything else is dropped`() {
        assertFalse(ding("""{"wake":"square"}""", decrypted = false), "not decrypted")
        assertFalse(ding(""), "empty")
        assertFalse(ding("wake square"), "not JSON")
        assertFalse(ding("""["wake","square"]"""), "not an object")
        assertFalse(ding(""""square""""), "a string")
        assertFalse(ding("""{"wake":"square","sale":"4500"}"""), "another key")
        assertFalse(ding("""{"wake":"clover"}"""), "another POS")
        assertFalse(ding("""{"wake":["square"]}"""), "not a string")
        assertFalse(ding("""{"wake":null}"""), "null")
        assertFalse(ding("""{}"""), "nothing")
        assertFalse(PushEvents.isDing(byteArrayOf(0xC3.toByte(), 0x28), decrypted = true, PointOfSale.Square), "not UTF-8")
    }

    @Test
    fun `an endpoint needs both keys to be a subscription`() {
        assertEquals(
            PushState.Subscribed(PushSubscription("https://ntfy.sh/up1", "BKey", "auth")),
            PushEvents.endpoint("https://ntfy.sh/up1", "BKey", "auth"),
        )
        assertEquals(PushState.Failed(PushFailure.NoKeys), PushEvents.endpoint("https://ntfy.sh/up1", null, null))
        assertEquals(PushState.Failed(PushFailure.NoKeys), PushEvents.endpoint("https://ntfy.sh/up1", "BKey", ""))
    }
}
