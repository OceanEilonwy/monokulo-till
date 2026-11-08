package com.monokulo.till.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * The store, as `GET /api/v1/store` answers it now. Read each time it is
 * shown, never kept: the merchant can rename the store or move it to
 * another wallet at any time.
 */
@Serializable
data class Store(
    val name: String,
    @SerialName("base_currency") val baseCurrency: String,
    /** Where its payments go; null for a store without a wallet. */
    val wallet: Wallet? = null,
) {
    @Serializable
    data class Wallet(
        val name: String,
        /** `mainnet`, `stagenet` or `testnet`. */
        val network: String,
        /** The primary address, whole. */
        val address: String,
    ) {
        /** The address as the app shows it: its first and last 8 characters. */
        val addressShort: String get() = shorten(address)
    }

    companion object {
        /** [address]'s first and last 8 characters, or all of it when that's no shorter. */
        fun shorten(address: String): String =
            if (address.length <= 17) address else address.take(8) + "…" + address.takeLast(8)
    }
}

/** This device's connection as the store has it (`GET /api/v1/app`). */
@Serializable
data class AppConnection(
    @SerialName("app_id") val appId: String,
    /** What the merchant (or, when they gave none, the app) calls the device. */
    @SerialName("device_name") val deviceName: String,
    val platform: String = "",
    @SerialName("connected_at") val connectedAt: Long = 0,
)
