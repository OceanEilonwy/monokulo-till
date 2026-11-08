package com.monokulo.till.receipt

import kotlinx.serialization.Serializable
import uniffi.orf_codec_ffi.OrfException
import uniffi.orf_codec_ffi.appendReceipt

/** The code to show for a sale: the payment URI, with the receipt when it could be added. */
@Serializable
data class PaymentCode(
    val uri: String,
    /** Whether [uri] carries the receipt. */
    val hasReceipt: Boolean,
    /** Why it doesn't, when it doesn't but one was asked for. */
    val why: String? = null,
)

/**
 * Adds receipts to payment URIs (`docs/design.md` §4). Paying never depends
 * on the receipt: whenever one can't be added, the plain URI is the code.
 */
object ReceiptCode {
    /** [paymentUri] (as the order gave it), with [receipt] in it when there is one and it encodes. */
    fun of(paymentUri: String, receipt: Receipt?): PaymentCode {
        if (receipt == null) return PaymentCode(paymentUri, hasReceipt = false)
        return try {
            PaymentCode(appendReceipt(paymentUri, receipt.toJson()), hasReceipt = true)
        } catch (e: OrfException) {
            PaymentCode(paymentUri, hasReceipt = false, why = e.message)
        }
    }
}
