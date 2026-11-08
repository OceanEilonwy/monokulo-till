package com.monokulo.till.sales

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.Locale

/**
 * The Clover merchant the till signed in at: its name for the receipt, and
 * the currency and time zone its payments and orders are in (`/properties`).
 */
@Serializable
data class CloverMerchant(
    val id: String,
    val name: String,
    /** ISO 4217, the merchant's `defaultCurrency`. */
    val currency: String,
    val timezone: String? = null,
    /** ISO 3166 alpha-2, which a receipt needs: from the address, else the locale. */
    val country: String? = null,
)

/** One of the merchant's tenders (payment types). */
@Serializable
data class CloverTender(
    val id: String,
    val label: String? = null,
    val enabled: Boolean = true,
    val visible: Boolean = true,
)

/** The parts of a Clover payment the till reads. Times are Unix milliseconds. */
@Serializable
data class CloverPayment(
    val id: String,
    /** In the currency's minor unit; a tip is apart from it. */
    val amount: Long,
    /** `SUCCESS`, `FAIL`, `VOIDED`, … */
    val result: String,
    val createdTime: Long,
    val voided: Boolean = false,
    val tender: Ref? = null,
    val order: Order? = null,
    val device: Ref? = null,
    val refunds: Elements<CloverRefund> = Elements(),
) {
    @Serializable
    data class Ref(val id: String)

    /** The payment's order, expanded: its id, and the title the cashier gave it ("Table 4"). */
    @Serializable
    data class Order(val id: String, val title: String? = null)
}

@Serializable
data class CloverRefund(val id: String, val amount: Long, val voided: Boolean = false)

/** The parts of a Clover order a receipt is made from. */
@Serializable
data class CloverOrder(
    val id: String,
    val currency: String? = null,
    /** In the currency's minor unit, tax included. */
    val total: Long,
    val createdTime: Long,
    /** `OPEN`, `PAID`, `REFUNDED`, `PARTIALLY_REFUNDED`, … */
    val paymentState: String? = null,
    val lineItems: Elements<LineItem> = Elements(),
) {
    /**
     * Clover repeats a line item once per unit; an item sold by measure has
     * one line with [unitQty], its quantity in thousandths, at [price] each.
     */
    @Serializable
    data class LineItem(
        val id: String,
        val name: String? = null,
        val price: Long,
        val unitQty: Long? = null,
        val note: String? = null,
    )
}

/** How Clover wraps a collection, nested or not. */
@Serializable
data class Elements<T>(val elements: List<T> = emptyList())

/**
 * Clover's REST API (`/v3`), as the till uses it, with its own sign-in.
 * Every request names the till ([CloverConfig.userAgent]), which Clover
 * requires. A `401` is met with a fresh token, then means signing in
 * again; a `429` says how long to wait.
 */
