package com.monokulo.till.engine

import com.monokulo.till.TillApp
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage
import uniffi.till_app.Action

/**
 * The connector's service: the distributor binds to it for every endpoint,
 * push and failure of the Bell's instance, and each one goes to the engine
 * as the `Push…` action [UnifiedPushPlatform] answers with
 * (docs/decisions/core.md 44). The service returns at once (android.md 93):
 * it never starts the till's foreground service, which Android 14 refuses
 * from the background, and whether a push is a ding, and whether a shift is
 * on to hear it, is the engine's to say.
 */
class WakePushService : PushService() {
    private fun send(action: Action) {
        // The engine starts with the application; a process the distributor started for a push alone starts it here.
        (application as TillApp).engine
        EngineStore.send(action)
    }

    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        if (instance != UnifiedPushPlatform.INSTANCE) return
        send(Action.PushEndpoint(endpoint.url, endpoint.pubKeySet?.pubKey, endpoint.pubKeySet?.auth))
    }

    override fun onMessage(message: PushMessage, instance: String) {
        if (instance != UnifiedPushPlatform.INSTANCE) return
        send(Action.PushReceived(message.content, message.decrypted))
    }

    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        if (instance != UnifiedPushPlatform.INSTANCE) return
        send(Action.PushFailed(UnifiedPushPlatform.failure(reason)))
    }

    override fun onUnregistered(instance: String) {
        if (instance != UnifiedPushPlatform.INSTANCE) return
        send(Action.PushUnregistered)
    }
}
