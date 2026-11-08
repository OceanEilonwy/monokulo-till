package com.monokulo.till.sales

import com.monokulo.till.core.PointOfSale
import java.time.Instant

/** Where a source keeps its place, so a restart neither repeats nor skips sales. */
interface CursorStore {
    /** RFC 3339: payments from here on are looked at. */
    fun since(): String?
    fun handled(): Set<String>
    fun save(since: String, handled: Set<String>)
}

/**
 * Square as a [SaleSource]: the till's location's payments since it
 * started watching, kept to Other tenders named [SquareConfig.tenderName]
 * (`docs/design.md` §1). How Square reports a Point of Sale "Other" tender
 * is to be confirmed by the Square spike (`docs/design.md` §10); this reads
 * it as an `EXTERNAL` payment whose `external_details.source` is the
 * tender's name.
 */
class SquareSource(
    private val api: SquareApi,
    private val config: SquareConfig,
    private val location: SquareLocation,
    private val cursor: CursorStore,
    private val now: () -> Instant = Instant::now,
) : SaleSource {
    /** Payments made shortly before the cursor are looked at again, in case Square was slow to list them. */
    private val overlapSeconds = 120L

    /** Most payment ids remembered as handled. */
    private val remembered = 500

    override suspend fun newSales(): List<FoundSale> {
        val since = cursor.since() ?: now().toString().also { cursor.save(it, emptySet()) }
        val from = Instant.parse(since).minusSeconds(overlapSeconds).toString()
        val handled = cursor.handled()
        return api.payments(location.id, from)
            .filter { isMonero(it) && it.id !in handled }
            .map { it.toFoundSale() }
    }

    override fun handled(sale: FoundSale) {
        val handled = (cursor.handled() + sale.id).toList().takeLast(remembered).toSet()
        // The cursor moves up to the sale, never back.
        val since = maxOf(Instant.parse(cursor.since() ?: sale.createdAt), Instant.parse(sale.createdAt))
        cursor.save(since.toString(), handled)
    }

    /** The receipt and the order's recipient ([SquareOrder.customerName]), from the one read. */
    override suspend fun order(sale: FoundSale): SaleOrder {
        val orderId = sale.orderId ?: return SaleOrder(null)
        val order = api.order(orderId)
        return SaleOrder(SquareReceipts.of(order, location, sale), order.customerName)
    }

    override val pointOfSale = PointOfSale.Square

    /** A full refund through Square's API, once whatever the retries ([SquareApi.refund]'s key). */
    override val refunds: Refunds.ByTill = Refunds.ByTill { sale ->
        api.refund(
            paymentId = sale.id,
            amount = Money(sale.amountMinor, sale.currency),
            idempotencyKey = "till-refund-${sale.id}".take(45),
            reason = "Not paid in Monero",
        )
    }

    internal fun isMonero(p: SquarePayment): Boolean =
        p.status == "COMPLETED" && p.sourceType == "EXTERNAL" &&
            p.externalDetails?.source?.trim().equals(config.tenderName.trim(), ignoreCase = true)

    private fun SquarePayment.toFoundSale(): FoundSale {
        val money = totalMoney ?: amountMoney
        return FoundSale(
            id = id,
            amountMinor = money.amount,
            currency = money.currency,
            reference = receiptNumber,
            orderId = orderId,
            createdAt = createdAt,
            deviceId = deviceDetails?.deviceId,
            pointOfSale = PointOfSale.Square,
        )
    }
}
