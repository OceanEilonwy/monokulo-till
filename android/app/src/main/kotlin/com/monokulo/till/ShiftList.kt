package com.monokulo.till

import com.monokulo.till.core.Amounts
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.sales.Standing
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.needsAction
import com.monokulo.till.sales.standing
import java.math.BigDecimal
import java.time.ZoneId
import java.util.Locale

/** The shift's filter chips (ShiftList's All / Needs action / Paid). */
enum class ShiftFilter { All, NeedsAction, Paid }

/** What a shift row opens: the code, for a sale that's live, else the sale's page. */
enum class RowOpens { Code, Details }

/** The colour role a row's second line is in (`ui/Theme.kt`'s tokens of the same names). */
enum class Tone { Link, Warn, Danger, Paid, Muted }

/** The glyph in a row's circle, and on a detail page's hero. */
enum class StateIcon { Ring, Clock, Warning, Tick, Refund, Close }

/** Which of its corners a shift row rounds: the rows make one card, rounded at its two ends. */
enum class RowEnds { Only, First, Middle, Last }

/**
 * One item of the shift's list under its chips (the design's A4): the
 * waiting tray's header, a sale's row (in the tray or under Earlier), the
 * tray's fold row, and the Earlier header. The screen keys each, so a row
 * leaving the tray for Earlier is the same item moving.
 */
sealed interface ShiftItem {
    /** "Waiting for payment" over the tray, with how many sales are live. */
    data class TrayHeader(val count: Int) : ShiftItem

    /** A sale's row, its corners [ends] counted within its section; [tray] when it sits in the waiting tray. */
    data class Row(val sale: TillSale, val ends: RowEnds, val tray: Boolean) : ShiftItem

    /** The tray's fold: a stack and "+[hidden]" folded, "Show fewer" [open]. */
    data class Fold(val hidden: Int, val open: Boolean) : ShiftItem

    /** The header over the sales that are no longer live, under the tray. */
    data object Earlier : ShiftItem
}

/** One shift row, in words and roles: what the screen draws for a sale. */
data class ShiftRow(
    val title: String,
    val sub: String,
    val tone: Tone,
    /** The title in muted, as a refunded sale's is: nothing more to do. */
    val quiet: Boolean,
    val icon: StateIcon,
    val opens: RowOpens,
)

/**
 * The shift (the design's ShiftList): which sales each filter shows, how
 * many need the cashier, the summary line and each row's words. Rows are
 * newest first, as the till keeps them.
 */
object ShiftList {
    fun rows(sales: List<TillSale>, filter: ShiftFilter): List<TillSale> = when (filter) {
        ShiftFilter.All -> sales
        ShiftFilter.NeedsAction -> sales.filter { it.needsAction }
        ShiftFilter.Paid -> sales.filter { it.standing is Standing.Paid }
    }

    /** How many live sales the folded tray shows before its fold row. */
    const val FOLD_AFTER = 3

    /**
     * The shift's list under its chips (A4): with a live sale under [filter], the
     * tray's header, every live sale newest first (only the first [FOLD_AFTER]
     * and the fold row while [folded] and there are more), then the Earlier
     * header and the rest newest first. With none, [rows] as they always read:
     * no header, no fold, no Earlier. Needs action shows the tray whole, with
     * no fold (a filter for what needs the cashier hides none of it), then the
     * sales ended unpaid; Paid has no live sale, so no header.
     */
    fun sections(sales: List<TillSale>, filter: ShiftFilter, folded: Boolean): List<ShiftItem> {
        val live = if (filter == ShiftFilter.Paid) emptyList() else sales.filter { it.standing is Standing.Live }
        val rest = rows(sales, filter).filter { it.standing !is Standing.Live }
        if (live.isEmpty()) return section(rest, tray = false)
        val folds = filter != ShiftFilter.NeedsAction && live.size > FOLD_AFTER
        val shown = if (folds && folded) live.take(FOLD_AFTER) else live
        val items = ArrayList<ShiftItem>(live.size + rest.size + 3)
        items += ShiftItem.TrayHeader(live.size)
        items += section(shown, tray = true)
        if (folds) items += ShiftItem.Fold(hidden = live.size - FOLD_AFTER, open = !folded)
        if (rest.isNotEmpty()) {
            items += ShiftItem.Earlier
            items += section(rest, tray = false)
        }
        return items
    }

    /** Whether [orderId]'s row is one the folded tray hides under [filter]: a live sale after the first [FOLD_AFTER]. */
    fun folds(sales: List<TillSale>, filter: ShiftFilter, orderId: String): Boolean {
        if (filter != ShiftFilter.All) return false
        return sales.filter { it.standing is Standing.Live }.indexOfFirst { it.orderId == orderId } >= FOLD_AFTER
    }

    private fun section(sales: List<TillSale>, tray: Boolean): List<ShiftItem.Row> =
        sales.mapIndexed { i, sale -> ShiftItem.Row(sale, ends(i, sales.lastIndex), tray) }

    /** The tray's header. */
    const val TRAY_HEADER = "Waiting for payment"

    /** The header over what is no longer live. */
    const val EARLIER = "Earlier"

    /** The fold's pill: how many more there are folded ("+18"), else "Show fewer". */
    fun foldLabel(fold: ShiftItem.Fold): String = if (fold.open) "Show fewer" else "+${fold.hidden}"

    /** What TalkBack says for the tray's header. */
    fun trayHeaderSaid(count: Int): String = "$TRAY_HEADER, $count ${if (count == 1) "sale" else "sales"}"

