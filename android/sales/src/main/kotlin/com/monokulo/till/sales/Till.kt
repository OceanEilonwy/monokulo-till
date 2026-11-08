package com.monokulo.till.sales

import com.monokulo.till.core.Connection
import com.monokulo.till.core.MonokuloApi
import com.monokulo.till.core.MonokuloException
import com.monokulo.till.core.Order
import com.monokulo.till.core.PaymentUri
import com.monokulo.till.core.Problems
import com.monokulo.till.core.SaleEvent
import com.monokulo.till.core.SalePhase
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.receipt.ReceiptCode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.Serializable
import java.security.MessageDigest
import java.time.Instant

/** A sale on the till's screen, and in its saved shift ([ShiftStore]). */
@Serializable
data class TillSale(
    val orderId: String,
    val amount: String,
    val currency: String,
    val reference: String?,
    /** What to show as the QR code: with the receipt when this till found the sale and could add it. */
    val code: PaymentCode,
    val phase: SalePhase,
    /** Unix seconds. */
    val expiresAt: Long,
    val xmr: String,
    /** The address the customer pays: what "Copy address" copies. */
    val address: String,
    /** The POS sale this till found, when it found it: what a refund needs. */
    val found: FoundSale? = null,
    /**
     * What was bought, as the receipt this till built from the POS's order
     * (`docs/design.md` §4). It is shown on this till's own screens and
     * never leaves the device except inside [code]: never sent to Monokulo,
     * never logged. Null for another till's sale, or when the order couldn't
     * be read.
     */
    val receipt: Receipt? = null,
    /**
     * Whom the sale is for, when the POS's order names them
     * ([SaleOrder.customerName]): the sale page's title. Kept as [receipt]
     * is, on this till's own screens only: never in the code, never sent to
     * Monokulo, never logged, never on a notification.
     */
    val customerName: String? = null,
    /** The device that made the order, for a sale another till found. */
    val deviceName: String? = null,
    /** Where to read the order again: its `self` link, or its sale event's `mk:order`. */
    val url: String? = null,
    /**
     * Monokulo said nothing more will happen to the order (`is_final`). An
     * expired or cancelled order isn't, while Monokulo still watches it for
     * a late payment that turns it paid.
     */
    val settled: Boolean = false,
    /** When it was rung up, Unix seconds: the POS's time for this till's own, the event's for another's. */
    val createdAt: Long = 0,
    /** When its payment was first seen on the network, if it was. */
    val seenAt: Long? = null,
    val paidAt: Long? = null,
    /** When it ended unpaid: the code ran out, the order was cancelled or the payment reversed. */
    val endedAt: Long? = null,
    /** When the cashier closed it on this till (the close sheet's "Not sure" or "No"), if they did. */
    val closedAt: Long? = null,
    /**
     * When it was refunded in the POS: by this till through the POS's API
     * ([Refunds.ByTill]), or when the till saw the cashier had done it there
     * ([Refunds.InPos]).
     */
    val refundedAt: Long? = null,
    /**
     * When the cashier was told to refund it in the POS themselves
     * ([Refunds.InPos]): until [refundedAt], and only while the sale's point
     * of sale ([FoundSale.pointOfSale]) is the till's, the till looks for
     * the refund.
     */
    val refundPendingAt: Long? = null,
    /**
     * How many confirmations its payment has, of the [confirmationsRequired]
     * the store asks for its amount: what a seen or confirming sale's ring
     * grows by and its "Confirming · 4 of 10" counts. Null until Monokulo
     * says, when the ring fills by step instead.
     */
    val confirmations: Int? = null,
    val confirmationsRequired: Int? = null,
) {
    /** Worth reading again: Monokulo may still move it on. */
    val open: Boolean get() = !settled && phase != SalePhase.Paid

    /** Its payment is on the network and not yet paid: its confirmations are worth following ([Till.followConfirmations]). */
    val confirming: Boolean get() = open && (phase == SalePhase.Seen || phase == SalePhase.Confirming)

    val refunded: Boolean get() = refundedAt != null
}

