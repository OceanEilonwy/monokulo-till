package com.monokulo.till

import android.app.ForegroundServiceStartNotAllowedException
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.os.Build
import android.util.Log
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import com.monokulo.till.engine.EngineStore
import com.monokulo.till.engine.TorProbe
import com.monokulo.till.notify.SaleNotifications
import com.monokulo.till.overlay.Overlay
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import uniffi.till_app.Action
import uniffi.till_app.Effect

/**
 * Keeps the till running during a shift (`docs/design.md` §3, §7): a
 * foreground service, up while the engine says the till runs
 * ([uniffi.till_app.Update.Service]), whose ongoing notification is the
 * engine's ongoing face, and which holds the bubble over other apps. The
 * shift itself runs in the engine; the service only keeps the process in
 * front of Android's eye while it does.
 */
class TillService : LifecycleService() {
    private val app get() = application as TillApp
    private lateinit var notes: SaleNotifications
    private lateinit var overlay: Overlay

    override fun onCreate() {
        super.onCreate()
        notes = SaleNotifications(this).also { it.channels() }
        ServiceCompat.startForeground(
            this,
            SaleNotifications.ONGOING_ID,
            notes.watching(),
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0,
        )
        overlay = Overlay(this, lifecycleScope, onOpenShift = ::openShift, onOpenApp = ::openApp, onFullScreen = ::openSale)
        overlay.start()
        lifecycleScope.launch {
            app.engineStore.notification.map { it?.ongoing }.distinctUntilChanged().collect { face ->
                // A sale's heads-up is its face's alert, set in the one update as it arrives while Till is away.
                if (face != null) notes.ongoing(notes.ongoing(face))
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        if (intent?.action == ACTION_SHOW_CODE && !overlay.showCode()) {
            intent.getStringExtra(MainActivity.EXTRA_ORDER)?.let(::openSale)
        }
        return START_STICKY
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        overlay.configurationChanged()
    }

    override fun onDestroy() {
        overlay.stop()
        super.onDestroy()
    }

    /** Opens Till on the shift (the panel's header row): the sale stays live, on the bubble and in the list. */
    private fun openShift() {
        EngineStore.send(Action.OpenShift)
        startActivity(
            Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(MainActivity.EXTRA_SHIFT, true),
        )
    }

    /**
     * Brings Till to the front as its launcher icon does (the idle
     * bubble): the task it was in, on the screen it was on, or a fresh one.
     * Allowed from here because the tap is on the overlay's own visible
     * window, as [openShift] and [openSale] are.
     */
    private fun openApp() {
        startActivity(
            Intent(this, MainActivity::class.java)
                .setAction(Intent.ACTION_MAIN)
                .addCategory(Intent.CATEGORY_LAUNCHER)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED),
        )
    }

    /** Opens Till full screen on [orderId]'s sale (from the bubble's panel: an app showing an overlay may start activities). */
    private fun openSale(orderId: String) {
        EngineStore.send(Action.OpenSale(orderId))
        startActivity(
            Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(MainActivity.EXTRA_ORDER, orderId),
        )
    }

    companion object {
        const val ACTION_SHOW_CODE = "com.monokulo.till.SHOW_CODE"
        private const val TAG = "TillService"

        fun start(context: Context) =
            ContextCompat.startForegroundService(context, Intent(context, TillService::class.java))

        fun stop(context: Context) = context.stopService(Intent(context, TillService::class.java))

        /**
         * Carries out the engine's Android side for the life of the process
         * (call once, after the engine is made): the service up while the
         * till runs ([uniffi.till_app.Update.Service]), a sale's notices
         * ([Effect.Notify], [Effect.ClearNotification]) posted and cleared,
         * and every copy ([Effect.CopyText], by what it copies), from the
         * app, the panel or a notification alike.
         */
        fun follow(app: TillApp) {
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
            val notes = SaleNotifications(app).also { it.channels() }
            scope.launch {
                app.engineStore.service.collect { running ->
                    if (running) {
                        // Android 12+ refuses a foreground service started from the background: the app's next
                        // Opened has the engine publish again, which starts it then.
                        try {
                            start(app)
                        } catch (e: IllegalStateException) {
                            if (Build.VERSION.SDK_INT >= 31 && e is ForegroundServiceStartNotAllowedException) {
                                Log.w(TAG, "the till's service can't start from the background", e)
                            } else {
                                throw e
                            }
                        }
                    } else {
                        stop(app)
                    }
                }
            }
            scope.launch {
                app.engineStore.effects.collect { effect ->
                    when (effect) {
                        is Effect.Notify -> notes.notice(effect.orderId, effect.notice, effect.paid, effect.timeoutMs, effect.quiet)
                        is Effect.ClearNotification -> notes.clear(effect.id)
                        is Effect.CopyText -> SaleNotifications.copyText(app, effect.text, effect.kind)
                        // Orbot's probe, as the app comes forward and as step 3 or Settings opens (docs/decisions/core.md 102).
                        is Effect.ProbeTor -> launch { TorProbe.probe(app.engineStore::send) }
                        else -> Unit
                    }
                }
            }
        }
    }
}
