package com.monokulo.till

import uniffi.till_app.Screen
import com.monokulo.till.core.Amounts
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.Standing
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live
import com.monokulo.till.sales.standing
import java.math.BigDecimal
import java.math.RoundingMode
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Currency
import java.util.Locale

/** How a sale is named and described everywhere it's shown: the app, the bubble and the notification. */
object SaleText {
    /** "Sale #1042" from the POS's receipt number, or "Monero sale" when the POS gave none. */
    fun title(sale: TillSale): String {
        val reference = sale.reference?.trim().orEmpty()
        if (reference.isEmpty()) return "Monero sale"
        return "Sale " + if (reference.startsWith("#")) reference else "#$reference"
    }

    /**
     * A sale page's title: whom it's for when the POS's order names them
     * ([TillSale.customerName]), else [title]. Only the sale's own pages show
     * it: the shift, the bubble and the notifications (on the lock screen)
     * keep [title].
     */
    fun heading(sale: TillSale): String = customer(sale) ?: title(sale)

    /**
     * The line under [heading]: [place], led by the sale's [title] when the
     * heading is the customer's name, so the cashier can still match it to
     * the POS ("Sale #m2Qe · Front Counter · this till").
     */
    fun headingLine(sale: TillSale, place: String): String = if (customer(sale) == null) place else "${title(sale)} · $place"

    private fun customer(sale: TillSale): String? = sale.customerName?.trim()?.takeIf { it.isNotEmpty() }

    /** Where it came from: "Front Counter · this till" for this till's, "From Back till" for another's. */
    fun place(sale: TillSale, location: String?): String = when {
        sale.found != null && location != null -> "$location · this till"
        sale.found != null -> "This till"
        sale.deviceName != null -> "From ${sale.deviceName}"
        else -> "From another till"
    }

    fun amount(sale: TillSale): String = Amounts.fiat(sale.amount, sale.currency)

    /** The exact XMR to pay: rounding it would have the customer underpay. */
    fun xmr(sale: TillSale): String = "${sale.xmr} XMR"

    /**
     * The sale's state in a few words: the shift row's second line and the
     * detail page's heading. Another till's sale says what this till knows
     * (its code ran out, its payment was reversed, it was closed here), never
     * that the other till closed it, which Monokulo doesn't tell us. [till] is
     * the till's point of sale ([PosText.of]).
     */
    fun status(sale: TillSale, till: PointOfSale?): String = when (val standing = sale.standing) {
        is Standing.Live -> when (standing.phase) {
            SalePhase.Partial -> "Part paid"
            SalePhase.Seen -> "Seen on the network"
            SalePhase.Confirming -> confirming(sale)
            else -> "Waiting for payment"
        }
        is Standing.Paid -> when {
            standing.afterRefund -> "Paid after the refund"
            sale.closedAt != null -> "Paid after you closed it"
            standing.late -> "Paid after the code expired"
            else -> "Paid"
        }
        Standing.Closed -> "Closed · not paid"
        Standing.Expired, Standing.Reversed, Standing.OtherTill -> ended(sale.phase)
        Standing.Refunded -> PosText.refundedIn(till)
        Standing.RefundInPos -> "Not paid · " + PosText.refundItIn(till)
    }

    /** "Confirming · 4 of 10": its payment's confirmations of the store's threshold, or "Confirming" until Monokulo says. */
    fun confirming(sale: TillSale): String {
        val have = sale.confirmations ?: return "Confirming"
        val need = sale.confirmationsRequired?.takeIf { it > 0 } ?: return "Confirming"
        return "Confirming · ${have.coerceIn(0, need)} of $need"
    }

    /** How a sale that isn't live ended, or that it was closed here while still waiting. */
    private fun ended(phase: SalePhase): String = when (phase) {
        SalePhase.Expired -> "Code expired · not paid"
        SalePhase.Cancelled -> "Cancelled · not paid"
        SalePhase.DoubleSpend -> "Payment reversed"
        else -> "Closed · not paid"
    }

    /** The status's second line under a live sale's code: what the cashier tells the customer; paid, the XMR it took. */
    fun hint(sale: TillSale, secondsLeft: Long): String = when (sale.phase) {
        SalePhase.Waiting -> "Scan with any Monero wallet · ${clock(secondsLeft)} left"
        SalePhase.Partial -> "Ask the customer to pay the rest to the same code · ${clock(secondsLeft)} left"
        SalePhase.Paid -> "${xmr(sale)} received"
        else -> "The customer can go: it's being confirmed"
    }

    /** Which of the four steps (Waiting, Seen, Confirming, Paid) it's at, from 1; 0 when it ended unpaid. */
    fun step(phase: SalePhase): Int = when (phase) {
        SalePhase.Waiting, SalePhase.Partial -> 1
        SalePhase.Seen -> 2
        SalePhase.Confirming -> 3
        SalePhase.Paid -> 4
        SalePhase.Expired, SalePhase.Cancelled, SalePhase.DoubleSpend -> 0
    }

    /** "1:05" from seconds. */
    fun clock(seconds: Long): String = "${seconds / 60}:${"%02d".format(seconds % 60)}"

