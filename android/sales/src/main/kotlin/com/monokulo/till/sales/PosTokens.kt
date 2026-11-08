package com.monokulo.till.sales

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * The till's own sign-in to its point of sale: one shape for Square's and
 * Clover's, so the app keeps either the same way. The names are Square's
 * wire names, which stored sign-ins from before Clover were written with.
 */
@Serializable
data class PosTokens(
    @SerialName("access_token") val accessToken: String,
    /** RFC 3339. Clover's Unix-seconds expiry is converted to it at sign-in. */
    @SerialName("expires_at") val expiresAt: String,
    @SerialName("refresh_token") val refreshToken: String? = null,
    /** The merchant the sign-in is for: Square's token answer says it, Clover's redirect does. */
    @SerialName("merchant_id") val merchantId: String,
)

/** Where a sign-in is kept, encrypted (the app's `Keystore`-backed store): one per point of sale. */
interface TokenStore {
    fun load(): PosTokens?
    fun save(tokens: PosTokens?)
}
