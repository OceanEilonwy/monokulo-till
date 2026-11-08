package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.PosException
import com.monokulo.till.sales.Refunds
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

/** The source of a till not signed in to a point of sale (decision 143). */
class NoSourceTest {
    @Test
    fun `a Square sale's refund fails with nothing sent, so the till never marks it refunded`() = runBlocking {
        val sale = FoundSale("p_1", 840, "USD", "bP9m", "o_1", "2026-10-22T22:50:00Z", null)
        val refunds = NoSource.refunds as Refunds.ByTill
        val refused = assertFailsWith<PosException> { refunds.refund(sale) }
        assertEquals("Sign in to Square first", refused.message)
        assertEquals(PointOfSale.Square, NoSource.pointOfSale, "a Clover sale isn't its own, so Till marks it to refund in Clover")
    }
}
