package com.monokulo.till.sales

import com.monokulo.till.core.SalePhase

/**
 * Where a sale stands on this till (the design's shift rows and detail
 * pages): what Monokulo says about its payment, together with what the
 * cashier did here (closed it for now, refunded it in the POS). A row's
 * words, its icon and which page it opens all follow from it, so nothing
 * else reads the phase and the cashier's marks side by side.
 */
sealed interface Standing {
    /** Its code is up: waiting, seen, confirming or part paid, and not closed. */
    data class Live(val phase: SalePhase) : Standing

    /**
     * Paid: [late] after the cashier closed it or its code ran out (which
     * one, `closedAt` and `endedAt` say), [afterRefund] after it was refunded
     * in the POS, so the cashier has to ring it up again or return it.
     */
    data class Paid(val late: Boolean = false, val afterRefund: Boolean = false) : Standing

    /** Closed for now, not paid: Monokulo still watches it for a late payment. */
    data object Closed : Standing

    /** Its code ran out (or the order was cancelled) with no payment. */
    data object Expired : Standing

    /** The payment was spent twice on the network, so it doesn't count. */
    data object Reversed : Standing

    /** Refunded in the POS, nothing paid. */
    data object Refunded : Standing

    /**
     * The cashier was told to refund it in the POS (`Refunds.InPos`), and
     * the POS doesn't show it refunded yet: nothing paid.
     */
    data object RefundInPos : Standing

    /** Another till found it and it ended, or was closed, unpaid: that till has the POS sale. */
    data object OtherTill : Standing
}

val TillSale.standing: Standing
    get() = when {
        phase == SalePhase.Paid -> Standing.Paid(late = closedAt != null || endedAt != null, afterRefund = refunded)
        !phase.isFinal && closedAt == null -> Standing.Live(phase)
        found == null -> Standing.OtherTill
        refunded -> Standing.Refunded
        refundPendingAt != null -> Standing.RefundInPos
        phase == SalePhase.DoubleSpend -> Standing.Reversed
        phase.isUnpaid -> Standing.Expired
        else -> Standing.Closed
    }

/** Its code is up: what the bubble lights for, the notification follows and the shift opens as a code. */
val TillSale.live: Boolean get() = standing is Standing.Live

/**
 * Needs the cashier: closed, expired or reversed and this till can refund it
 * in the POS; told to refund it there and not yet seen refunded; or paid
 * after it was refunded, so the sale must be rung up again or the payment
 * returned.
 */
val TillSale.needsAction: Boolean
    get() = when (val standing = standing) {
        Standing.Closed, Standing.Expired, Standing.Reversed, Standing.RefundInPos -> true
        is Standing.Paid -> standing.afterRefund
        else -> false
    }