    /** What TalkBack says for the fold row (its state, collapsed or expanded, is said beside it). */
    fun foldSaid(fold: ShiftItem.Fold): String =
        if (fold.open) "Show fewer" else "Show ${fold.hidden} more waiting ${if (fold.hidden == 1) "sale" else "sales"}"

    /** What a new shift keeps of the last one's [sales]: those that still need the cashier (decision 86). */
    fun carried(sales: List<TillSale>): List<TillSale> = sales.filter { it.needsAction }

    /** What the Needs action chip counts (decision 60). */
    fun needsAction(sales: List<TillSale>): Int = sales.count { it.needsAction }

    /**
     * "5 paid · $69.50 · since 9:02 AM", or "Nothing paid yet · since 9:02 AM": the sales paid since the shift
     * started summed per currency (decision 72). A sale carried over from the last shift, paid after its refund,
     * still needs the cashier but isn't this shift's takings.
     */
    fun summary(sales: List<TillSale>, startedAt: Long?, locale: Locale = Locale.getDefault(), zone: ZoneId = ZoneId.systemDefault()): String {
        val paid = sales.filter { it.phase == SalePhase.Paid && (startedAt == null || (it.paidAt ?: startedAt) >= startedAt) }
        val totals = paid.groupBy { it.currency }.map { (currency, those) ->
            Amounts.fiat(those.sumOf { runCatching { BigDecimal(it.amount) }.getOrDefault(BigDecimal.ZERO) }.toPlainString(), currency, locale)
        }
        val head = if (paid.isEmpty()) listOf("Nothing paid yet") else listOf("${paid.size} paid") + totals
        val since = startedAt?.let { "since " + SaleText.time(it, locale, zone) }
        return (head + listOfNotNull(since)).joinToString(" · ")
    }

    /** Where a row goes: a live sale's code, any other sale's page (the design's note s2). */
    fun opens(sale: TillSale): RowOpens = if (sale.standing is Standing.Live) RowOpens.Code else RowOpens.Details

    /** The row for [sale] at a till on [till], with [secondsLeft] on its code when it's waiting or part paid. */
    fun row(
        sale: TillSale,
        till: PointOfSale?,
        secondsLeft: Long,
        locale: Locale = Locale.getDefault(),
        zone: ZoneId = ZoneId.systemDefault(),
    ): ShiftRow {
        val pos = till
        fun at(time: Long?) = time?.let { " · " + SaleText.time(it, locale, zone) }.orEmpty()
        val left = " · ${SaleText.clock(secondsLeft)} left"
        val other = sale.deviceName ?: "another till"
        val standing = sale.standing
        val (sub, tone, icon) = when (standing) {
            // The money on its way is the paid green, as its ring is (Board 5); still waiting for it, the link's.
            is Standing.Live -> when (standing.phase) {
                SalePhase.Partial -> Triple("Part paid$left", Tone.Link, StateIcon.Ring)
                SalePhase.Seen -> Triple("Seen on the network", Tone.Paid, StateIcon.Ring)
                SalePhase.Confirming -> Triple(SaleText.confirming(sale), Tone.Paid, StateIcon.Ring)
                else -> Triple("Waiting$left", Tone.Link, StateIcon.Ring)
            }
            is Standing.Paid -> Triple(
                SaleText.status(sale, till) + at(sale.paidAt),
                when {
                    standing.afterRefund -> Tone.Warn
                    standing.late -> Tone.Paid
                    else -> Tone.Muted
                },
                StateIcon.Tick,
            )
            Standing.Closed -> Triple("Closed · not paid · " + PosText.refundItIn(pos), Tone.Warn, StateIcon.Clock)
            Standing.Expired -> Triple(SaleText.status(sale, till), Tone.Warn, StateIcon.Clock)
            Standing.Reversed -> Triple("Payment reversed · don't hand it over", Tone.Danger, StateIcon.Warning)
            Standing.Refunded -> Triple(PosText.refundedIn(pos) + at(sale.refundedAt), Tone.Muted, StateIcon.Refund)
            Standing.RefundInPos -> Triple(
                PosText.refundItIn(pos).replaceFirstChar(Char::uppercase) + orderOf(sale) + at(sale.refundPendingAt),
                Tone.Warn, StateIcon.Refund,
            )
            Standing.OtherTill -> Triple(
                if (sale.phase.isFinal) "${SaleText.status(sale, till)} · on $other" else "Closed · not paid · refund it on $other",
                Tone.Muted, StateIcon.Close,
            )
        }
        return ShiftRow(
            title = "${SaleText.title(sale)} · ${SaleText.amount(sale)}",
            sub = sub,
            tone = tone,
            quiet = standing == Standing.Refunded,
            icon = icon,
            opens = opens(sale),
        )
    }

    /** " · order 7F3K…": the point of sale's order to refund, by the start of its id, when the till has it. */
    private fun orderOf(sale: TillSale): String = sale.found?.orderId?.let { " · order ${it.take(4)}…" }.orEmpty()

    /** The corners row [index] of rows up to [lastIndex] rounds: the card's top on the first, its bottom on the last. */
    fun ends(index: Int, lastIndex: Int): RowEnds = when {
        index == 0 && lastIndex == 0 -> RowEnds.Only
        index == 0 -> RowEnds.First
        index == lastIndex -> RowEnds.Last
        else -> RowEnds.Middle
    }

    /** What an empty filter says where its rows would be. */
    fun empty(filter: ShiftFilter): String = when (filter) {
        ShiftFilter.All -> "No sale right now"
        ShiftFilter.NeedsAction -> "Nothing needs action"
        ShiftFilter.Paid -> "Nothing paid yet"
    }
}
