package com.monokulo.till.receipt

import uniffi.orf_codec_ffi.receiptFromUri
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/** Through the real Rust codec, built for this machine. */
class ReceiptCodeTest {
    private val uri = "monero:8Bx?tx_amount=0.5"

    private fun receipt(lines: List<Receipt.LineItem>) = Receipt(
        receipt = Receipt.Info("#1042", "2026-10-22T09:15:00-07:00"),
        merchant = Receipt.Merchant("The Bean Room", "US"),
        lineItems = lines,
        tax = Receipt.Tax(listOf(Receipt.TaxLine(rate = 0.0888, taxAmount = 1.13))),
        payment = Receipt.Payment("monero"),
        total = 13.88,
        currency = "USD",
    )

    private val lines = listOf(
        Receipt.LineItem("Flat white, oat milk", 2.0, 4.5, 9.0),
        Receipt.LineItem("Almond croissant", 1.0, 3.75, 3.75),
    )

    @Test
    fun `a receipt rides in the payment uri and reads back exactly`() {
        val code = ReceiptCode.of(uri, receipt(lines))
        assertTrue(code.hasReceipt)
        assertTrue(code.uri.startsWith("$uri&orf="), code.uri)
        val back = assertNotNull(receiptFromUri(code.uri))
        assertTrue(back.contains("Almond croissant"), back)
        assertTrue(back.contains("\"total\":13.88"), back)
    }

    @Test
    fun `without a receipt, or one the codec refuses, the plain uri is the code`() {
        assertEquals(PaymentCode(uri, hasReceipt = false), ReceiptCode.of(uri, null))
        // Half a cent can't be carried exactly.
        val refused = ReceiptCode.of(uri, receipt(listOf(Receipt.LineItem("Gum", 1.0, 0.005, 0.005))))
        assertFalse(refused.hasReceipt)
        assertEquals(uri, refused.uri)
        assertNotNull(refused.why)
    }
}
