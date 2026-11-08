package com.monokulo.till

import com.monokulo.till.core.Connection
import com.monokulo.till.core.MonokuloException
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.Problems
import com.monokulo.till.core.StreamEvent
import com.monokulo.till.push.interval
import com.monokulo.till.sales.CloverSource
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.PosException
import com.monokulo.till.sales.Refunds
import com.monokulo.till.sales.SaleOrder
import com.monokulo.till.sales.SaleSource
import com.monokulo.till.sales.Shift
import com.monokulo.till.sales.Standing
import com.monokulo.till.sales.standing
import com.monokulo.till.sales.SquareSource
import com.monokulo.till.sales.Till
import com.monokulo.till.sales.TillState
import com.monokulo.till.sales.WakeCause
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlin.time.Duration.Companion.seconds

/** The till as the screens and the service see it. */
data class RunnerState(
    val running: Boolean = false,
    /** The store's event stream is open. */
    val live: Boolean = false,
    /** The store disconnected this device: its key no longer works. */
    val disconnected: Boolean = false,
    /** What the event stream's last failure said, while it's closed: `too-many-streams`, say. */
    val streamProblem: String? = null,
    /**
     * The point of sale the till's source watches ([SaleSource.pointOfSale]):
     * with [Closing.refundsThere] and a sale's own, whether its "No" is
     * refunded by the till or by the cashier there (decision 130).
     */
    val source: PointOfSale = PointOfSale.Square,
    val till: TillState = TillState(),
)

/**
 * A source for a till that runs without a point of sale signed in: it shows
 * other tills' sales only. A Square sale's refund fails rather than marking
 * the sale refunded with nothing sent to Square (decision 143); any other
 * point of sale's is marked to refund there, as `Till` does for a sale its
 * source didn't take.
 */
internal object NoSource : SaleSource {
    override val pointOfSale = PointOfSale.Square
    override suspend fun newSales() = emptyList<FoundSale>()
    override suspend fun order(sale: FoundSale) = SaleOrder(null)
    override val refunds = Refunds.ByTill { throw PosException(0, "Sign in to Square first") }
    override fun handled(sale: FoundSale) = Unit
}

/**
 * Runs the till while a shift is on (`docs/design.md` §3): the POS checked
 * on a timer and whenever woken, and the store's event stream followed,
 * reconnecting with a growing wait. Held by the app; [TillService] keeps
 * the process alive while it runs.
 */
