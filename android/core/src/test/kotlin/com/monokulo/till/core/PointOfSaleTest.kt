package com.monokulo.till.core

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/** The points of sale by the names a Bell gives them (`docs/bell.md`): lowercase, nothing else read. */
class PointOfSaleTest {
    @Test
    fun `each wire name reads back as its point of sale`() {
        PointOfSale.entries.forEach { assertEquals(it, PointOfSale.fromWire(it.wire)) }
        assertEquals(listOf("square", "clover"), PointOfSale.entries.map { it.wire })
        assertEquals(listOf("Square", "Clover"), PointOfSale.entries.map { it.displayName })
    }

    @Test
    fun `a name this build doesn't know, or another spelling, is none`() {
        assertNull(PointOfSale.fromWire("toast"))
        assertNull(PointOfSale.fromWire("Square"), "the wire name is lowercase")
        assertNull(PointOfSale.fromWire(" clover"))
        assertNull(PointOfSale.fromWire(""))
    }
}
