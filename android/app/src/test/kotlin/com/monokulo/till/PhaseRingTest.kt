package com.monokulo.till

import com.monokulo.till.core.SalePhase
import com.monokulo.till.ui.ringFill
import com.monokulo.till.ui.ringGreen
import com.monokulo.till.ui.ringSpins
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class PhaseRingTest {
    @Test
    fun `with no count from Monokulo the ring fills a quarter a step and is empty once a sale ended unpaid`() {
        assertEquals(0.25f, ringFill(SalePhase.Waiting))
        assertEquals(0.25f, ringFill(SalePhase.Partial), "part paid is still waiting for the rest")
        assertEquals(0.5f, ringFill(SalePhase.Seen))
        assertEquals(0.75f, ringFill(SalePhase.Confirming))
        assertEquals(1f, ringFill(SalePhase.Paid), "paid closes the ring")
        for (ended in listOf(SalePhase.Expired, SalePhase.Cancelled, SalePhase.DoubleSpend)) assertEquals(0f, ringFill(ended))
    }

    @Test
    fun `seen and confirming, the arc is the payment's way to the store's threshold`() {
        assertEquals(0.12f, ringFill(SalePhase.Seen, 0, 10), "seen: a short arc")
        assertEquals(0.12f, ringFill(SalePhase.Seen, null, 10), "seen has no confirmation yet")
        assertEquals(0.12f + 0.088f, ringFill(SalePhase.Confirming, 1, 10), 1e-6f)
        assertEquals(0.12f + 0.88f * 4 / 10, ringFill(SalePhase.Confirming, 4, 10), 1e-6f)
        assertEquals(0.12f + 0.88f * 8 / 10, ringFill(SalePhase.Confirming, 8, 10), 1e-6f)
        assertEquals(1f, ringFill(SalePhase.Confirming, 12, 10), 1e-6f, "never past the full ring")
        assertEquals(0.12f + 0.88f / 3, ringFill(SalePhase.Confirming, 1, 3), 1e-6f, "a store asking three grows in thirds")
        assertEquals(0.75f, ringFill(SalePhase.Confirming, 0, 0), "no threshold to share out: by step")
        assertEquals(0.25f, ringFill(SalePhase.Waiting, 0, 10), "waiting stays the still quarter whatever the count")
        assertEquals(1f, ringFill(SalePhase.Paid, 4, 10), "paid closes it")
    }

    @Test
    fun `the arc turns green and spins once the payment is seen, and stops paid`() {
        for (phase in listOf(SalePhase.Seen, SalePhase.Confirming)) {
            assertTrue(ringSpins(phase), "$phase spins")
            assertTrue(ringGreen(phase), "$phase is green")
        }
        assertTrue(ringGreen(SalePhase.Paid))
        assertFalse(ringSpins(SalePhase.Paid), "paid stops")
        for (phase in listOf(SalePhase.Waiting, SalePhase.Partial, SalePhase.Expired, SalePhase.Cancelled, SalePhase.DoubleSpend)) {
            assertFalse(ringSpins(phase), "$phase stands still")
            assertFalse(ringGreen(phase), "$phase isn't green")
        }
    }
}
