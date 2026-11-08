package com.monokulo.till

import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.os.Build
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import com.monokulo.till.core.SalePhase
import com.monokulo.till.notify.Ongoing
import com.monokulo.till.notify.SaleNotifications
import com.monokulo.till.overlay.Overlay
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live
import com.monokulo.till.ui.Motion
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

/**
 * Keeps the till running during a shift (`docs/design.md` §3, §7): a
 * foreground service whose ongoing notification is the current sale, and
 * which holds the bubble over other apps. The notification follows the
 * newest sale whose code is up, turns into a green tick for a few seconds
 * once one is paid, and goes back to watching when none is up.
 */
class TillService : LifecycleService() {
    private val graph get() = (application as TillApp).graph
    private lateinit var notes: SaleNotifications
    private lateinit var overlay: Overlay

    override fun onCreate() {
        super.onCreate()
        notes = SaleNotifications(this).also { it.channels() }
        ServiceCompat.startForeground(
            this,
            SaleNotifications.ONGOING_ID,
            watching(),
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0,
        )
        graph.runner.start()
        overlay = Overlay(
            this, graph, lifecycleScope,
            onCopy = { SaleNotifications.copyAddress(this, it.address) },
            onCopyXmr = { SaleNotifications.copyXmr(this, it.xmr) },
            onFullScreen = ::openSale,
            onOpenShift = ::openShift,
            onOpenApp = ::openApp,
        )
        overlay.start()
        val changes = PhaseChanges()
        // The sale just paid, while the chip shows its tick.
        val justPaid = MutableStateFlow<String?>(null)
        var paidTimer: Job? = null
        var posted: List<Any?>? = null
        val alerted = mutableSetOf<String>()
        lifecycleScope.launch {
            combine(graph.runner.state, graph.permissions, graph.store.now, justPaid) { state, _, _, _ -> state }.collect { state ->
                val sales = state.till.sales
                for (sale in changes.next(sales)) {
                    when (sale.phase) {
                        SalePhase.Paid -> {
                            notes.paid(sale, quiet = graph.appVisible.value)
                            overlay.paid()
                            // The chip's tick starts at the same instant as the overlay's dwell, for the same time: a
                            // notification can't follow the panel's collapse, so it keeps its own timer.
                            justPaid.value = sale.orderId
                            paidTimer?.cancel()
                            paidTimer = lifecycleScope.launch {
                                delay(Motion.Dwell.PAID_MS)
                                justPaid.value = null
                            }
                        }
                        SalePhase.Expired, SalePhase.Cancelled, SalePhase.DoubleSpend -> notes.unpaid(sale, graph.stores.pointOfSale(sale), quiet = graph.appVisible.value)
                        else -> Unit
                    }
                }
                // Read now, not from the combine: a sale paid just above is the one to show.
                val ongoing = Ongoing.of(sales, justPaid.value)
                val sale = (ongoing as? Ongoing.Sale)?.sale ?: (ongoing as? Ongoing.Paid)?.sale
                val key = listOf(ongoing::class, sale?.orderId, sale?.phase, sale?.receipt != null, notes.allowed(), graph.store.now.value.store?.name)
                if (key == posted) return@collect
                posted = key
                when (ongoing) {
                    Ongoing.Watching -> notes.ongoing(watching())
                    is Ongoing.Paid -> notes.ongoing(notes.justPaid(ongoing.sale))
                    is Ongoing.Sale -> {
                        // A heads-up once per sale, when it first appears; not again when an older one comes back to the top.
                        val alert = ongoing.sale.phase == SalePhase.Waiting && alerted.add(ongoing.sale.orderId)
                        notes.ongoing(notes.sale(ongoing.sale, graph.stores.placeName(ongoing.sale), alert))
                    }
                }
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        if (intent?.action == ACTION_SHOW_CODE && !overlay.showCode()) {
            graph.runner.state.value.till.sales.firstOrNull { it.live }?.let(::openSale)
        }
        return START_STICKY
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        overlay.configurationChanged()
    }

    override fun onDestroy() {
        overlay.stop()
        graph.runner.stop()
        super.onDestroy()
    }

    /** Opens Till on the shift (the panel's header row): the sale stays live, on the bubble and in the list. */
    private fun openShift() {
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

    private fun watching() = notes.watching(graph.store.now.value.store?.name.orEmpty(), graph.stores.placeName)

    /** Opens Till full screen on [sale] (from the bubble's panel: an app showing an overlay may start activities). */
    private fun openSale(sale: TillSale) {
        startActivity(
            Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(MainActivity.EXTRA_ORDER, sale.orderId),
        )
    }

    companion object {
        const val ACTION_SHOW_CODE = "com.monokulo.till.SHOW_CODE"

        fun start(context: Context) =
            ContextCompat.startForegroundService(context, Intent(context, TillService::class.java))

        fun stop(context: Context) = context.stopService(Intent(context, TillService::class.java))
    }
}
