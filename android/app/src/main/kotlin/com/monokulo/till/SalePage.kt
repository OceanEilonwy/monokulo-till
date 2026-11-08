package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.sales.Standing
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live
import com.monokulo.till.sales.standing
import java.time.ZoneId
import java.util.Locale

/** The colour roles behind a detail page's hero glyph and its note. */
enum class Ground { Paid, Warn, Danger, Chip, Card }

/** A detail page's note: a few sentences on a card, on [Ground.Danger] for a reversed payment. */
data class Note(val text: String, val ground: Ground = Ground.Card)

/** A line of the timeline card: "Rung up" and its time, or "Till" and the till's name. */
data class Fact(val label: String, val value: String)

/** What a page offers: refund the point of sale's sale, put the code back up. */
enum class PageAction { Refund, ShowCodeAgain }

/** A sale's page when it isn't live (the design's row 2b), in words and roles. */
data class SalePageModel(
    val icon: StateIcon,
    val ground: Ground,
    val heading: String,
    /** The heading in warn: paid after the refund, which needs the cashier. */
    val headingWarn: Boolean,
    /** The amount in muted: refunded, so it no longer counts. */
    val amountMuted: Boolean,
    val line: String,
    val note: Note?,
    /** Whether the page shows what was bought: never for another till's sale, which this till has no receipt for. */
    val order: Boolean,
    val facts: List<Fact>,
    val actions: List<PageAction>,
)

/**
 * A sale screen's top, on a till on [till] (decision 141): the point of sale that names the
 * sale's words and receipt ([pos], [PosText.of]), whether the till has changed from it since
 * ([elsewhere]), the bar's second line ([place]: "Front Counter · this till", "Clover · this
 * till" for a sale rung up at the point of sale it changed from), and whether the sale offers
 * "Back to Square" ([backToPos]): only on a till on Square, and never for a sale another point
 * of sale took (decision 135).
 */
data class SaleHeader(val pos: PointOfSale?, val elsewhere: Boolean, val place: String, val backToPos: Boolean) {
    companion object {
        fun of(sale: TillSale, till: PointOfSale?, location: String?): SaleHeader {
            val elsewhere = PosText.elsewhere(sale, till)
            return SaleHeader(
                pos = PosText.of(sale, till),
                elsewhere = elsewhere,
                place = SaleText.place(sale, PosText.placeOf(sale, till, location)),
                backToPos = !elsewhere && till == PointOfSale.Square,
            )
        }
    }
}

/**
 * The detail pages (PaidDetail, LatePaidDetail, ClosedDetail,
 * ExpiredDetail, ReversedDetail, RefundedDetail, OtherTillDetail): what
 * each says, from where the sale stands. Times the app doesn't know aren't
 * written (decision 71). [till] is the till's point of sale, which names the
 * sale's. A sale whose order has a title at the point of sale (Clover's "Table
 * 4") shows it first among the facts. [elsewhere]: the sale was rung up at a
 * point of sale this till has since changed from (decision 141), so the till
 * can't refund it or see its refund, and says to refund it there; one already
 * told to refund it there keeps Refund, for the cashier's word that it's done.
 */
