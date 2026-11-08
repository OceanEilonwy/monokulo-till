package com.monokulo.till.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.Transient

/**
 * An order, as the API answers it: made with the `create-order` form, or
 * read at its `self` link.
 */
@Serializable
data class Order(
    @SerialName("order_id") val orderId: String,
    /** `pending`, `unconfirmed`, `confirming`, `paid`, `partial`, `overpaid`, `expired` or `cancelled`. */
    val status: String = "pending",
    @SerialName("is_final") val isFinal: Boolean = false,
    /** The subaddress to pay: this order's alone. */
    val address: String,
    @SerialName("xmr_amount_piconero") val piconero: Long,
    /** The price it was made with; null for an order Monokulo has no record of. */
    val amount: String? = null,
    val currency: String? = null,
    @SerialName("merchant_order_id") val merchantOrderId: String? = null,
    @SerialName("expires_at") val expiresAt: Long,
    /** Confirmations its payment has; null from a Monokulo that doesn't say. */
    val confirmations: Int? = null,
    /** Confirmations it needs to count as paid: the store's threshold for its amount. Null from a Monokulo that doesn't say. */
    @SerialName("confirmations_required") val confirmationsRequired: Int? = null,
    /** Its `self` link, absolute: where to read it again. */
    @Transient val url: String = "",
) {
    /** The plain payment URI, which the receipt is appended to. */
    val paymentUri: String get() = PaymentUri.build(address, piconero)

    /** The phase it puts a sale in. */
    val phase: SalePhase get() = phaseOf(status)

    companion object {
        fun phaseOf(status: String): SalePhase = when (status) {
            "unconfirmed" -> SalePhase.Seen
            "confirming" -> SalePhase.Confirming
            "paid", "overpaid" -> SalePhase.Paid
            "partial" -> SalePhase.Partial
            "expired" -> SalePhase.Expired
            "cancelled" -> SalePhase.Cancelled
            else -> SalePhase.Waiting
        }
    }
}

/** Monero payment URIs: `monero:<address>?tx_amount=<XMR>`. */
object PaymentUri {
    private const val PICONERO_PER_XMR = 1_000_000_000_000L

    /** [piconero] as XMR, without trailing zeros: 80370584829 is "0.080370584829". */
    fun xmr(piconero: Long): String {
        require(piconero >= 0) { "a negative amount" }
        val whole = piconero / PICONERO_PER_XMR
        val frac = (piconero % PICONERO_PER_XMR).toString().padStart(12, '0').trimEnd('0')
        return if (frac.isEmpty()) "$whole" else "$whole.$frac"
    }

    fun build(address: String, piconero: Long): String = "monero:$address?tx_amount=${xmr(piconero)}"
}
