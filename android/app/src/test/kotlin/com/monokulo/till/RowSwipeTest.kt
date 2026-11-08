package com.monokulo.till

import com.monokulo.till.core.SalePhase
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.TillSale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** A shift row swiped to the start: which rows swipe, how far the row follows, and when a release asks. */
class RowSwipeTest {
    private fun sale(id: String, phase: SalePhase = SalePhase.Waiting, closedAt: Long? = null, mine: Boolean = true) = TillSale(
        orderId = id,
        amount = "8.40",
        currency = "USD",
        reference = id,
        code = PaymentCode("monero:4abc?tx_amount=0.1", hasReceipt = mine),
        phase = phase,
        expiresAt = Long.MAX_VALUE / 2,
        xmr = "0.0336",
        address = "4abc",
        found = if (mine) FoundSale("p_$id", 840, "USD", id, "o", "2026-10-22T22:50:00Z", null) else null,
        createdAt = 1_761_000_000,
        closedAt = closedAt,
    )

    @Test
    fun onlyTheRowsWhosePageHasTheXSwipe() {
        assertTrue(RowSwipe.swipes(sale("w")), "a code up")
        assertTrue(RowSwipe.swipes(sale("t", mine = false)), "another till's: its X closes it")
        assertFalse(RowSwipe.swipes(sale("p", SalePhase.Paid)), "paid: its page has no X")
        assertFalse(RowSwipe.swipes(sale("e", SalePhase.Expired)), "ended")
        assertFalse(RowSwipe.swipes(sale("c", closedAt = 1)), "closed for now")
    }

    @Test
    fun theRowFollowsTheFingerToTheStartOnly() {
        assertEquals(0f, RowSwipe.pulled(30f, 360f), "to the end is no swipe")
        assertEquals(-40f, RowSwipe.pulled(-40f, 360f))
        assertEquals(-360f, RowSwipe.pulled(-500f, 360f), "never past its own width")
    }

    @Test
    fun aReleasePastAThirdOrAFlingToTheStartAsks() {
        val third = -360f * RowSwipe.REACH
        assertFalse(RowSwipe.asks(third, 360f, 0f), "a third exactly settles back")
        assertTrue(RowSwipe.asks(third - 1f, 360f, 0f))
        assertFalse(RowSwipe.asks(-20f, 360f, 0f), "a small swipe settles back")
        assertTrue(RowSwipe.asks(-20f, 360f, -RowSwipe.FLING_DP_PER_S), "a short fling to the start")
        assertFalse(RowSwipe.asks(-20f, 360f, 1f - RowSwipe.FLING_DP_PER_S))
        assertFalse(RowSwipe.asks(-200f, 360f, RowSwipe.FLING_DP_PER_S), "past the reach but flung back")
    }
}
