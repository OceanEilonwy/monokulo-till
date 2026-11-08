package com.monokulo.till.push

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import com.monokulo.till.core.PointOfSale
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.ResolvedDistributor

/**
 * The till's push subscription for its Bell, through UnifiedPush's
 * connector (decision 90): one instance, [INSTANCE], made for the Bell's
 * VAPID key. The connector keeps the keys and decrypts each push before
 * [WakePushService] hands it here (decision 92); only a ding for the
 * till's point of sale, [pointOfSale] as it is when the ding arrives, gets
 * through to [dings] (decision 133). Held by the app's graph for the life of the process.
 */
class UnifiedPushSubscriptions(
    private val context: Context,
    private val pointOfSale: () -> PointOfSale?,
) : PushSubscriptions, Distributors {
    private val mutableState = MutableStateFlow<PushState>(PushState.None)
    override val state: StateFlow<PushState> = mutableState.asStateFlow()

    // No replay: a ding no shift is collecting is gone (decision 93).
    private val mutableDings = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val dings: SharedFlow<Unit> = mutableDings.asSharedFlow()

    private val mutableChanges = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val changes: SharedFlow<Unit> = mutableChanges.asSharedFlow()

    init {
        // An app installed or removed may be a distributor coming or going
        // (decision 91): the connector has no callback for a distributor uninstalled.
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_PACKAGE_ADDED)
            addAction(Intent.ACTION_PACKAGE_REMOVED)
            addDataScheme("package")
        }
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                // An app updated is removed and added again with EXTRA_REPLACING: nothing came or went,
                // and a look would have the connector end the subscription it holds (decision 118).
                if (intent.getBooleanExtra(Intent.EXTRA_REPLACING, false)) return
                mutableChanges.tryEmit(Unit)
            }
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            context.registerReceiver(receiver, filter)
        }
    }

    override fun resolve(): Distributor {
        val installed = UnifiedPush.getDistributors(context)
        UnifiedPush.getSavedDistributor(context)?.takeIf { it in installed }?.let { return Distributor.Found(it) }
        // The connector's choice without its dialog: the system's default, or the only one.
        return when (val resolved = UnifiedPush.resolveDefaultDistributor(context)) {
            is ResolvedDistributor.Found -> Distributor.Found(resolved.packageName).also { save(resolved.packageName) }
            ResolvedDistributor.ToSelect -> if (installed.isEmpty()) Distributor.None else Distributor.Choose(installed)
            ResolvedDistributor.NoneAvailable -> Distributor.None
        }
    }

    override fun save(packageName: String) = UnifiedPush.saveDistributor(context, packageName)

    override fun subscribe(vapidPublicKey: String) {
        // None saved (the connector forgot it, decision 114): look again rather than fail with one installed.
        if (UnifiedPush.getSavedDistributor(context) == null && resolve() !is Distributor.Found) {
            mutableState.value = PushState.Failed(PushFailure.NoDistributor)
            return
        }
        mutableState.value = PushState.Subscribing
        try {
            UnifiedPush.register(context, INSTANCE, null, vapidPublicKey)
        } catch (_: UnifiedPush.VapidNotValidException) {
            mutableState.value = PushState.Failed(PushFailure.Other)
        }
    }

    override fun unsubscribe() {
        mutableState.value = PushState.None
        // The connector forgets the saved distributor along with its last registration,
        // and "bell" is the only one: keep the distributor, the cashier's pick included (decision 114).
        val distributor = UnifiedPush.getSavedDistributor(context)
        UnifiedPush.unregister(context, INSTANCE)
        distributor?.let(::save)
    }

    /** [WakePushService]: the distributor's endpoint, with the subscription's public keys when it gave them. */
    internal fun endpoint(url: String, p256dh: String?, auth: String?) {
        mutableState.value = PushEvents.endpoint(url, p256dh, auth)
    }

    /** [WakePushService]: a push, decrypted or not. */
    internal fun message(content: ByteArray, decrypted: Boolean) {
        if (PushEvents.isDing(content, decrypted, pointOfSale())) mutableDings.tryEmit(Unit)
    }

    /** [WakePushService]: the distributor couldn't make the subscription. */
    internal fun failed(reason: PushFailure) {
        mutableState.value = PushState.Failed(reason)
    }

    /** [WakePushService]: the distributor ended the subscription. One the app ended itself is already [PushState.None]. */
    internal fun unregistered() {
        if (mutableState.value != PushState.None) mutableState.value = PushState.Failed(PushFailure.Lost)
    }

    companion object {
        /** The connector's instance for the Bell: there is only ever one. */
        const val INSTANCE = "bell"
    }
}

/** What the connector's callbacks mean, apart from Android: tested on the JVM. */
internal object PushEvents {
    private val json = Json

    /** An endpoint without both keys is one the Bell can't encrypt for. */
    fun endpoint(url: String, p256dh: String?, auth: String?): PushState =
        if (p256dh.isNullOrEmpty() || auth.isNullOrEmpty()) {
            PushState.Failed(PushFailure.NoKeys)
        } else {
            PushState.Subscribed(PushSubscription(url, p256dh, auth))
        }

    /**
     * A ding: decrypted, and exactly the JSON object `{"wake":"<pointOfSale>"}` for the
     * till's point of sale (decision 92, 133). A Bell rings a till only for the point of
     * sale it registered, so a ding for another is stale or forged; none is a ding without one.
     */
    fun isDing(content: ByteArray, decrypted: Boolean, pointOfSale: PointOfSale?): Boolean {
        if (!decrypted || pointOfSale == null) return false
        val element = runCatching { json.parseToJsonElement(content.toString(Charsets.UTF_8)) }.getOrNull()
        if (element !is JsonObject || element.keys != setOf("wake")) return false
        val wake = element["wake"] as? JsonPrimitive ?: return false
        return wake.isString && wake.content == pointOfSale.wire
    }
}