/** What the till shows: its sales, newest first, and the last thing that went wrong. */
data class TillState(
    val sales: List<TillSale> = emptyList(),
    val problem: String? = null,
    /** The POS sign-in has ended: the cashier must sign in again. */
    val signInAgain: Boolean = false,
    /**
     * Why the POS refused a refund, by order: shown on that sale's page
     * beside its Refund button until it's tried again, which the next
     * check, clearing [problem], doesn't undo.
     */
    val refundProblems: Map<String, String> = emptyMap(),
)

/**
 * Where the till keeps this shift's sales between runs (decision 64): the
 * app backs it with its encrypted store, since a sale's receipt says what
 * was bought.
 */
interface ShiftStore {
    fun load(): List<TillSale>

    fun save(sales: List<TillSale>)
}

/**
 * The till (`docs/design.md` §1): turns the POS's Monero sales into
 * Monokulo orders, puts receipts in their codes, and follows them to paid
 * or expired. A sale it finds is handled once it has an order; until then
 * every [check] tries again, and the idempotency key makes a retry, or
 * another till finding the same sale, get the same order.
 */
class Till(
    private val source: SaleSource,
    private val monokulo: MonokuloApi,
    private val connection: Connection,
    /** Only this device's sales, when the POS says which device took each. */
    private val deviceId: String? = null,
    /** The time, Unix seconds: what the cashier's marks and a resync's are stamped with. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    /** This shift's sales as saved: loaded now, saved again on every change. */
    private val store: ShiftStore? = null,
) {
    private val state = MutableStateFlow(TillState(sales = store?.load().orEmpty().take(MAX_SALES)))
    val sales: StateFlow<TillState> = state.asStateFlow()
    private val checking = Mutex()
    /** The sales the last [check] found, by id: what the next one's count leaves out. */
    private var lastFound: Set<String> = emptySet()
    private val saving = Any()
    /** When the timer last asked the POS about pending refunds ([REFUND_LOOK_SECONDS]). */
    private var lastRefundLookAt: Long? = null

    /**
     * Changes the till's state, keeping at most [MAX_SALES] sales, and saves
     * the sales when they changed. The save writes the state as it is then,
     * not as this change left it, so two changes racing still leave the
     * latest on disk.
     */
    private fun change(block: (TillState) -> TillState) {
        var moved = false
        state.update { old ->
            val new = block(old).let { if (it.sales.size > MAX_SALES) it.copy(sales = it.sales.take(MAX_SALES)) else it }
            moved = new.sales != old.sales
            new
        }
        if (moved && store != null) synchronized(saving) { store.save(state.value.sales) }
    }

    /**
     * Looks for new sales; makes each an order with its code. Answers how
     * many of the POS's sales this check found that the last one hadn't,
     * another device's included (the Bell rings for every sale at the
     * location): a sale whose order failed, found again, isn't counted
     * twice. 0 when the POS couldn't be read.
     */
    suspend fun check(): Int = checking.withLock {
        val found = try {
            source.newSales()
        } catch (e: PosException) {
            change { it.withPosProblem(e) }
            return@withLock 0
        } catch (e: Exception) {
            change { it.copy(problem = "Couldn't reach the POS: ${e.message}") }
            return@withLock 0
        }
        val fresh = found.count { it.id !in lastFound }
        lastFound = found.mapTo(HashSet()) { it.id }
        var problem: String? = null
        for (sale in found) {
            if (deviceId != null && sale.deviceId != null && sale.deviceId != deviceId) {
                source.handled(sale)
                continue
            }
            val order = try {
                monokulo.createOrder(
                    connection,
                    amount = decimalAmount(sale.amountMinor, sale.currency),
                    currency = sale.currency,
                    merchantOrderId = sale.reference,
                    idempotencyKey = idempotencyKey(sale),
                )
            } catch (e: Exception) {
                problem = creationProblem(e)
                continue
            }
            source.handled(sale)
            val read = runCatching { source.order(sale) }.getOrNull()
            add(order, ReceiptCode.of(order.paymentUri, read?.receipt), read, sale)
        }
        change { it.copy(problem = problem, signInAgain = false) }
        lookForRefunds()
        fresh
    }

    /**
     * Asks the POS whether the sales the cashier was told to refund there
     * are refunded now, at most every [REFUND_LOOK_SECONDS]: a check runs
     * every 2 s without a Bell, and each pending sale costs a request. The
     * sheet's "I've refunded it" ([confirmRefund]) asks at once, so the
     * cashier never waits on this. A failure waits for the next look. Only
     * the sales this source's point of sale took are asked about.
     */
    private suspend fun lookForRefunds() {
        val refunds = source.refunds as? Refunds.InPos ?: return
        val pending = state.value.sales.filter {
            it.refundPendingAt != null && it.refundedAt == null && it.found?.pointOfSale == source.pointOfSale
        }
        if (pending.isEmpty()) return
        val at = now()
        if (lastRefundLookAt?.let { at - it < REFUND_LOOK_SECONDS } == true) return
        lastRefundLookAt = at
        for (sale in pending) {
            if (seenRefunded(refunds, sale.found ?: continue)) markRefunded(sale.orderId)
        }
    }

    /** Whether the POS shows [found] refunded; false when it couldn't be asked. */
    private suspend fun seenRefunded(refunds: Refunds.InPos, found: FoundSale): Boolean = try {
        refunds.refunded(found)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        false
    }

    /**
     * [e] shown, unless the sign-in has already ended and [e] only says so
     * again: the words that said why it ended ("Clover's sign-in has ended")
     * stay until the cashier signs in, rather than the next check's bare
     * "Sign in to Clover."
     */
    private fun TillState.withPosProblem(e: PosException): TillState =
        if (signInAgain && e.signInAgain) this else copy(problem = e.message, signInAgain = e.signInAgain)

    private fun markRefunded(orderId: String) {
        val at = now()
        change { s -> s.copy(sales = s.sales.map { if (it.orderId == orderId) it.copy(refundedAt = it.refundedAt ?: at) else it }) }
    }

    /**
     * What the till says when an order couldn't be made: what Monokulo's
     * problem means and who can fix it ([Problems]). No order was made, so
     * the sale stays unhandled and the next check asks again with the same
     * idempotency key.
     */
    private fun creationProblem(e: Exception): String =
        if (e is MonokuloException) Problems.say(e) else "Couldn't make the payment code: ${e.message}"

    /** What the store's event stream said. */
    fun onEvent(event: SaleEvent) {
        val phase = event.phase ?: return
        change { s ->
            if (s.sales.any { it.orderId == event.orderId }) {
                s.copy(sales = s.sales.map { if (it.orderId == event.orderId) it.moved(phase, event.at).copy(url = it.url ?: event.orderUrl) else it })
            } else if (phase == SalePhase.Waiting) {
                // Another till's sale: shown with the plain code (no receipt here).
                val uri = PaymentUri.build(event.address, event.piconero)
                s.copy(sales = listOf(event.toTillSale(PaymentCode(uri, hasReceipt = false))) + s.sales)
            } else {
                s
            }
        }
    }

    /**
     * An order's phase, from its status, whether it's [TillSale.settled], and
     * its confirmations of those it needs when Monokulo said (a read of the
     * order: after the stream reconnects, or [followConfirmations]).
     */
    fun onStatus(orderId: String, phase: SalePhase, settled: Boolean = false, confirmations: Int? = null, required: Int? = null) {
        val at = now()
        change { s -> s.copy(sales = s.sales.map { if (it.orderId == orderId) it.read(phase, settled, confirmations, required, at) else it }) }
    }

    /** [order], read again: its phase, whether it's settled and its confirmations. */
    fun onStatus(order: Order) = onStatus(order.orderId, order.phase, order.isFinal, order.confirmations, order.confirmationsRequired)

    /**
     * A [followConfirmations] read of [order], kept only while its sale is
     * still [TillSale.confirming]: a read sent before `sale.paid` that lands
     * after it would take a paid sale back to confirming, and its paid note
     * would sound again when the next read closed it.
     */
    internal fun onConfirmations(order: Order) {
        val at = now()
        change { s ->
            s.copy(
                sales = s.sales.map {
                    if (it.orderId == order.orderId && it.confirming) {
                        it.read(order.phase, order.isFinal, order.confirmations, order.confirmationsRequired, at)
                    } else {
                        it
                    }
                },
            )
        }
    }

    /** The sale as a read of its order left it: moved to [phase], and its count where the read gave one. */
    private fun TillSale.read(phase: SalePhase, settled: Boolean, confirmations: Int?, required: Int?, at: Long): TillSale {
        val moved = moved(phase, at)
        return moved.copy(
            settled = settled,
            confirmations = confirmations ?: moved.confirmations,
            confirmationsRequired = required ?: moved.confirmationsRequired,
        )
    }

    /**
     * Reads every sale whose payment is seen or confirming ([TillSale.confirming])
     * at once and then each [CONFIRMATIONS_READ_MS], so its ring and its "4 of
     * 10" follow the blocks: the store's event stream says when a payment is
     * seen and when it's first in a block, never each confirmation after. One
     * wait, started again whenever which sales those are changes (a sale paid,
     * a new one seen); nothing is read while there are none. A read that fails
     * waits for the next.
     */
    suspend fun followConfirmations() {
        sales.map { s -> s.sales.mapNotNull { sale -> sale.url?.takeIf { sale.confirming } } }.distinctUntilChanged().collectLatest { urls ->
            while (urls.isNotEmpty()) {
                for (url in urls) {
                    try {
                        onConfirmations(monokulo.order(connection, url))
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: Exception) {
                    }
                }
                delay(CONFIRMATIONS_READ_MS)
            }
        }
    }

    /**
     * The sale moved on to [to], the time of its first step there kept: seen when
     * its payment first showed, paid, or ended unpaid. A step already timed
     * keeps its time (a resync says what, not when). Seen, its payment has no
     * confirmation yet; confirming, at least the one block it's in, until a
     * read of the order says how many.
     */
    private fun TillSale.moved(to: SalePhase, at: Long): TillSale = after(to).let { phase ->
        copy(
            phase = phase,
            confirmations = when (phase) {
                SalePhase.Seen -> 0
                SalePhase.Confirming -> (confirmations ?: 1).coerceAtLeast(1)
                else -> confirmations
            },
            seenAt = seenAt ?: at.takeIf { phase == SalePhase.Seen || phase == SalePhase.Confirming || phase == SalePhase.Partial },
            paidAt = paidAt ?: at.takeIf { phase == SalePhase.Paid },
            endedAt = endedAt ?: at.takeIf { phase.isUnpaid },
        )
    }

    /**
     * Where a sale goes when Monokulo says [to]. Only a payment moves a sale
     * that ended unpaid: a reversed one stays reversed (its order reads
     * `pending` again once the double spend voided the payment, and carries
     * nothing saying so), and an expired or cancelled one never goes back
     * to waiting. Otherwise, what Monokulo says.
     */
    private fun TillSale.after(to: SalePhase): SalePhase = when {
        to == SalePhase.Paid -> to
        phase == SalePhase.DoubleSpend -> phase
        phase.isUnpaid && !to.isUnpaid -> phase
        else -> to
    }

    /**
     * The cashier closed the sale for now: its code comes down and it goes
     * to the shift, where Monokulo's late payment still reaches it and the
     * cashier can refund it. Nothing is said to Monokulo.
     */
    fun close(orderId: String) {
        val at = now()
        change { s -> s.copy(sales = s.sales.map { if (it.orderId == orderId && it.closedAt == null) it.copy(closedAt = at) else it }) }
    }

    /**
     * Watching stopped: every live sale's code comes down, closed as the
     * close sheet's "Not sure" closes one, so none stays up with nothing
     * watching it and each waits in the shift to be refunded or paid late.
     */
    fun closeLive() {
        val at = now()
        change { s -> s.copy(sales = s.sales.map { if (it.live) it.copy(closedAt = at) else it }) }
    }

    /**
     * The clock passed a live sale's `expiresAt`: its code is ended by the
     * clock (expired, as Monokulo will say), so it no longer waits, pulses or
     * takes the panel, even when Monokulo is late to say so. Only a payment
     * moves it on from there, as for any sale that ended unpaid.
     */
    fun lapse() {
        val at = now()
        change { s -> s.copy(sales = s.sales.map { if (it.live && it.expiresAt in 1..at) it.moved(SalePhase.Expired, at) else it }) }
    }

    /** When the next live code runs out, Unix seconds; null with none live. */
    fun nextLapse(state: TillState): Long? = state.sales.filter { it.live && it.expiresAt > 0 }.minOfOrNull { it.expiresAt }

    /**
     * Ends each live code when its time runs out ([lapse]), whether or not
     * Monokulo has said so yet: one wait, on [nowMs], for the next code to
     * run out, started again whenever that changes. A [lapse] that ends
     * nothing (the wall clock stepped back during the wait, or [now] reads a
     * second behind [nowMs]) tries again a second later until the code is
     * ended, so a dead code never stays up waiting for some other change.
     */
    suspend fun lapseOnTime(nowMs: () -> Long = System::currentTimeMillis) {
        sales.map(::nextLapse).distinctUntilChanged().collectLatest { next ->
            if (next == null) return@collectLatest
            delay((next * 1000 - nowMs()).coerceAtLeast(0))
            while (true) {
                lapse()
                // Ended, the sales change and this wait is replaced by the next one.
                if (nextLapse(sales.value) != next) break
                delay(LAPSE_RETRY_MS)
            }
        }
    }

    /** "Show the code again" on a closed sale: its code is back up. */
    fun reopen(orderId: String) {
        change { s -> s.copy(sales = s.sales.map { if (it.orderId == orderId) it.copy(closedAt = null) else it }) }
    }

    /**
     * Refunds the POS sale of an order that was never paid: through the POS
     * when the till can ([Refunds.ByTill]), else marks it for the cashier to
     * refund there ([Refunds.InPos]), which sends nothing. A sale another
     * point of sale took (the cashier changed it since) is only ever marked:
     * its payment id means nothing to this source's POS. A paid sale is
     * never refunded from here, nor one already refunded: the payment may
     * land while the cashier's "No" is on its way.
     */
    suspend fun refund(orderId: String) {
        val sale = state.value.sales.firstOrNull { it.orderId == orderId } ?: return
        if (sale.phase == SalePhase.Paid || sale.refundedAt != null) return
        val found = sale.found ?: return
        change { it.copy(refundProblems = it.refundProblems - orderId) }
        val byTill = source.refunds as? Refunds.ByTill
        if (byTill == null || found.pointOfSale != source.pointOfSale) {
            val at = now()
            change { s -> s.copy(sales = s.sales.map { if (it.orderId == orderId) it.copy(refundPendingAt = it.refundPendingAt ?: at) else it }) }
            return
        }
        try {
            byTill.refund(found)
            markRefunded(orderId)
        } catch (e: Exception) {
            change { it.copy(refundProblems = it.refundProblems + (orderId to "Couldn't refund it in the POS: ${e.message}. Refund it there instead.")) }
        }
    }

    /**
     * The refund sheet's "I've refunded it", for a sale the cashier was told
     * to refund in the POS ([TillSale.refundPendingAt]).
     *
     * When the sale's point of sale is this source's and the source can ask
     * it ([Refunds.InPos]), asks now: true, and the sale is refunded, when it
     * shows the refund; false, and the sale stays as it was, when it doesn't
     * yet or couldn't be asked.
     *
     * Otherwise nothing can check: the sale's point of sale isn't the till's
     * any more (the cashier changed it), or the source can't ask its own
     * ([Refunds.ByTill]). The cashier's word marks the sale refunded, so it
     * isn't left needing action for the rest of the shift.
     */
    suspend fun confirmRefund(orderId: String): Boolean {
        val sale = state.value.sales.firstOrNull { it.orderId == orderId } ?: return false
        if (sale.refundedAt != null) return true
        if (sale.refundPendingAt == null) return false
        val found = sale.found ?: return false
        val refunds = source.refunds as? Refunds.InPos
        if (refunds != null && found.pointOfSale == source.pointOfSale && !seenRefunded(refunds, found)) return false
        markRefunded(orderId)
        return true
    }

    private fun add(order: Order, code: PaymentCode, read: SaleOrder?, found: FoundSale) {
        // Another till may have made this order already, and the store's events
        // may have moved it on (paid, even): keep what they said, and when,
        // else the order's own. Read and replaced in the one change, so a
        // close or a refund stamped meanwhile on another thread isn't lost.
        change { s ->
            val known = s.sales.firstOrNull { it.orderId == order.orderId }
            val sale = TillSale(
                orderId = order.orderId,
                amount = order.amount ?: decimalAmount(found.amountMinor, found.currency),
                currency = order.currency ?: found.currency,
                reference = order.merchantOrderId,
                code = code,
                phase = known?.phase ?: order.phase,
                url = order.url,
                settled = known == null && order.isFinal,
                expiresAt = order.expiresAt,
                xmr = PaymentUri.xmr(order.piconero),
                address = order.address,
                found = found,
                receipt = read?.receipt,
                customerName = read?.customerName,
                createdAt = runCatching { Instant.parse(found.createdAt).epochSecond }.getOrElse { now() },
                seenAt = known?.seenAt,
                paidAt = known?.paidAt,
                endedAt = known?.endedAt,
                closedAt = known?.closedAt,
                refundedAt = known?.refundedAt,
                refundPendingAt = known?.refundPendingAt,
                confirmations = known?.confirmations ?: order.confirmations,
                confirmationsRequired = order.confirmationsRequired ?: known?.confirmationsRequired,
            )
            s.copy(sales = listOf(sale) + s.sales.filterNot { it.orderId == order.orderId })
        }
    }

    private fun SaleEvent.toTillSale(code: PaymentCode) = TillSale(
        orderId = orderId,
        amount = amount,
        currency = currency,
        reference = merchantOrderId,
        code = code,
        phase = SalePhase.Waiting,
        expiresAt = expiresAt,
        xmr = PaymentUri.xmr(piconero),
        address = address,
        deviceName = deviceName.ifBlank { null },
        url = orderUrl,
        createdAt = at,
    )

    companion object {
        /** The most sales a shift keeps, newest first: older ones drop off the list and the saved shift. */
        const val MAX_SALES = 500

        /** The timer asks the POS about pending refunds at most this often, in seconds (decision 130). */
        const val REFUND_LOOK_SECONDS = 30L

        /**
         * How often [followConfirmations] reads a seen or confirming sale again: a
         * Monero block comes about every two minutes, so a confirmation shows within
         * a sixth of one, for a request a sale every 20 s while it's on its way.
         */
        const val CONFIRMATIONS_READ_MS = 20_000L

        /** How long [lapseOnTime] waits to try again when the clock it lapsed by hadn't reached a code's end yet. */
        const val LAPSE_RETRY_MS = 1_000L

        /** `sq:` and the start of SHA-256 of the POS's payment id: the same for every till, and opaque. */
        fun idempotencyKey(sale: FoundSale): String {
            val hash = MessageDigest.getInstance("SHA-256").digest(sale.id.toByteArray())
            return "sq:" + hash.joinToString("") { "%02x".format(it) }.take(40)
        }
    }
}
