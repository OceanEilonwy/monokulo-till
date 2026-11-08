package com.monokulo.till.sales

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.receipt.Receipt
import kotlinx.serialization.Serializable

/**
 * A point of sale the till watches for Monero sales (`docs/design.md` §1,
 * "POS sources"): Square, Clover.
 */
interface SaleSource {
    /**
     * The point of sale this source watches: a sale another one took (the
     * cashier changed the point of sale since) is never refunded or asked
     * about through this one.
     */
    val pointOfSale: PointOfSale

    /**
     * Sales tendered as Monero since the source started watching, oldest
     * first, that haven't been [handled]. Calling it again gives the same
     * ones until they are.
     */
    suspend fun newSales(): List<FoundSale>

    /**
     * The sale's receipt and whom it's for, from one read of its order: a
     * null receipt when it can't be read, and no name at a point of sale
     * that gives none.
     */
    suspend fun order(sale: FoundSale): SaleOrder

    /** How a sale never paid in Monero is refunded at this point of sale. */
    val refunds: Refunds

    /** [sale] has its order: don't give it again. */
    fun handled(sale: FoundSale)
}

/**
 * What the till takes from a sale's order at the point of sale: its
 * receipt, and the customer's name when the point of sale gives one
 * (Square's fulfillment recipient). The name stays on the device; the
 * receipt leaves it only in the sale's code ([TillSale.receipt]).
 */
data class SaleOrder(val receipt: Receipt?, val customerName: String? = null)

/**
 * Who refunds a sale never paid in Monero (decision 130). A type rather
 * than a `refund()` that does nothing at a point of sale with no refund in
 * its API: there, the till can only look, by construction.
 */
sealed interface Refunds {
    /** The till does, through the point of sale's API (Square). */
    fun interface ByTill : Refunds {
        suspend fun refund(sale: FoundSale)
    }

    /**
     * The cashier does, in the point of sale, and the till watches for it
     * (Clover: no API refunds an external-tender payment). It carries no
     * words: what the cashier is told names the sale's own point of sale
     * ([FoundSale.pointOfSale]), worded in the app (decision 136).
     */
    fun interface InPos : Refunds {
        /** Whether the point of sale now shows [sale] refunded. */
        suspend fun refunded(sale: FoundSale): Boolean
    }
}

/** A Monero sale the POS closed. */
@Serializable
data class FoundSale(
    /** The POS's id for the payment: what the till's idempotency key is made from. */
    val id: String,
    /** In the currency's minor unit (cents). */
    val amountMinor: Long,
    val currency: String,
    /** What the POS printed or showed for it ("bP9m"), if anything. */
    val reference: String?,
    /** The POS's order, for the receipt. */
    val orderId: String?,
    /** RFC 3339. */
    val createdAt: String,
    /** The device that took it, when the POS says: to show it on its own till only. */
    val deviceId: String?,
    /** The name the cashier gave the order ("Table 4"), when the POS has one: Clover's title; Square leaves it null. */
    val title: String? = null,
    /**
     * The point of sale that took it, so a shift's saved sales keep naming it
     * after the till changes point of sale; a sale saved before Clover joined
     * reads as Square's.
     */
    val pointOfSale: PointOfSale = PointOfSale.Square,
)
