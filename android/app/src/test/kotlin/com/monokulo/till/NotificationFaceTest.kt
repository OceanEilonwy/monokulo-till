package com.monokulo.till

import androidx.compose.ui.graphics.toArgb
import com.monokulo.till.notify.SaleNotifications
import com.monokulo.till.notify.Shade
import com.monokulo.till.ui.Fixed
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import uniffi.till_screens.Face
import uniffi.till_screens.FaceIcon
import uniffi.till_screens.FaceTint

/**
 * How Android lays out the engine's notification faces. What a face says
 * (its words, icon, tint and step for each phase, and which sale the
 * ongoing notification follows) is `till_screens::notification`'s, tested
 * there test for test from this file's old cases.
 */
class NotificationFaceTest {
    private fun face(
        icon: FaceIcon = FaceIcon.RING_QUARTER,
        tint: FaceTint = FaceTint.ACCENT,
        detail: String? = "2 × Flat white, Almond croissant",
        fullScreen: Boolean = true,
    ) = Face(
        icon = icon,
        tint = tint,
        title = "New Monero sale · Front Counter",
        text = "$13.88 · 3 items · waiting for payment",
        detail = detail,
        publicText = "$13.88 · waiting for payment",
        step = 1u,
        fullScreen = fullScreen,
    )

    @Test
    fun `the chip's ring fills a quarter a step, and paid is the tick`() {
        assertEquals(R.drawable.ic_mark, Shade.icon(FaceIcon.MARK))
        assertEquals(R.drawable.ic_mark_q1, Shade.icon(FaceIcon.RING_QUARTER))
        assertEquals(R.drawable.ic_mark_q2, Shade.icon(FaceIcon.RING_HALF))
        assertEquals(R.drawable.ic_mark_q3, Shade.icon(FaceIcon.RING_THREE_QUARTERS))
        assertEquals(R.drawable.ic_tick, Shade.icon(FaceIcon.TICK))
    }

    @Test
    fun `a sale is in the accent, paid in the paid green`() {
        assertEquals(Fixed.Accent.toArgb(), Shade.color(FaceTint.ACCENT))
        assertEquals(Fixed.Paid.toArgb(), Shade.color(FaceTint.PAID))
    }

    @Test
    fun `what was bought goes on a line of its own, on the private version only`() {
        val f = face()
        assertEquals("$13.88 · 3 items · waiting for payment\n2 × Flat white, Almond croissant", Shade.text(f))
        assertFalse("Flat white" in f.publicText, "the lock screen never shows an item")
        assertEquals("$13.88 · 3 items · waiting for payment", Shade.text(face(detail = null)), "another till's sale has none")
    }

    @Test
    fun `a code still to scan, or a heads-up, is on the sales channel, a payment on its way on the shift's`() {
        assertEquals(SaleNotifications.CHANNEL_SALES, Shade.channel(face(), alert = false))
        assertEquals(SaleNotifications.CHANNEL_SHIFT, Shade.channel(face(icon = FaceIcon.RING_HALF, fullScreen = false), alert = false))
        assertEquals(SaleNotifications.CHANNEL_SALES, Shade.channel(face(fullScreen = false), alert = true))
    }
}
