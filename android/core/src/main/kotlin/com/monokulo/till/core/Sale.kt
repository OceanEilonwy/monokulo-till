package com.monokulo.till.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.Transient
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.HttpUrl

/** Where a sale's payment is. */
enum class SalePhase {
    Waiting, Seen, Confirming, Paid, Partial, Expired, DoubleSpend,

    /**
     * Cancelled at the store's POS. Only the POS cancels, and only its own
     * orders, so an app's never is: but one read back still shows as ended
     * unpaid, like [Expired].
     */
    Cancelled;

    /** Nothing more will happen to it. */
    val isFinal: Boolean get() = this == Paid || isUnpaid

    /**
     * Nothing of the money has been seen yet (waiting, or part paid and
     * waiting for the rest), so the code's time left is what matters: the
     * countdown shows beside the code.
     */
    val countsDown: Boolean get() = this == Waiting || this == Partial

    /** It ended without being paid: expired, cancelled or reversed. */
    val isUnpaid: Boolean get() = this == Expired || this == Cancelled || this == DoubleSpend

    companion object {
        /** The phase a `sale.*` event name puts a sale in, or null for another event. */
        fun ofEvent(name: String): SalePhase? = when (name) {
            "sale.waiting" -> Waiting
            "sale.seen" -> Seen
            "sale.confirming" -> Confirming
            "sale.paid" -> Paid
            "sale.partial" -> Partial
            "sale.expired" -> Expired
            "sale.double_spend" -> DoubleSpend
            else -> null
        }
    }
}

/**
 * One `sale.*` event from the store's event stream (`GET /api/v1/events`):
 * an order a connected app made, and what just happened to it. Never
 * anything about what was bought. It links its order (`mk:order`).
 */
@Serializable
data class SaleEvent(
    val event: String,
    @SerialName("order_id") val orderId: String,
    val amount: String,
    val currency: String,
    val address: String,
    @SerialName("xmr_amount_piconero") val piconero: Long,
    @SerialName("merchant_order_id") val merchantOrderId: String? = null,
    @SerialName("expires_at") val expiresAt: Long,
    @SerialName("device_name") val deviceName: String = "",
    val at: Long,
    /** Its order (`mk:order`), absolute: where to read it again. */
    @Transient val orderUrl: String? = null,
) {
    val phase: SalePhase? get() = SalePhase.ofEvent(event)

    companion object {
        internal val json = Json { ignoreUnknownKeys = true }

        /** The event in [text], its `mk:order` link resolved against [stream], the URL it came from. */
        fun decode(text: String, stream: HttpUrl): SaleEvent {
            val body = json.parseToJsonElement(text).jsonObject
            val href = runCatching {
                (body["_links"]?.jsonObject?.get("mk:order") as? JsonObject)?.get("href")?.jsonPrimitive?.contentOrNull
            }.getOrNull()
            return json.decodeFromJsonElement(serializer(), body).copy(orderUrl = href?.let(stream::resolve)?.toString())
        }
    }
}
