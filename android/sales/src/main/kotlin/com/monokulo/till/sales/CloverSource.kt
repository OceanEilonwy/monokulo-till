package com.monokulo.till.sales

import com.monokulo.till.core.PointOfSale
import java.time.Duration
import java.time.Instant

/** Where the Monero tender's id is remembered, per merchant: Clover gives it when it's made. */
interface TenderStore {
    fun tenderId(merchantId: String): String?
    fun save(merchantId: String, tenderId: String?)
}

/** What the Monero tender is at the merchant, as the sign-in card shows it. */
sealed interface TenderState {
    /** The cashier can take Monero in Clover Register with [tender]. */
    data class Ready(val tender: CloverTender) : TenderState

    /**
     * Clover didn't let the till add it (no Merchant write, risk R9): the
     * cashier adds a payment type called Monero in Clover, then asks the
     * till to look again. [reason] is what Clover said.
     */
    data class AddInClover(val reason: String) : TenderState
}

/**
 * Clover as a [SaleSource] (`docs/design.md` §1): the merchant's payments
 * since the till started watching, kept to the Monero tender's, that
 * succeeded and weren't voided or refunded. A tender made over REST shares
 * its `labelKey` with every external payment, so the tender's id, which
 * the till found or made, is what tells Monero apart (decision 129).
 * Clover has no API that refunds such a payment: the cashier does, in
 * Clover's Orders app, and [refunds] looks for it (decision 130).
 */
class CloverSource(
    private val api: CloverApi,
    private val config: CloverConfig,
    private val merchant: CloverMerchant,
    private val cursor: CursorStore,
    private val tenders: TenderStore,
    private val now: () -> Instant = Instant::now,
) : SaleSource {
    /** Payments made shortly before the cursor are looked at again, in case Clover was slow to list them. */
    private val overlapSeconds = 120L

    /** Most payment ids remembered as handled. */
    private val remembered = 500

    /** Clover filters payments by time at most 90 days back. */
    private val furthestBack = Duration.ofDays(89)

    /** Clover asked the till to slow down: no looks before this. */
    @Volatile
    private var holdUntil: Instant? = null

    /**
     * The Monero tender: the one remembered, else the merchant's tender
     * labelled [CloverConfig.tenderLabel] (trimmed, any case), else one made
     * now. Called after sign-in and from the card's "Look again", never on a
     * timer: a refused POST is [TenderState.AddInClover], tried again only
     * when the cashier asks.
     */
    suspend fun ensureTender(): TenderState {
        val all = api.tenders(merchant.id)
        tenders.tenderId(merchant.id)?.let { id ->
            all.firstOrNull { it.id == id }?.let { return TenderState.Ready(it) }
            // Deleted in Clover since: find it again, or make it.
            tenders.save(merchant.id, null)
        }
        labelled(all)?.let {
            tenders.save(merchant.id, it.id)
            return TenderState.Ready(it)
        }
        val made = try {
            api.createTender(merchant.id, config.tenderLabel)
        } catch (e: CloverException) {
            if (e.status in 400..499 && e.status != 429) return TenderState.AddInClover(e.message.orEmpty())
            throw e
        }
        tenders.save(merchant.id, made.id)
        return TenderState.Ready(made)
    }

    override suspend fun newSales(): List<FoundSale> {
        val at = now()
        if (holdUntil?.isAfter(at) == true) return emptyList()
        return try {
            look(at)
        } catch (e: CloverException) {
            if (e.status == 429) holdUntil = at.plusSeconds(e.retryAfterSeconds ?: 1)
            throw e
        }
    }

    private suspend fun look(at: Instant): List<FoundSale> {
        // Not made yet (the card asks the cashier to): found by its label, never made from here.
        val tenderId = tenders.tenderId(merchant.id)
            ?: labelled(api.tenders(merchant.id))?.id?.also { tenders.save(merchant.id, it) }
            ?: return emptyList()
        val since = cursor.since() ?: at.toString().also { cursor.save(it, emptySet()) }
        val from = maxOf(Instant.parse(since).minusSeconds(overlapSeconds), at.minus(furthestBack))
        val handled = cursor.handled()
        return api.payments(merchant.id, tenderId, from.toEpochMilli())
            .filter { isMonero(it, tenderId) && it.id !in handled }
            .sortedBy { it.createdTime }
            .map { it.toFoundSale() }
    }

    override fun handled(sale: FoundSale) {
        val handled = (cursor.handled() + sale.id).toList().takeLast(remembered).toSet()
        // The cursor moves up to the sale, never back.
        val since = maxOf(Instant.parse(cursor.since() ?: sale.createdAt), Instant.parse(sale.createdAt))
        cursor.save(since.toString(), handled)
    }

    /** The receipt alone: the till reads no customer from Clover's order. */
    override suspend fun order(sale: FoundSale): SaleOrder {
        val orderId = sale.orderId ?: return SaleOrder(null)
        if (merchant.country == null) return SaleOrder(null)
        return SaleOrder(CloverReceipts.of(api.order(merchant.id, orderId), merchant, sale))
    }

    override val pointOfSale = PointOfSale.Clover

    override val refunds: Refunds.InPos = object : Refunds.InPos {
        /**
         * The payment's refunds that weren't voided reach its amount, or the
         * payment was voided, or its order reads `REFUNDED`. A part refund
         * isn't enough.
         */
        override suspend fun refunded(sale: FoundSale): Boolean {
            val payment = api.payment(merchant.id, sale.id)
            if (payment.voided || payment.result == "VOIDED") return true
            if (payment.refunds.elements.filterNot { it.voided }.sumOf { it.amount } >= sale.amountMinor) return true
            val orderId = sale.orderId ?: payment.order?.id ?: return false
            return api.order(merchant.id, orderId).paymentState == "REFUNDED"
        }
    }

    private fun labelled(all: List<CloverTender>): CloverTender? {
        val label = config.tenderLabel.trim()
        val matches = all.filter { it.label?.trim().equals(label, ignoreCase = true) }
        return matches.firstOrNull { it.enabled && it.visible } ?: matches.firstOrNull()
    }

    internal fun isMonero(p: CloverPayment, tenderId: String): Boolean =
        p.result == "SUCCESS" && !p.voided && p.tender?.id == tenderId && p.refunds.elements.none { !it.voided }

    private fun CloverPayment.toFoundSale() = FoundSale(
        id = id,
        amountMinor = amount,
        currency = merchant.currency,
        // Clover prints no receipt number on a payment (acceptance D7).
        reference = null,
        orderId = order?.id,
        createdAt = Instant.ofEpochMilli(createdTime).toString(),
        deviceId = device?.id,
        title = order?.title?.takeIf { it.isNotBlank() },
        pointOfSale = PointOfSale.Clover,
    )
}