class TillRunner(private val graph: Graph) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutable = MutableStateFlow(RunnerState())
    val state: StateFlow<RunnerState> = mutable.asStateFlow()
    private var job: Job? = null
    private var till: Till? = null
    private var shift: Shift? = null
    /** Copies [till]'s state into [state]: it outlives [job], so a stopped till's shift still shows and changes. */
    private var showing: Job? = null

    fun start() {
        if (job?.isActive == true) return
        val connection = graph.stores.connection ?: return
        graph.store.read(connection)
        // The first run, in setup (or after an update from before shifts were saved), starts the first shift.
        if (graph.stores.shiftStartedAt == null) graph.stores.shiftStartedAt = System.currentTimeMillis() / 1000
        val till = newTill(connection).also(::show)
        val wakeUps = graph.wakeUps
        val shift = Shift(
            till,
            // 30 s while the Bell is connected, else 2 s; read on every tick (decision 94).
            interval = { wakeUps.status.value.interval },
            // Only the timer's finds tell whether the Bell missed a sale: a ding's or the app's don't.
            onChecked = { cause, found -> if (cause == WakeCause.Timer) wakeUps.timerFound(found) },
        ).also { shift = it }
        mutable.update { it.copy(running = true, live = false, disconnected = false, streamProblem = null) }
        job = scope.launch {
            launch { shift.run() }
            launch { follow(till) }
            // The Bell's dings, while this shift runs; outside one nothing collects them, so they go nowhere (decision 93).
            launch { graph.push.dings.collect { shift.wake(WakeCause.Ding) } }
            launch { till.lapseOnTime() }
            launch { till.followConfirmations() }
        }
        // A shift starting registers again, so a registration the Bell dropped comes back (decision 95).
        wakeUps.recheck()
    }

    /**
     * While the till isn't watching: the saved shift on screen, so its
     * sales can still be opened and refunded (decision 86). The app calls
     * it as it starts with watching off.
     */
    fun showSaved() {
        if (till != null) return
        val connection = graph.stores.connection ?: return
        show(newTill(connection))
    }

    /** The till for the point of sale the cashier chose and signed in to, or one that only shows other tills' sales. */
    private fun newTill(connection: Connection): Till {
        val stores = graph.stores
        val location = stores.location
        val merchant = stores.cloverMerchant
        val source: SaleSource = when (stores.pointOfSale) {
            PointOfSale.Square -> if (location != null && graph.squareAuth().signedIn) {
                SquareSource(graph.squareApi(), graph.squareConfig(), location, stores.cursor)
            } else {
                NoSource
            }
            // The phone isn't the Clover device, so every sale at the merchant is the till's (no device filter).
            PointOfSale.Clover -> if (merchant != null && graph.cloverAuth().signedIn) graph.cloverSource(merchant) else NoSource
            null -> NoSource
        }
        mutable.update { it.copy(source = source.pointOfSale) }
        return Till(source, graph.monokulo, connection, store = stores.shift)
    }

    private fun show(till: Till) {
        this.till = till
        showing?.cancel()
        showing = scope.launch { till.sales.collect { s -> mutable.update { it.copy(till = s) } } }
    }

    fun stop() {
        job?.cancel()
        job = null
        mutable.update { it.copy(running = false, live = false) }
    }

    /**
     * The cashier stopped watching: every live sale's code comes down
     * ([Till.closeLive]). Done here and now, not launched, so the closes
     * are saved before the service stops and before the next shift reads
     * what the last one left to do.
     */
    fun closeLive() {
        till?.closeLive()
    }

    /** The store disconnected this device (it answered 401): stop asking. */
    fun disconnected() {
        mutable.update { it.copy(live = false, disconnected = true) }
        job?.cancel()
    }

    /** Check the POS now (the app came to the front). */
    fun wake() = shift?.wake(WakeCause.Opened)

    /** Refunds the sale's POS sale; [close] it first (the close sheet's "No"). The job ends when the POS has answered. */
    fun refund(orderId: String, close: Boolean = false): Job? {
        val till = till ?: return null
        return scope.launch {
            if (close) till.close(orderId)
            till.refund(orderId)
        }
    }

    /** Where [orderId] stands in the till itself, ahead of [state]'s copy. */
    fun standing(orderId: String): Standing? = till?.sales?.value?.sales?.firstOrNull { it.orderId == orderId }?.standing

    /** The refund sheet's "I've refunded it": whether the point of sale now shows the sale refunded (decision 130). */
    suspend fun confirmRefund(orderId: String): Boolean = till?.confirmRefund(orderId) ?: false

    /** Closes the sale for now (the close sheet's "Not sure"): its code comes down, it stays in the shift. */
    fun close(orderId: String) {
        val till = till ?: return
        scope.launch { till.close(orderId) }
    }

    /** "Show the code again" on a closed sale. */
    fun reopen(orderId: String) {
        val till = till ?: return
        scope.launch { till.reopen(orderId) }
    }

    private suspend fun follow(till: Till) {
        val connection = graph.stores.connection ?: return
        var wait = 1.seconds
        while (true) {
            try {
                graph.monokulo.events(connection).collect { event ->
                    when (event) {
                        StreamEvent.Ready, StreamEvent.Lagged -> {
                            wait = 1.seconds
                            mutable.update { it.copy(live = true, streamProblem = null) }
                            resync(till)
                        }
                        is StreamEvent.Sale -> till.onEvent(event.event)
                    }
                }
            } catch (e: MonokuloException) {
                if (e.status == 401) {
                    disconnected()
                    return
                }
                if (e.problem != null) mutable.update { it.copy(streamProblem = Problems.say(e)) }
            } catch (_: Exception) {
            }
            mutable.update { it.copy(live = false) }
            delay(wait)
            wait = (wait * 2).coerceAtMost(30.seconds)
        }
    }

    /** Re-reads the sales on screen, after the stream (re)opens. */
    private suspend fun resync(till: Till) {
        val connection = graph.stores.connection ?: return
        // An expired sale too, until Monokulo stops watching it for a late payment.
        for (sale in till.sales.value.sales.filter { it.open }) {
            val url = sale.url ?: continue
            runCatching { graph.monokulo.order(connection, url) }
                .onSuccess(till::onStatus)
        }
    }
}