    /**
     * "15 items" or "1 item": the receipt's whole quantities added up, and a line sold by measure
     * (2.5 × Granola) one item however much of it, so "2 × Flat white, 2.5 × Granola" is "3 items",
     * what the customer takes away (decision 145).
     */
    fun items(receipt: Receipt): String {
        val n = itemCount(receipt)
        return if (n == 1) "1 item" else "$n items"
    }

    fun itemCount(receipt: Receipt): Int = receipt.lineItems.sumOf { if (it.quantity % 1.0 == 0.0) it.quantity.toInt() else 1 }

    /** "2 × Flat white, Oat latte, 3 × Almond croissant, +5 more": the first [show] lines, and how many more there are. */
    fun firstItems(receipt: Receipt, show: Int = 3): String {
        val lines = receipt.lineItems
        val head = lines.take(show).joinToString(", ") { line(it) }
        val more = lines.size - show
        return if (more > 0) "$head, +$more more" else head
    }

    /** "2 × Flat white"; one of something is just its name. */
    fun line(item: Receipt.LineItem): String =
        if (item.quantity == 1.0) item.description else "${quantity(item.quantity)} × ${item.description}"

    /** "2", or "1.5" for something sold by weight. */
    fun quantity(quantity: Double): String = BigDecimal.valueOf(quantity).stripTrailingZeros().toPlainString()

    /** "9.00": a receipt's amount in the currency's minor digits with no symbol, as the design's order lists have it. */
    fun plain(value: Double, currency: String): String {
        val digits = runCatching { Currency.getInstance(currency.uppercase(Locale.ROOT)).defaultFractionDigits }.getOrDefault(2).coerceAtLeast(0)
        return BigDecimal.valueOf(value).setScale(digits, RoundingMode.HALF_UP).toPlainString()
    }

    /**
     * The lines a sale's order card lists: every line when there are at
     * most [max], else the first `max - 1` and how many more there are (the
     * card's "+5 more", which opens the whole order).
     */
    fun shownLines(receipt: Receipt, max: Int = 4): Pair<List<Receipt.LineItem>, Int> {
        val lines = receipt.lineItems
        return if (lines.size <= max) lines to 0 else lines.take(max - 1) to lines.size - (max - 1)
    }

    /** "Tax 1.13": the receipt's taxes together, or null when it has none. */
    fun tax(receipt: Receipt): String? {
        val lines = receipt.tax?.taxLines.orEmpty()
        if (lines.isEmpty()) return null
        return "Tax " + plain(lines.sumOf { it.taxAmount }, receipt.currency)
    }

    /** "13.88 USD": the receipt's total with the currency's code. */
    fun total(receipt: Receipt): String = "${plain(receipt.total, receipt.currency)} ${receipt.currency}"

    /** "Square receipt bP9m · 15 items · 79.21 USD": a folded order card's one row, whose receipt, how much and for what. */
    fun receiptSummary(receipt: Receipt, pos: PointOfSale?): String =
        PosText.receiptLabel(pos, receipt.receipt.receiptId) + " · " + items(receipt) + " · " + total(receipt)

    /** "4:07 PM" (or "16:07"): a time of day as the till's locale writes it, with a plain space before am or pm. */
    fun time(epochSeconds: Long, locale: Locale = Locale.getDefault(), zone: ZoneId = ZoneId.systemDefault()): String =
        DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale)
            .format(Instant.ofEpochSecond(epochSeconds).atZone(zone))
            .replace(NARROW_SPACE, ' ')

    /** The narrow no-break space newer JDKs put before am or pm. */
    private const val NARROW_SPACE = '\u202F'
}

/**
 * Which surface buzzes when a payment lands (`docs/motion.md` §6): the one
 * the cashier is looking at, and it alone, so a payment is one buzz
 * however many surfaces show it. The app when it is on screen (any of its
 * screens: the sale turning paid, or its row), else the bubble.
 */
object PaidBuzz {
    enum class Surface { App, Bubble }

    fun by(appVisible: Boolean): Surface = if (appVisible) Surface.App else Surface.Bubble

    /** The bubble's side of the rule: whether a payment landing on it buzzes. */
    fun bubble(appVisible: Boolean): Boolean = by(appVisible) == Surface.Bubble
}

/** Which till screen the sales lead to, as they change (decision 70). */
object TillNav {
    /**
     * The screen after the sales changed on [screen]: the shift, while the
     * stage rests on it ([restingOnShift]), brings up a live sale that is
     * new since the last look ([liveBefore]) and not [left]; a sale that
     * came while the cashier was elsewhere (another sale, Settings, the
     * enlarged code, the order page, a Back still landing) waits in the
     * shift's tray and never comes up later, and a closed or ended one
     * never comes up on its own (decision 266). A sale on screen stays
     * whatever it becomes, through Paid, its page following its standing;
     * the enlarged code and the order page are for a live sale only, so
     * they give way to the sale's page once it's paid or ends.
     */
    fun follow(screen: Screen, sales: List<TillSale>, left: Set<String>, liveBefore: Set<String>, restingOnShift: Boolean): Screen = when (screen) {
        Screen.Till -> if (!restingOnShift) screen else {
            sales.firstOrNull { it.live && it.orderId !in left && it.orderId !in liveBefore }?.let { Screen.Sale(it.orderId) } ?: screen
        }
        is Screen.Code -> if (liveOrUnknown(sales, screen.orderId)) screen else Screen.Sale(screen.orderId)
        is Screen.Order -> if (liveOrUnknown(sales, screen.orderId)) screen else Screen.Sale(screen.orderId)
        else -> screen
    }

