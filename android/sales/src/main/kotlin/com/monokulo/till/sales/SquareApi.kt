package com.monokulo.till.sales

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

@Serializable
data class Money(val amount: Long, val currency: String)

/** The parts of a Square payment the till reads. */
@Serializable
data class SquarePayment(
    val id: String,
    @SerialName("created_at") val createdAt: String,
    val status: String,
    @SerialName("source_type") val sourceType: String? = null,
    @SerialName("amount_money") val amountMoney: Money,
    @SerialName("total_money") val totalMoney: Money? = null,
    @SerialName("location_id") val locationId: String? = null,
    @SerialName("order_id") val orderId: String? = null,
    @SerialName("receipt_number") val receiptNumber: String? = null,
    @SerialName("external_details") val externalDetails: External? = null,
    @SerialName("device_details") val deviceDetails: Device? = null,
) {
    @Serializable
    data class External(val type: String, val source: String)

    @Serializable
    data class Device(@SerialName("device_id") val deviceId: String? = null)
}

/** The parts of a Square order a receipt is made from. */
@Serializable
data class SquareOrder(
    val id: String,
    @SerialName("location_id") val locationId: String,
    @SerialName("created_at") val createdAt: String,
    @SerialName("line_items") val lineItems: List<LineItem> = emptyList(),
    val taxes: List<OrderTax> = emptyList(),
    @SerialName("total_money") val totalMoney: Money,
    @SerialName("total_tip_money") val totalTipMoney: Money? = null,
    @SerialName("total_discount_money") val totalDiscountMoney: Money? = null,
    /**
     * The order's pickups, deliveries and shipments, kept as plain JSON: only
     * the name of whom each is for is read ([customerName]), and a shape
     * Square changes there never costs the sale its receipt.
     */
    val fulfillments: JsonElement? = null,
) {
    /**
     * Who the order is for, when the seller named them on a pickup, delivery
     * or shipment ("Ana López"): the first recipient's `display_name`, on one
     * line. Null when none names one; an order with only a `customer_id`
     * would need Square's Customers API, which the till's scopes don't reach.
     */
    val customerName: String?
        get() = (fulfillments as? JsonArray).orEmpty().asSequence()
            .mapNotNull { it as? JsonObject }
            .flatMap { f -> RECIPIENT_DETAILS.asSequence().mapNotNull { f[it] as? JsonObject } }
            .mapNotNull { ((it["recipient"] as? JsonObject)?.get("display_name") as? JsonPrimitive)?.takeIf { p -> p.isString } }
            .map { oneLine(it.content).take(MAX_NAME) }
            .firstOrNull { it.isNotEmpty() }

    @Serializable
    data class LineItem(
        val name: String? = null,
        /** A decimal: "2", "1.5". */
        val quantity: String,
        @SerialName("variation_name") val variationName: String? = null,
        val modifiers: List<Modifier> = emptyList(),
        @SerialName("base_price_money") val basePriceMoney: Money? = null,
        @SerialName("gross_sales_money") val grossSalesMoney: Money? = null,
    )

    @Serializable
    data class Modifier(val name: String? = null)

    @Serializable
    data class OrderTax(
        val name: String? = null,
        /** A percentage, as a decimal string: "8.875". */
        val percentage: String? = null,
        @SerialName("applied_money") val appliedMoney: Money? = null,
    )

    companion object {
        /** Where a fulfillment names whom it's for, by its type. */
        private val RECIPIENT_DETAILS = listOf("pickup_details", "delivery_details", "shipment_details")

        /** The most of a name kept: a title shows far less, and a shift keeps it for every sale. */
        private const val MAX_NAME = 80
    }
}

@Serializable
data class SquareLocation(
    val id: String,
    val name: String,
    @SerialName("business_name") val businessName: String? = null,
    val country: String,
    val currency: String,
    val timezone: String? = null,
    val status: String? = null,
)

/** Square's API, as the till uses it, with its own sign-in. */
class SquareApi(
    private val http: OkHttpClient,
    private val config: SquareConfig,
    private val auth: SquareAuth,
) {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    @Serializable
    private data class PaymentPage(val payments: List<SquarePayment> = emptyList(), val cursor: String? = null)

    @Serializable
    private data class OrderResult(val order: SquareOrder)

    @Serializable
    private data class Locations(val locations: List<SquareLocation> = emptyList())

    /** Payments at [locationId] made at or after [beginTime] (RFC 3339), oldest first. */
    suspend fun payments(locationId: String, beginTime: String): List<SquarePayment> {
        val all = mutableListOf<SquarePayment>()
        var cursor: String? = null
        do {
            val url = url("v2/payments").newBuilder()
                .addQueryParameter("location_id", locationId)
                .addQueryParameter("begin_time", beginTime)
                .addQueryParameter("sort_order", "ASC")
                .addQueryParameter("limit", "100")
                .apply { cursor?.let { addQueryParameter("cursor", it) } }
                .build()
            val page = get(url, PaymentPage.serializer())
            all += page.payments
            cursor = page.cursor
        } while (cursor != null)
        return all
    }

    suspend fun order(orderId: String): SquareOrder =
        get(url("v2/orders").newBuilder().addPathSegment(orderId).build(), OrderResult.serializer()).order

    suspend fun locations(): List<SquareLocation> =
        get(url("v2/locations"), Locations.serializer()).locations.filter { it.status != "INACTIVE" }

    /** Refunds a payment in full. [idempotencyKey] makes a retry refund it once. */
    suspend fun refund(paymentId: String, amount: Money, idempotencyKey: String, reason: String) {
        val body = buildJsonObject {
            put("idempotency_key", idempotencyKey)
            put("payment_id", paymentId)
            put("amount_money", JsonObject(mapOf("amount" to JsonPrimitive(amount.amount), "currency" to JsonPrimitive(amount.currency))))
            put("reason", reason)
        }
        send(Request.Builder().url(url("v2/refunds")).post(body.toString().toRequestBody(jsonType)))
    }

    private val jsonType = "application/json".toMediaType()

    private fun url(path: String): HttpUrl = "${config.connectBase}/$path".toHttpUrl()

    private suspend fun <T> get(url: HttpUrl, reader: kotlinx.serialization.KSerializer<T>): T =
        json.decodeFromString(reader, send(Request.Builder().url(url).get()))

    private suspend fun send(builder: Request.Builder): String {
        val token = auth.accessToken()
        return withContext(Dispatchers.IO) {
            val request = builder
                .header("Authorization", "Bearer $token")
                .header("Square-Version", SquareConfig.API_VERSION)
                .build()
            http.newCall(request).execute().use { response ->
                val text = response.body.string()
                if (!response.isSuccessful) {
                    throw SquareException(
                        response.code,
                        squareError(text) ?: "Square said ${response.code}.",
                        signInAgain = response.code == 401,
                    )
                }
                text
            }
        }
    }
}
