package com.monokulo.till.push

import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

/**
 * The connector's service: the distributor binds to it for every endpoint,
 * push and failure. Each one is handed to the app's
 * [UnifiedPushSubscriptions] and the service returns at once (decision 93):
 * it never starts the till's foreground service, which Android 14 refuses
 * from the background, and a ding no shift is collecting is dropped there.
 */
class WakePushService : PushService() {
    /** The app holds the one [UnifiedPushSubscriptions] (its graph's); this module can't see the app's class. */
    interface Host {
        val pushSubscriptions: UnifiedPushSubscriptions
    }

    private val subscriptions get() = (application as Host).pushSubscriptions

    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        if (instance != UnifiedPushSubscriptions.INSTANCE) return
        subscriptions.endpoint(endpoint.url, endpoint.pubKeySet?.pubKey, endpoint.pubKeySet?.auth)
    }

    override fun onMessage(message: PushMessage, instance: String) {
        if (instance != UnifiedPushSubscriptions.INSTANCE) return
        subscriptions.message(message.content, message.decrypted)
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        if (instance != UnifiedPushSubscriptions.INSTANCE) return
        subscriptions.failed(
            when (reason) {
                FailedReason.NETWORK -> PushFailure.Network
                FailedReason.ACTION_REQUIRED -> PushFailure.ActionRequired
                FailedReason.VAPID_REQUIRED, FailedReason.INTERNAL_ERROR -> PushFailure.Other
            },
        )
    }

    override fun onUnregistered(instance: String) {
        if (instance != UnifiedPushSubscriptions.INSTANCE) return
        subscriptions.unregistered()
    }
}