    /**
     * The sale on [screen] if it was live at the last look ([liveBefore])
     * and is paid now: it was paid while the customer watched, so it steps
     * back to the shift after a few seconds. Opened from the shift once
     * paid, it stays.
     */
    fun paidOnScreen(screen: Screen, sales: List<TillSale>, liveBefore: Set<String>): String? {
        val shown = (screen as? Screen.Sale)?.orderId ?: return null
        val sale = sales.firstOrNull { it.orderId == shown } ?: return null
        return shown.takeIf { it in liveBefore && sale.standing is Standing.Paid }
    }

    /** Whether a payment landed since the last look: a sale that was live then is paid now. */
    fun paymentLanded(sales: List<TillSale>, liveBefore: Set<String>): Boolean =
        sales.any { it.orderId in liveBefore && it.standing is Standing.Paid }

    private fun liveOrUnknown(sales: List<TillSale>, orderId: String): Boolean =
        sales.firstOrNull { it.orderId == orderId }?.live ?: true
}

/**
 * Which sales changed phase since the last look: what the bubble and the
 * notifications react to ("Paid" for a few seconds, an alert on a new sale).
 */
class PhaseChanges {
    private var last = emptyMap<String, SalePhase>()
    private var started = false

    /** The sales in [sales] that are new or in a new phase. The first look reports nothing: those sales aren't news. */
    fun next(sales: List<TillSale>): List<TillSale> {
        val changed = if (started) sales.filter { last[it.orderId] != it.phase } else emptyList()
        last = sales.associate { it.orderId to it.phase }
        started = true
        return changed
    }
}

/** What the bubble shows. */
enum class BubbleLook { Idle, Lit, Paid }

/** What a tap on the bubble does. */
enum class BubbleTap {
    /** No sale to show: Till itself comes to the front, as its launcher icon would bring it. */
    OpenApp,

    /** The panel opens on the bubble's sale. */
    OpenPanel,

    /** The panel goes back into the bubble. */
    LowerPanel,
}

object Bubble {
    /** Green for [com.monokulo.till.ui.Motion.Dwell.PAID_MS] after a sale is paid, then lit while a sale's code is up, else faded. */
    fun look(sales: List<TillSale>, justPaid: Boolean): BubbleLook = when {
        justPaid -> BubbleLook.Paid
        sales.any { it.live } -> BubbleLook.Lit
        else -> BubbleLook.Idle
    }

    /**
     * Which sale the bubble and its panel are about: the one just paid,
     * while it shows as paid; else the newest whose code is up; else the
     * last one they showed.
     */
    fun focus(sales: List<TillSale>, previous: String?, justPaid: Boolean): String? {
        val shown = sales.firstOrNull { it.orderId == previous }
        if (justPaid && shown?.phase == SalePhase.Paid) return previous
        return sales.firstOrNull { it.live }?.orderId ?: previous
    }

    /**
     * What a tap does, with [sale] the one the bubble is about ([focus]) and
     * [panelOpen] whether its panel is up or on its way. The panel only shows
     * a sale, so with none the bubble opens Till instead of a panel that
     * would never appear (and stay "open" for the next tap to lower).
     */
    fun tap(sale: TillSale?, panelOpen: Boolean): BubbleTap = when {
        sale == null -> BubbleTap.OpenApp
        panelOpen -> BubbleTap.LowerPanel
        else -> BubbleTap.OpenPanel
    }

    /** What TalkBack says a tap does ("double-tap to …"). */
    fun tapLabel(tap: BubbleTap): String = when (tap) {
        BubbleTap.OpenApp -> "Open Till"
        BubbleTap.OpenPanel -> "Show the sale"
        BubbleTap.LowerPanel -> "Lower to the bubble"
    }

    /**
     * What TalkBack says for it, as the design's aria labels do (`docs/motion.md`
     * §12.5): the bubble no longer shows a sale's step, so its label carries the
     * amount and the step, in the words the shift and the panel use for it, for
     * [sale], the one it is about ([focus]).
     */
    fun label(look: BubbleLook, sale: TillSale?): String = when {
        look == BubbleLook.Idle -> "Till, nothing waiting"
        sale == null -> if (look == BubbleLook.Paid) "Till, paid" else "Till, a sale waiting for payment"
        look == BubbleLook.Paid -> "Till, ${SaleText.amount(sale)} paid"
        else -> "Till, ${SaleText.amount(sale)} " + (if (sale.live) SaleText.status(sale, null) else "Waiting for payment").replaceFirstChar(Char::lowercase)
    }
}
