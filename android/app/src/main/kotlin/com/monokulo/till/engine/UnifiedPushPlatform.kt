package com.monokulo.till.engine

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.ResolvedDistributor
import uniffi.till_app.Action
import uniffi.till_app.Distributors
import uniffi.till_app.PushFailure
import uniffi.till_app.PushPlatform

/**
 * The engine's [PushPlatform] over UnifiedPush's connector (docs/decisions/core.md
 * 44; android.md 90): one instance, [INSTANCE], for the Bell's VAPID key. The
 * engine calls one way; what the distributor answers comes back through
 * [WakePushService] as `Push…` actions, sent with [send]. An app installed or
 * removed may be a distributor coming or going (android.md 91), which is
 * [Action.DistributorsChanged]; whether a push is a ding is the engine's to say.
 */
class UnifiedPushPlatform(
    private val context: Context,
    private val send: (Action) -> Unit,
) : PushPlatform {
    init {
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_PACKAGE_ADDED)
            addAction(Intent.ACTION_PACKAGE_REMOVED)
            addDataScheme("package")
        }
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                // An app updated is removed and added again with EXTRA_REPLACING: nothing came or went,
                // and a look would have the connector end the subscription it holds (android.md 118).
                if (intent.getBooleanExtra(Intent.EXTRA_REPLACING, false)) return
                send(Action.DistributorsChanged)
            }
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            context.registerReceiver(receiver, filter)
        }
    }

    override fun distributors(): Distributors {
        val installed = UnifiedPush.getDistributors(context)
        UnifiedPush.getSavedDistributor(context)?.takeIf { it in installed }?.let { return Distributors.Found(it) }
        // The connector's choice without its dialog: the system's default, or the only one.
        return when (val resolved = UnifiedPush.resolveDefaultDistributor(context)) {
            is ResolvedDistributor.Found -> Distributors.Found(resolved.packageName).also { saveDistributor(resolved.packageName) }
            ResolvedDistributor.ToSelect -> if (installed.isEmpty()) Distributors.None else Distributors.Choose(installed)
            ResolvedDistributor.NoneAvailable -> Distributors.None
        }
    }

    override fun saveDistributor(name: String) = UnifiedPush.saveDistributor(context, name)

    override fun subscribe(vapidPublicKey: String) {
        // None saved (the connector forgot it, android.md 114): look again rather than fail with one installed.
        if (UnifiedPush.getSavedDistributor(context) == null && distributors() !is Distributors.Found) {
            send(Action.PushFailed(PushFailure.NO_DISTRIBUTOR))
            return
        }
        try {
            UnifiedPush.register(context, INSTANCE, null, vapidPublicKey)
        } catch (_: UnifiedPush.VapidNotValidException) {
            send(Action.PushFailed(PushFailure.OTHER))
        }
    }

    override fun unsubscribe() {
        // The connector forgets the saved distributor along with its last registration,
        // and "bell" is the only one: keep the distributor, the cashier's pick included (android.md 114).
        val distributor = UnifiedPush.getSavedDistributor(context)
        UnifiedPush.unregister(context, INSTANCE)
        distributor?.let(::saveDistributor)
    }

    companion object {
        /** The connector's instance for the Bell: there is only ever one. */
        const val INSTANCE = "bell"

        /** What a distributor's refusal means to the engine. */
        fun failure(reason: FailedReason): PushFailure = when (reason) {
            FailedReason.NETWORK -> PushFailure.NETWORK
            FailedReason.ACTION_REQUIRED -> PushFailure.ACTION_REQUIRED
            FailedReason.VAPID_REQUIRED, FailedReason.INTERNAL_ERROR -> PushFailure.OTHER
        }
    }
}
