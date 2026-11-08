package com.monokulo.till

import com.monokulo.till.ui.ringFill
import com.monokulo.till.ui.ringGreen
import com.monokulo.till.ui.ringSpins
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** The ring's step for each phase, as the engine's `sale_text::step` counts it. */
private object Step {
    const val Waiting = 1
    const val Partial = 1
    const val Seen = 2
    const val Confirming = 3
    const val Paid = 4
    const val Expired = 0
    const val Cancelled = 0
    const val DoubleSpend = 0
}

class PhaseRingTest {
    @Test
    fun `with no count from Monokulo the ring fills a quarter a step and is empty once a sale ended unpaid`() {
        assertEquals(0.25f, ringFill(Step.Waiting))
        assertEquals(0.25f, ringFill(Step.Partial), "part paid is still waiting for the rest")
        assertEquals(0.5f, ringFill(Step.Seen))
        assertEquals(0.75f, ringFill(Step.Confirming))
        assertEquals(1f, ringFill(Step.Paid), "paid closes the ring")
        for (ended in listOf(Step.Expired, Step.Cancelled, Step.DoubleSpend)) assertEquals(0f, ringFill(ended))
    }

    @Test
    fun `seen and confirming, the arc is the payment's way to the store's threshold`() {
        assertEquals(0.12f, ringFill(Step.Seen, 0, 10), "seen: a short arc")
        assertEquals(0.12f, ringFill(Step.Seen, null, 10), "seen has no confirmation yet")
        assertEquals(0.12f + 0.088f, ringFill(Step.Confirming, 1, 10), 1e-6f)
        assertEquals(0.12f + 0.88f * 4 / 10, ringFill(Step.Confirming, 4, 10), 1e-6f)
        assertEquals(0.12f + 0.88f * 8 / 10, ringFill(Step.Confirming, 8, 10), 1e-6f)
        assertEquals(1f, ringFill(Step.Confirming, 12, 10), 1e-6f, "never past the full ring")
        assertEquals(0.12f + 0.88f / 3, ringFill(Step.Confirming, 1, 3), 1e-6f, "a store asking three grows in thirds")
        assertEquals(0.75f, ringFill(Step.Confirming, 0, 0), "no threshold to share out: by step")
        assertEquals(0.25f, ringFill(Step.Waiting, 0, 10), "waiting stays the still quarter whatever the count")
        assertEquals(1f, ringFill(Step.Paid, 4, 10), "paid closes it")
    }

    @Test
    fun `the arc turns green and spins once the payment is seen, and stops paid`() {
        for (step in listOf(Step.Seen, Step.Confirming)) {
            assertTrue(ringSpins(step), "step $step spins")
            assertTrue(ringGreen(step), "step $step is green")
        }
        assertTrue(ringGreen(Step.Paid))
        assertFalse(ringSpins(Step.Paid), "paid stops")
        for (step in listOf(Step.Waiting, Step.Partial, Step.Expired, Step.Cancelled, Step.DoubleSpend)) {
            assertFalse(ringSpins(step), "step $step stands still")
            assertFalse(ringGreen(step), "step $step isn't green")
        }
    }
}
