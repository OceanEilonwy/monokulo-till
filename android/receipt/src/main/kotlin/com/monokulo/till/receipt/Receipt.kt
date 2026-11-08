package com.monokulo.till.receipt

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/**
 * An Open Receipt Format 0.1 document: the parts a till fills in. The codec
 * (`orf-codec`) refuses fields it doesn't know, so only ORF's own are here;
 * anything else goes in [extensions].
 */
@Serializable
data class Receipt(
    @SerialName("orf_version") val orfVersion: String = "0.1.0",
    val receipt: Info,
    val merchant: Merchant,
    @SerialName("line_items") val lineItems: List<LineItem>,
    val tax: Tax? = null,
    val payment: Payment? = null,
    val total: Double,
    val currency: String,
    val extensions: JsonObject? = null,
) {
    @Serializable
    data class Info(
        @SerialName("receipt_id") val receiptId: String,
        /** RFC 3339, whole seconds: `2026-10-22T09:15:30-07:00`. */
        @SerialName("issue_timestamp") val issueTimestamp: String,
        @SerialName("receipt_type") val receiptType: String = "sale",
    )

    @Serializable
    data class Merchant(
        @SerialName("legal_name") val legalName: String,
        @SerialName("country_code") val countryCode: String,
        @SerialName("display_name") val displayName: String? = null,
    )

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
    data class TaxLine(
        /** A fraction: 0.0875 is 8.75%. */
        val rate: Double,
        @SerialName("tax_amount") val taxAmount: Double,
    )

    @Serializable
    data class Payment(@SerialName("payment_method") val paymentMethod: String)

    fun toJson(): String = json.encodeToString(serializer(), this)

    companion object {
        private val json = Json { explicitNulls = false; encodeDefaults = true }
    }
}