object SalePage {
    fun of(
        sale: TillSale,
        till: PointOfSale?,
        now: Long,
        locale: Locale = Locale.getDefault(),
        zone: ZoneId = ZoneId.systemDefault(),
        elsewhere: Boolean = false,
    ): SalePageModel {
        val pos = till
        val name = PosText.name(pos)
        val status = SaleText.status(sale, till)
        val hasIt = PosText.hasItAsMonero(pos)
        fun time(at: Long) = SaleText.time(at, locale, zone)
        fun fact(label: String, at: Long?) = at?.takeIf { it > 0 }?.let { Fact(label, time(it)) }
        val xmr = SaleText.xmr(sale)
        val rungUp = fact("Rung up", sale.createdAt)
        val ended = fact(endedLabel(sale.phase), sale.endedAt)
        val watching = when {
            sale.settled -> "Monokulo has stopped watching it."
            else -> "Monokulo keeps watching for a late payment for a while: if one arrives, this sale turns paid."
        }
        val model = when (val standing = sale.standing) {
            is Standing.Paid -> {
                val closed = sale.closedAt != null
                val note = when {
                    standing.afterRefund -> Note("$xmr arrived after the $name sale was refunded. Ring the sale up in $name again, or return the payment.")
                    closed -> Note("The customer paid ${after(sale.closedAt, sale.paidAt)}after the sale was closed. ${nothingToDo(pos)}")
                    standing.late -> Note("The customer paid ${after(sale.endedAt, sale.paidAt)}after the code ran out. ${nothingToDo(pos)}")
                    else -> null
                }
                val facts = when {
                    standing.afterRefund -> listOf(rungUp, refundedBecause(sale, ::fact), fact(PosText.refundedIn(pos), sale.refundedAt), fact("Paid", sale.paidAt))
                    closed -> listOf(rungUp, fact("Closed", sale.closedAt), fact("Paid", sale.paidAt))
                    standing.late -> listOf(rungUp, ended, fact("Paid", sale.paidAt))
                    else -> listOf(rungUp, fact("Seen on the network", sale.seenAt), fact("Paid", sale.paidAt))
                }
                SalePageModel(
                    StateIcon.Tick, Ground.Paid, status, headingWarn = standing.afterRefund, amountMuted = false,
                    line = "$xmr received" + sale.paidAt?.let { " · " + time(it) }.orEmpty(),
                    note = note, order = true, facts = facts.filterNotNull(), actions = emptyList(),
                )
            }
            Standing.Closed -> SalePageModel(
                StateIcon.Clock, Ground.Warn, status, headingWarn = false, amountMuted = false, line = xmr,
                note = Note(
                    "$hasIt. If the customer isn't paying, refund it. " + when {
                        sale.settled -> "Monokulo has stopped watching it."
                        sale.expiresAt > now -> "The code is still live until ${time(sale.expiresAt)}, and Monokulo keeps watching for a late payment after that: if one arrives, this sale turns paid."
                        else -> watching
                    },
                ),
                order = true, facts = listOfNotNull(rungUp, fact("Closed", sale.closedAt)),
                actions = listOf(PageAction.Refund, PageAction.ShowCodeAgain),
            )
            Standing.Expired -> SalePageModel(
                StateIcon.Clock, Ground.Warn, status, headingWarn = false, amountMuted = false, line = xmr,
                note = Note(
                    if (sale.phase == SalePhase.Cancelled) {
                        "The order was cancelled in Monokulo. $hasIt: refund it if the customer isn't paying."
                    } else {
                        "The code ran out at ${time(sale.expiresAt)} with no payment. $hasIt: refund it if the customer isn't paying. $watching"
                    },
                ),
                order = true, facts = listOfNotNull(rungUp, ended), actions = listOf(PageAction.Refund),
            )
            Standing.Reversed -> SalePageModel(
                StateIcon.Warning, Ground.Danger, status, headingWarn = false, amountMuted = false, line = "$xmr · no longer counts",
                note = Note("The payment was spent twice, so it doesn't count. Don't hand anything over. $hasIt: refund it there.", Ground.Danger),
                order = true, facts = listOfNotNull(rungUp, fact("Seen on the network", sale.seenAt), ended),
                actions = listOf(PageAction.Refund),
            )
            Standing.Refunded -> SalePageModel(
                StateIcon.Refund, Ground.Chip, status, headingWarn = false, amountMuted = true,
                line = listOfNotNull(sale.refundedAt?.let { "Refunded " + time(it) }, "nothing was paid").joinToString(" · "),
                note = Note("Nothing more to do. If the customer pays after all, Till tells you straight away and this sale shows Paid after the refund."),
                order = true, facts = listOfNotNull(rungUp, refundedBecause(sale, ::fact), fact(PosText.refundedIn(pos), sale.refundedAt)),
                actions = emptyList(),
            )
            Standing.RefundInPos -> SalePageModel(
                StateIcon.Refund, Ground.Warn, status, headingWarn = false, amountMuted = false, line = "$xmr · not paid",
                note = Note("Refund this sale in $name, then tap Refund to say it's done. While the shift is on, Till notices by itself too."),
                order = true, facts = listOfNotNull(rungUp, refundedBecause(sale, ::fact), fact("Told to refund it", sale.refundPendingAt)),
                actions = listOf(PageAction.Refund),
            )
            Standing.OtherTill -> {
                val other = sale.deviceName
                SalePageModel(
                    StateIcon.Close, Ground.Chip, status, headingWarn = false, amountMuted = false, line = "$xmr · not paid",
                    note = Note(
                        (if (other != null) "$other rang this sale up." else "Another till rang this sale up.") +
                            " Its $name sale and what was bought stay on that till, so refund it there if the customer isn't paying. A late payment still shows here.",
                    ),
                    order = false,
                    facts = listOfNotNull(
                        other?.let { Fact("Till", it) },
                        rungUp,
                        if (sale.phase.isFinal) ended else fact("Closed here", sale.closedAt),
                    ),
                    actions = emptyList(),
                )
            }
            // A live sale shows its code, not a page; this is only what it would say.
            is Standing.Live -> SalePageModel(
                StateIcon.Ring, Ground.Chip, status, headingWarn = false, amountMuted = false, line = xmr,
                note = null, order = true, facts = listOfNotNull(rungUp, fact("Seen on the network", sale.seenAt)), actions = emptyList(),
            )
        }
        val here = when {
            !elsewhere || PageAction.Refund !in model.actions -> model
            // Already told to refund it there: Refund opens the sheet, whose "I've refunded it" takes the cashier's word.
            sale.standing == Standing.RefundInPos -> model.copy(
                note = Note("This sale was rung up in $name before this till changed its point of sale. Refund it in $name, then tap Refund to say it's done."),
            )
            else -> model.copy(
                actions = model.actions - PageAction.Refund,
                note = Note("This sale was rung up in $name before this till changed its point of sale. If the customer isn't paying, refund it in $name."),
            )
        }
        val title = sale.found?.title ?: return here
        return here.copy(facts = listOf(Fact("Order", title)) + here.facts)
    }

