package com.monokulo.till

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

/*
 * The on-device tests' sales, written as Android's `Stores.kt` saved a shift
 * (`till_sales::shift_json`), which the engine's previews read
 * (`TillPreview.salesJson`). They carry no behaviour: every word, step and
 * ring the screens draw is the engine's, from this JSON.
 */

/** Where a sale's payment is, by the name the saved shift writes. */
enum class SalePhase { Waiting, Seen, Confirming, Paid, Partial, Expired, DoubleSpend, Cancelled }

/** Which of the ring's four steps the phase is at, as the engine's `sale_text::step` counts: 0 once it ended unpaid. */
val SalePhase.step: Int
    get() = when (this) {
        SalePhase.Waiting, SalePhase.Partial -> 1
        SalePhase.Seen -> 2
        SalePhase.Confirming -> 3
        SalePhase.Paid -> 4
        SalePhase.Expired, SalePhase.DoubleSpend, SalePhase.Cancelled -> 0
    }

/** A point of sale, by the name the saved shift writes. */
enum class PointOfSale { Square, Clover }

/** The code a sale shows: the payment URI, with the receipt when one could be added. */
@Serializable
data class PaymentCode(val uri: String, val hasReceipt: Boolean, val why: String? = null)

/** The point-of-sale sale this till found. */
@Serializable
data class FoundSale(
    val id: String,
    val amountMinor: Long,
    val currency: String,
    val reference: String?,
    val orderId: String?,
    val createdAt: String,
    val deviceId: String?,
    val title: String? = null,
    val pointOfSale: PointOfSale = PointOfSale.Square,
)

/** What was bought: an ORF receipt, as the saved shift writes it. */
@Serializable
data class Receipt(
    @SerialName("orf_version") val orfVersion: String = "0.1.0",
    val receipt: Info,
    val merchant: Merchant,
    @SerialName("line_items") val lineItems: List<LineItem>,
    val tax: Tax? = null,
    val total: Double,
    val currency: String,
) {
    @Serializable
    data class Info(
        @SerialName("receipt_id") val receiptId: String,
        @SerialName("issue_timestamp") val issueTimestamp: String,
        @SerialName("receipt_type") val receiptType: String = "sale",
    )

    @Serializable
    data class Merchant(@SerialName("legal_name") val legalName: String, @SerialName("country_code") val countryCode: String)

    @Serializable
    data class LineItem(
        val description: String,
        val quantity: Double,
        @SerialName("unit_price") val unitPrice: Double,
        @SerialName("line_total") val lineTotal: Double,
    )

    @Serializable
    data class Tax(@SerialName("tax_lines") val taxLines: List<TaxLine>)

    @Serializable
    data class TaxLine(val rate: Double, @SerialName("tax_amount") val taxAmount: Double)
}

/** A sale in the saved shift, newest first in its list. */
@Serializable
data class TillSale(
    val orderId: String,
    val amount: String,
    val currency: String,
    val reference: String?,
    val code: PaymentCode,
    val phase: SalePhase,
    val expiresAt: Long,
    val xmr: String,
    val address: String,
    val found: FoundSale? = null,
    val receipt: Receipt? = null,
    val customerName: String? = null,
    val deviceName: String? = null,
    val url: String? = null,
    val settled: Boolean = false,
    val createdAt: Long = 0,
    val seenAt: Long? = null,
    val paidAt: Long? = null,
    val endedAt: Long? = null,
    val closedAt: Long? = null,
    val refundedAt: Long? = null,
    val refundPendingAt: Long? = null,
    val confirmations: Int? = null,
    val confirmationsRequired: Int? = null,
) {
    val refunded: Boolean get() = refundedAt != null

    /** Its code is up: as `TillSale::live` says (not paid, not ended, not closed). */
    val live: Boolean get() = phase in LIVE && closedAt == null
}

private val LIVE = setOf(SalePhase.Waiting, SalePhase.Seen, SalePhase.Confirming, SalePhase.Partial)

private val shiftJson = Json { ignoreUnknownKeys = true; explicitNulls = false }

/** [sales] as the saved shift's JSON. */
fun shiftJson(sales: List<TillSale>): String = shiftJson.encodeToString(ListSerializer(TillSale.serializer()), sales)
