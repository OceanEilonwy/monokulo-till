package com.monokulo.till.sales

/**
 * Which of Clover's platforms a merchant is on: each has its own sign-in
 * page and API host, and its own developer apps (decision 127).
 */
enum class CloverEnvironment(
    /** The name builds and the Bell's `/v1/info` use for it. */
    val wire: String,
    /** Where the merchant signs in. */
    val webBase: String,
    /** Tokens and `/v3`. */
    val apiBase: String,
) {
    Sandbox("sandbox", "https://sandbox.dev.clover.com", "https://apisandbox.dev.clover.com"),
    NorthAmerica("north_america", "https://www.clover.com", "https://api.clover.com"),
    Europe("europe", "https://www.eu.clover.com", "https://api.eu.clover.com"),
    LatinAmerica("latin_america", "https://www.la.clover.com", "https://api.la.clover.com"),
    ;

    companion object {
        fun fromWire(wire: String): CloverEnvironment? = entries.firstOrNull { it.wire == wire }
    }
}

/** Which Clover, and as which application (`docs/design.md` §1). */
data class CloverConfig(
    /** The Clover app's id (public: no secret is ever on the device). */
    val applicationId: String,
    val environment: CloverEnvironment,
    /** Where Clover sends the sign-in back: an App Link under the Clover app's Site URL. */
    val redirectUri: String,
    /** The label of the tender the cashier taps in Clover Register. */
    val tenderLabel: String = "Monero",
    /** Clover requires a `User-Agent` on every request. */
    val userAgent: String = "Till",
    /** A stand-in Clover's address (fake-clover, tests): both its sign-in page and its API. */
    private val baseUrl: String? = null,
) {
    val webBase: String get() = baseUrl ?: environment.webBase

    val apiBase: String get() = baseUrl ?: environment.apiBase
}
