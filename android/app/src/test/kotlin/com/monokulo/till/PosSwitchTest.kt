package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** Changing the point of sale (acceptance T1.4, D6; decision 138): in order, and only what it should touch. */
class PosSwitchTest {
    /** The app's side as a log, with the two things a switch must never touch beside it. */
    private class Recording : PosSwitchSide {
        val log = mutableListOf<String>()
        val wakeUpChoice = "Monokulo Bell"
        val savedSales = listOf("bP9m", "k4Rt")
        override suspend fun takeBellRegistrationBack() {
            log += "Bell: DELETE, awaited"
        }
        override fun signOut(pos: PointOfSale) {
            log += "sign out of ${pos.wire}"
        }
        override fun forgetCursor(pos: PointOfSale) {
            log += "forget ${pos.wire} cursor"
        }
        override fun choose(pos: PointOfSale) {
            log += "choose ${pos.wire}"
        }
    }

    @Test
    fun `switching while signed in takes the Bell's registration back, then signs out, then chooses`() = runBlocking {
        val side = Recording()
        assertTrue(PosSwitch.choose(side, from = PointOfSale.Square, signedIn = true, to = PointOfSale.Clover))
        assertEquals(listOf("Bell: DELETE, awaited", "sign out of square", "forget square cursor", "choose clover"), side.log)
        assertEquals("Monokulo Bell", side.wakeUpChoice, "the Wake-ups choice stays")
        assertEquals(listOf("bP9m", "k4Rt"), side.savedSales, "the shift's sales stay")
    }

    @Test
    fun `and back the other way`() = runBlocking {
        val side = Recording()
        assertTrue(PosSwitch.choose(side, from = PointOfSale.Clover, signedIn = true, to = PointOfSale.Square))
        assertEquals(listOf("Bell: DELETE, awaited", "sign out of clover", "forget clover cursor", "choose square"), side.log)
    }

    @Test
    fun `choosing the one chosen changes nothing`() = runBlocking {
        PointOfSale.entries.forEach { pos ->
            val side = Recording()
            assertFalse(PosSwitch.choose(side, from = pos, signedIn = true, to = pos))
            assertFalse(PosSwitch.choose(side, from = pos, signedIn = false, to = pos))
            assertEquals(emptyList(), side.log, pos.wire)
        }
    }

    @Test
    fun `a first choice only chooses, and one after the sign-in ended still signs out`() = runBlocking {
        val first = Recording()
        assertTrue(PosSwitch.choose(first, from = null, signedIn = false, to = PointOfSale.Clover))
        assertEquals(listOf("choose clover"), first.log)
        val notSignedIn = Recording()
        assertTrue(PosSwitch.choose(notSignedIn, from = PointOfSale.Square, signedIn = false, to = PointOfSale.Clover))
        assertEquals(
            listOf("sign out of square", "forget square cursor", "choose clover"), notSignedIn.log,
            "no registration to take back, but the old location, cursor and tender id go",
        )
    }

    @Test
    fun `signing out takes the Bell's registration back first and keeps the cursor, so signing in again reads on from there`() = runBlocking {
        PointOfSale.entries.forEach { pos ->
            val side = Recording()
            PosSwitch.signOut(side, pos)
            assertEquals(listOf("Bell: DELETE, awaited", "sign out of ${pos.wire}"), side.log, pos.wire)
        }
    }
}