class CloverApi(
    private val http: OkHttpClient,
    private val config: CloverConfig,
    private val auth: CloverAuth,
) {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    @Serializable
    private data class Merchant(val id: String, val name: String? = null, val address: Address? = null) {
        @Serializable
        data class Address(val country: String? = null)
    }

    @Serializable
    private data class Properties(val defaultCurrency: String? = null, val timezone: String? = null, val locale: String? = null)

    /** The merchant, with its address, then its properties. */
    suspend fun merchant(merchantId: String): CloverMerchant {
        val merchant = get(merchantUrl(merchantId).newBuilder().addQueryParameter("expand", "address").build(), Merchant.serializer())
        if (merchant.id != merchantId) throw CloverException(0, "Clover answered for another merchant. Sign in again.", signInAgain = true)
        val properties = get(merchantUrl(merchantId, "properties"), Properties.serializer())
        val currency = properties.defaultCurrency?.takeIf { CURRENCY.matches(it) }
            ?: throw CloverException(0, "Clover didn't say the merchant's currency.")
        return CloverMerchant(
            id = merchant.id,
            name = merchant.name?.takeIf { it.isNotBlank() } ?: "Clover merchant",
            currency = currency,
            timezone = properties.timezone,
            country = country(merchant.address?.country, properties.locale),
        )
    }

    suspend fun tenders(merchantId: String): List<CloverTender> =
        get(
            merchantUrl(merchantId, "tenders").newBuilder().addQueryParameter("limit", "1000").build(),
            Elements.serializer(CloverTender.serializer()),
        ).elements

    /**
     * Adds a tender labelled [label]. One made over REST must take a system
     * tender's `labelKey`: the external payment's (decision 129). It opens
     * no cash drawer and takes no tips. Needs Merchant write.
     */
    suspend fun createTender(merchantId: String, label: String): CloverTender {
        val body = buildJsonObject {
            put("label", label)
            put("labelKey", EXTERNAL_PAYMENT)
            put("enabled", true)
            put("visible", true)
            put("opensCashDrawer", false)
            put("supportsTipping", false)
        }
        val text = send(Request.Builder().url(merchantUrl(merchantId, "tenders")).post(body.toString().toRequestBody(jsonType)))
        return json.decodeFromString(CloverTender.serializer(), text)
    }

    /**
     * The merchant's payments with [tenderId] made at or after [sinceMs]
     * (Unix ms), oldest first, with their tender, order and refunds: page
     * after page until a short one.
     */
    suspend fun payments(merchantId: String, tenderId: String, sinceMs: Long): List<CloverPayment> {
        val all = mutableListOf<CloverPayment>()
        for (page in 0 until MAX_PAGES) {
            val url = merchantUrl(merchantId, "payments").newBuilder()
                .addQueryParameter("filter", "tender.id=$tenderId")
                .addQueryParameter("filter", "createdTime>=$sinceMs")
                .addQueryParameter("expand", "tender,order,refunds")
                .addQueryParameter("orderBy", "createdTime ASC")
                .addQueryParameter("limit", PAGE.toString())
                .addQueryParameter("offset", (page * PAGE).toString())
                .build()
            val elements = get(url, Elements.serializer(CloverPayment.serializer())).elements
            all += elements
            if (elements.size < PAGE) break
        }
        return all
    }

    /** One payment, with its tender, order and refunds. */
    suspend fun payment(merchantId: String, paymentId: String): CloverPayment =
        get(
            merchantUrl(merchantId, "payments", paymentId).newBuilder().addQueryParameter("expand", "tender,order,refunds").build(),
            CloverPayment.serializer(),
        )

    /** One order, with its line items. */
    suspend fun order(merchantId: String, orderId: String): CloverOrder =
        get(merchantUrl(merchantId, "orders", orderId).newBuilder().addQueryParameter("expand", "lineItems").build(), CloverOrder.serializer())

    private val jsonType = "application/json".toMediaType()

    private fun merchantUrl(merchantId: String, vararg path: String): HttpUrl =
        "${config.apiBase}/v3/merchants".toHttpUrl().newBuilder()
            .addPathSegment(merchantId)
            .apply { path.forEach(::addPathSegment) }
            .build()

    private suspend fun <T> get(url: HttpUrl, reader: KSerializer<T>): T =
        json.decodeFromString(reader, send(Request.Builder().url(url).get()))

    /**
     * One request with the sign-in's access token. A `401` before the token's
     * time is up (Clover ended it early) is answered by refreshing it and
     * asking once more. A fresh token refused too can't be helped by
     * another: the till signs out, so no pass after spends a refresh token.
     */
    private suspend fun send(builder: Request.Builder): String {
        val token = auth.accessToken()
        return try {
            send(builder, token)
        } catch (e: CloverException) {
            if (e.status != 401) throw e
            val fresh = auth.refreshAfterRefusal(token)
            try {
                send(builder, fresh)
            } catch (again: CloverException) {
                if (again.status == 401) auth.signOut()
                throw again
            }
        }
    }

    private suspend fun send(builder: Request.Builder, token: String): String =
        withContext(Dispatchers.IO) {
            val request = builder
                .header("Authorization", "Bearer $token")
                .header("User-Agent", config.userAgent)
                .header("Accept", "application/json")
                .build()
            http.newCall(request).execute().use { response ->
                val text = response.body.string()
                when {
                    response.isSuccessful -> text
                    response.code == 401 -> throw CloverException(401, "Clover's sign-in has ended. Sign in again.", signInAgain = true)
                    response.code == 429 -> throw CloverException(
                        429,
                        "Clover asked the till to slow down.",
                        retryAfterSeconds = retryAfter(response.header("Retry-After")) ?: 1,
                    )
                    else -> throw CloverException(response.code, cloverError(text) ?: "Clover said ${response.code}.")
                }
            }
        }

    companion object {
        /** The system tender a REST-made one must take the key of. */
        const val EXTERNAL_PAYMENT = "com.clover.tender.external_payment"

        private const val PAGE = 100

        /** The most pages one look reads: 2,000 payments of one tender since the last look. */
        private const val MAX_PAGES = 20

        private val CURRENCY = Regex("[A-Z]{3}")
        private val COUNTRY = Regex("[A-Z]{2}")

        /** The address's country, else the locale's ("en_US"), when either is a country code. */
        internal fun country(address: String?, locale: String?): String? =
            address?.trim()?.uppercase(Locale.ROOT)?.takeIf { COUNTRY.matches(it) }
                ?: locale?.replace('-', '_')?.substringAfter('_', "")?.take(2)?.uppercase(Locale.ROOT)?.takeIf { COUNTRY.matches(it) }
    }
}
