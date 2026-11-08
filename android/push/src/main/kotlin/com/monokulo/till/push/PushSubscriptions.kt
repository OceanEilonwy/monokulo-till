package com.monokulo.till.push

import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.Serializable

/**
 * A Web Push subscription as the Bell takes it (`docs/bell.md`,
 * `POST /v1/register`): the distributor's endpoint and the public half of
 * this device's keys, base64url. The private keys stay with the connector
 * on this device (decision 92); nothing here can decrypt a ding.
 */
@Serializable
data class PushSubscription(
    val endpoint: String,
    /** The device's P-256 public key, uncompressed, base64url. */
    val p256dh: String,
    /** The 16-byte auth secret, base64url. */
    val auth: String,
)

/** Why a push subscription couldn't be made, or ended. */
enum class PushFailure {
    /** No UnifiedPush distributor (ntfy, NextPush, …) is installed to carry dings. */
    NoDistributor,
    /** The distributor couldn't reach its server: worth trying again when the network is back. */
    Network,
    /** The distributor needs the cashier to do something in it first (sign in again, say). */
    ActionRequired,
    /** The distributor gave an endpoint without Web Push keys, so the Bell couldn't encrypt for it. */
    NoKeys,
    /** The distributor ended the subscription: no more dings come to it. */
    Lost,
    /** Anything else the connector reported. */
    Other,
}

/** Where the device's push subscription stands. */
sealed interface PushState {
    /** Not subscribed. */
    data object None : PushState

    /** Asked the distributor; waiting for its endpoint. */
    data object Subscribing : PushState

    data class Subscribed(val subscription: PushSubscription) : PushState

    data class Failed(val reason: PushFailure) : PushState
}

/**
 * The device's Web Push subscription for the Bell, through the UnifiedPush
 * connector and whichever distributor is installed (`docs/design.md` §5).
 * One subscription at a time: for the Bell in use, made for its VAPID key,
 * so only that Bell can push to it.
 */
interface PushSubscriptions {
    val state: StateFlow<PushState>

    /**
     * Each ding the Bell sent, once the connector has decrypted it and it
     * reads as `{"wake":"<the till's point of sale>"}`. Nothing else about it: a ding carries
     * no sale. Dropped when nothing collects (no shift is running).
     */
    val dings: SharedFlow<Unit>

    /**
     * Subscribe for a sender whose VAPID public key is [vapidPublicKey]
     * (base64url, the Bell's `/v1/info`). The outcome comes through
     * [state]. Subscribing again with another key replaces the
     * subscription.
     */
    fun subscribe(vapidPublicKey: String)

    /** End the subscription with the distributor; [state] goes to [PushState.None]. */
    fun unsubscribe()
}