    /**
     * "15 minutes ", "1 minute ", "under a minute " from [from] to [to]: how
     * late a payment came. Empty when either time isn't known.
     */
    fun after(from: Long?, to: Long?): String {
        if (from == null || to == null) return ""
        val minutes = (to - from).coerceAtLeast(0) / 60
        return when (minutes) {
            0L -> "under a minute "
            1L -> "1 minute "
            else -> "$minutes minutes "
        }
    }

    /** How a refunded sale came to be refunded: the cashier closed it, or its code ran out first. */
    private fun refundedBecause(sale: TillSale, fact: (String, Long?) -> Fact?): Fact? = when {
        sale.closedAt != null -> fact("Closed: customer not paying", sale.closedAt)
        else -> fact(endedLabel(sale.phase), sale.endedAt)
    }

    private fun endedLabel(phase: SalePhase): String = when (phase) {
        SalePhase.Cancelled -> "Cancelled"
        SalePhase.DoubleSpend -> "Reversed"
        else -> "Code expired"
    }

    private fun nothingToDo(pos: PointOfSale?) =
        "${PosText.nameFirst(pos)} already has it as Monero, so there's nothing to do. Hand over the order if they're still here."
}

/** The close sheet's three answers to "Is the customer paying?" (the design's CloseAsk). */
enum class CloseChoice { RefundIt, CloseForNow, KeepUp }

/** What the close sheet's answer does: close the sale, refund it, and where the cashier lands. */
data class CloseOutcome(val close: Boolean, val refund: Boolean, val then: Then) {
    enum class Then {
        /** Stay on the sale, which shows its page now (Refunded, or Closed when the refund failed). */
        Sale,
        Shift,
        /** Nothing happens: the sheet goes and the code stays up. */
        Stay,
    }
}

