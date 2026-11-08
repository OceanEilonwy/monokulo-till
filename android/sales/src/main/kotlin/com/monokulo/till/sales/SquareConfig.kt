package com.monokulo.till.sales

/** Which Square, and as which application (`docs/design.md` §1). */
data class SquareConfig(
    /** The Square application's id (public: no secret is ever on the device). */
    val applicationId: String,
    val sandbox: Boolean,
    /** Where Square sends the sign-in back: an App Link that opens the app. */
    val redirectUri: String,
    /** The name of the Other tender the cashier taps. */
    val tenderName: String = "Monero",
    /** Square's address, for tests; otherwise production's or the sandbox's. */
    private val baseUrl: String? = null,
) {
    val connectBase: String
        get() = baseUrl ?: if (sandbox) "https://connect.squareupsandbox.com" else "https://connect.squareup.com"

    /** The environment [applicationId] is in, as a Bell spells it: `sandbox` or `production` (decision 147). */
    val environment: String
        get() = environmentOf(applicationId)

    companion object {
        /** Square's sandbox applications' ids start `sandbox-`; every other id is production's. */
        fun environmentOf(applicationId: String): String = if (applicationId.startsWith("sandbox-")) "sandbox" else "production"

        /** The API version every request asks for. */
        const val API_VERSION = "2026-09-16"

        val SCOPES = listOf("PAYMENTS_READ", "ORDERS_READ", "MERCHANT_PROFILE_READ", "PAYMENTS_WRITE")
    }
}