/** The close sheet (decisions 62, 70). */
object Closing {
    /**
     * The answers offered: all three for this till's sale; for another
     * till's, or one rung up at a point of sale this till has changed from
     * ([elsewhere]), "No: close it" and "Yes" (this till can't refund it, so
     * "No" and "Not sure" would be the same); none once it's refunded or its code
     * is down, where the sale screen has no X.
     */
    fun choices(sale: TillSale, elsewhere: Boolean = false): List<CloseChoice> = when {
        sale.standing !is Standing.Live -> emptyList()
        sale.found == null || elsewhere -> listOf(CloseChoice.CloseForNow, CloseChoice.KeepUp)
        else -> CloseChoice.entries
    }

    /**
     * Whether the sheet's answer still stands for [sale] as it is now: only
     * while its code is up. Paid (or ended) since the cashier tapped, the
     * answer is dropped, so a paid sale is never closed or refunded by it.
     */
    fun applies(sale: TillSale?): Boolean = sale?.live == true

    fun outcome(choice: CloseChoice): CloseOutcome = when (choice) {
        CloseChoice.RefundIt -> CloseOutcome(close = true, refund = true, then = CloseOutcome.Then.Sale)
        CloseChoice.CloseForNow -> CloseOutcome(close = true, refund = false, then = CloseOutcome.Then.Shift)
        CloseChoice.KeepUp -> CloseOutcome(close = false, refund = false, then = CloseOutcome.Then.Stay)
    }

    /** An answer's words at a till on [till]: another till's sale's "No" only closes it. */
    fun title(choice: CloseChoice, sale: TillSale, till: PointOfSale?): String = when (choice) {
        CloseChoice.RefundIt -> "No: " + PosText.refundItIn(till)
        CloseChoice.CloseForNow -> if (sale.found == null) "No: close it" else "Not sure: close it for now"
        CloseChoice.KeepUp -> "Yes: keep the code up"
    }

    /**
     * Whether "No" on a sale of [pos] leaves the refund to the cashier there, with the
     * refund sheet: [pos] isn't the point of sale the till's [source] watches (the
     * cashier changed it since), or [source]'s API can't refund (Clover, decision 130).
     * `Till.refund`'s own rule, from the two points of sale.
     */
    fun refundsThere(pos: PointOfSale?, source: PointOfSale): Boolean = pos != source || when (source) {
        PointOfSale.Square -> false
        PointOfSale.Clover -> true
    }

    /**
     * The line under an answer, with [secondsLeft] on the code for "Yes"; [pos] is the sale's
     * point of sale, [source] the one the till's source watches ([refundsThere]).
     */
    fun detail(choice: CloseChoice, sale: TillSale, pos: PointOfSale?, source: PointOfSale, secondsLeft: Long): String = when (choice) {
        CloseChoice.RefundIt ->
            if (refundsThere(pos, source)) "Till closes it and shows you how to refund it in ${PosText.name(pos)}."
            else "Till refunds the sale and closes it."
        CloseChoice.CloseForNow -> if (sale.found == null) {
            PosText.refundOnOtherTill(pos, sale.deviceName ?: "the till that rang it up") + " A late payment still shows here."
        } else {
            "It stays in the shift. A late payment still counts, and you can refund it there."
        }
        CloseChoice.KeepUp -> when (sale.phase) {
            SalePhase.Waiting, SalePhase.Partial -> "${SaleText.clock(secondsLeft)} left to pay."
            else -> "The payment is being confirmed."
        }
    }

    /** "Sale #bP9m · $13.88 · Square has it as Monero", or "… · From Back till" for another till's sale. */
    fun subtitle(sale: TillSale, till: PointOfSale?): String =
        "${SaleText.title(sale)} · ${SaleText.amount(sale)} · " +
            if (sale.found != null) "${PosText.nameFirst(till)} has it as Monero" else SaleText.place(sale, null)
}
