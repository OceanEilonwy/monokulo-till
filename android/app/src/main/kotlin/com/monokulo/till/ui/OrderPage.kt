package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LongState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.till.PosText
import com.monokulo.till.SaleText
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.Move
import com.monokulo.till.GroundLook
import androidx.compose.runtime.remember
import com.monokulo.till.groundExcept
import com.monokulo.till.sharedGround
import com.monokulo.till.rememberGroundTrack
import com.monokulo.till.words
import androidx.compose.ui.graphics.Color
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.TillSale
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonPrimitive
import java.math.BigDecimal
import java.math.RoundingMode
import java.time.OffsetDateTime
import java.time.ZoneId
import java.util.Locale

/** The words and numbers of the whole order (the design's PanelE), from the receipt on this device. */
object OrderText {
    /** "8 lines" or "1 line". */
    fun lines(receipt: Receipt): String = receipt.lineItems.size.let { if (it == 1) "1 line" else "$it lines" }

    /** "2 ×", and "1 ×" too: the order's quantity column. */
    fun quantity(item: Receipt.LineItem): String = "${SaleText.quantity(item.quantity)} ×"

    /** "8.875%" from a tax line's fraction (0.08875), to at most three decimals; "10%" from 0.1. */
    fun rate(fraction: Double): String =
        BigDecimal.valueOf(fraction).movePointRight(2).setScale(3, RoundingMode.HALF_UP).stripTrailingZeros().toPlainString() + "%"

    /** What the lines come to before tip, discount and tax. */
    fun subtotal(receipt: Receipt): Double = receipt.lineItems.sumOf { BigDecimal.valueOf(it.lineTotal) }.toDouble()

    /**
     * The receipt's tip and discount (its `extensions`, as the till wrote
     * them), each a name and its amount as the order shows it: "2.00", or
     * "−1.50" off.
     */
    fun extras(receipt: Receipt): List<Pair<String, String>> {
        val extensions = receipt.extensions ?: return emptyList()
        return listOfNotNull(
            amountOf(extensions, "tip")?.let { "Tip" to SaleText.plain(it, receipt.currency) },
            amountOf(extensions, "discount")?.let { "Discount" to "−" + SaleText.plain(it, receipt.currency) },
        )
    }

    private fun amountOf(extensions: JsonObject, key: String): Double? =
        runCatching { (extensions[key] as? JsonObject)?.get("amount")?.jsonPrimitive?.doubleOrNull }.getOrNull()?.takeIf { it > 0 }

    /** "79.21 USD": the total with the currency's code. */
    fun total(receipt: Receipt): String = "${SaleText.plain(receipt.total, receipt.currency)} ${receipt.currency}"

    /**
     * "Sale #k4Rt · Square receipt k4Rt · 4:07 pm": the sale, the point of
     * sale's receipt when it gave one, and when the receipt was issued.
     */
    fun subtitle(
        sale: TillSale,
        receipt: Receipt,
        pos: PointOfSale?,
        locale: Locale = Locale.getDefault(),
        zone: ZoneId = ZoneId.systemDefault(),
    ): String {
        val issued = runCatching { OffsetDateTime.parse(receipt.receipt.issueTimestamp).toEpochSecond() }.getOrNull()
        return listOfNotNull(
            SaleText.title(sale),
            sale.reference?.let { PosText.receiptLabel(pos, receipt.receipt.receiptId) },
            issued?.let { SaleText.time(it, locale, zone) },
        ).joinToString(" · ")
    }
}

/**
 * The whole order, full screen (the design's PanelE), in the app, as the
 * engine wrote it ([SaleFace.order]): opened from the sale's order card.
 * [backLabel] says where back goes; "Show the code" ([onShowCode]) goes
 * there too. Android's back is the activity's, which previews the way back
 * to the sale.
 */
@Composable
fun OrderPage(sale: SaleFace, backLabel: String, onBack: () -> Unit, onShowCode: () -> Unit) {
    // No ground of its own while it grows out of the sale's card: the sale's is under it, the same colour (decision 185).
    Surface(Modifier.fillMaxSize().groundExcept(Move.OrderGrow, mk.ground), color = Color.Transparent, contentColor = mk.ink) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            // As wide as the app's other pages ([Page]), centred on a tablet or a wide window.
            Column(Modifier.widthIn(max = 560.dp).fillMaxSize()) {
                val order = sale.order
                OrderBar(backLabel, onBack, order?.subtitle)
                if (order == null) {
                    // Another till's sale: what was bought stays on that till.
                    Muted("What was bought stays on the till that rang it up.", Modifier.padding(24.dp).weight(1f), size = 15)
                } else {
                    OrderCount(order.items, order.lines)
                    // The sale's order card's ground grows into this one (M7) and its lines fade in over it.
                    Box(Modifier.weight(1f).padding(horizontal = 16.dp).fillMaxWidth()) {
                        val look = GroundLook(mk.card, CardCorner)
                        val ground = rememberGroundTrack("order/${sale.orderId}")
                        Box(Modifier.matchParentSize().sharedGround("order/${sale.orderId}", Move.OrderGrow, look, ground))
                        Column(Modifier.matchParentSize().words(over = Move.OrderGrow, on = ground).clip(CardShape).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp)) {
                            order.rows.forEach { Line(it.quantity, it.description, it.price) }
                            order.sums.forEachIndexed { i, sum -> Total(sum.name, sum.value, Modifier.padding(top = if (i == 0) 12.dp else 4.dp, bottom = 4.dp)) }
                            GrandTotal(order.total)
                        }
                    }
                }
                Box(Modifier.words(through = OrderGrowOnly)) { ShowCodeBar("${sale.code.amount} · ${sale.code.under}", onShowCode) }
            }
        }
    }
}

/** The order's top bar: back, "The order" and whose order it is. */
@Composable
private fun OrderBar(backLabel: String, onBack: () -> Unit, subtitle: String?) = Row(
    Modifier.fillMaxWidth().words(through = OrderGrowOnly).height(64.dp).padding(start = 4.dp, end = 16.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(4.dp),
) {
    RoundIconButton(MkIcons.Back, backLabel, onBack, iconSize = 22.dp)
    Column(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }) {
        Text("The order", fontSize = 20.sp, fontWeight = FontWeight.Medium)
        if (subtitle != null) Text(subtitle, fontSize = 13.sp, color = mk.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** "15 items" and "8 lines" over the list. */
@Composable
private fun OrderCount(items: String, lines: String) = Row(
    Modifier.fillMaxWidth().words(through = OrderGrowOnly).padding(start = 24.dp, end = 24.dp, top = 8.dp, bottom = 16.dp),
    verticalAlignment = Alignment.Bottom,
) {
    Text(items, style = display(34, -0.6), modifier = Modifier.weight(1f).alignByBaseline())
    Text(lines, fontSize = 15.sp, color = mk.muted, modifier = Modifier.alignByBaseline())
}

@Composable
private fun GrandTotal(total: String) = Row(Modifier.fillMaxWidth().padding(top = 6.dp, bottom = 12.dp).semantics(mergeDescendants = true) {}) {
    Text("Total", fontSize = 18.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
    Text(total, fontSize = 18.sp, fontWeight = FontWeight.Bold)
}

/**
 * The order's content, wherever it's shown: the app's [OrderPage], or the
 * overlay's panel (which has no back gesture, so its two ways back are on
 * screen). The top bar, the count and the bottom bar stay; the lines
 * scroll. The bar's countdown ticks on [now], the screen's clock. Everything comes from the sale's receipt, on this device.
 */
@Composable
fun OrderContent(
    sale: TillSale,
    backLabel: String,
    onBack: () -> Unit,
    onShowCode: () -> Unit,
    modifier: Modifier = Modifier,
    now: LongState = rememberNowSeconds(),
) = Column(modifier) {
    val receipt = sale.receipt
    // In the app the page's own parts fade in once the sale's have gone, at their own size, while the card grows.
    Row(
        Modifier.fillMaxWidth().words(through = OrderGrowOnly).height(64.dp).padding(start = 4.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        RoundIconButton(MkIcons.Back, backLabel, onBack, iconSize = 22.dp)
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }) {
            Text("The order", fontSize = 20.sp, fontWeight = FontWeight.Medium)
            if (receipt != null) {
                Text(OrderText.subtitle(sale, receipt, posOf(sale)), fontSize = 13.sp, color = mk.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
    if (receipt == null) {
        // Another till's sale: what was bought stays on that till.
        Muted("What was bought stays on the till that rang it up.", Modifier.padding(24.dp).weight(1f), size = 15)
    } else {
        OrderCount(SaleText.items(receipt), OrderText.lines(receipt))
        // In the app, the sale's order card's ground grows into this one (M7) and its lines fade in over it; in the panel
        // there's no stage and it's a card.
        Box(Modifier.weight(1f).padding(horizontal = 16.dp).fillMaxWidth()) {
            val look = GroundLook(mk.card, CardCorner)
            val ground = rememberGroundTrack("order/${sale.orderId}")
            Box(Modifier.matchParentSize().sharedGround("order/${sale.orderId}", Move.OrderGrow, look, ground))
            OrderList(receipt, Modifier.matchParentSize().words(over = Move.OrderGrow, on = ground).clip(CardShape))
        }
    }
    Box(Modifier.words(through = OrderGrowOnly)) {
        val pos = posOf(sale)
        val seconds = secondsLeft(sale.expiresAt, now.longValue)
        ShowCodeBar("${SaleText.amount(sale)} · ${CodeText.under(sale, pos, seconds)}", onShowCode)
    }
}

/** Every line of the order, its subtotal, extras, tax and total; they scroll. */
@Composable
private fun OrderList(receipt: Receipt, modifier: Modifier) {
    Column(modifier.verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp)) {
        receipt.lineItems.forEach { item -> Line(OrderText.quantity(item), item.description, SaleText.plain(item.lineTotal, receipt.currency)) }
        Total("Subtotal", SaleText.plain(OrderText.subtotal(receipt), receipt.currency), Modifier.padding(top = 12.dp, bottom = 4.dp))
        OrderText.extras(receipt).forEach { (name, value) -> Total(name, value, Modifier.padding(vertical = 4.dp)) }
        receipt.tax?.taxLines.orEmpty().forEach { tax ->
            Total("Tax ${OrderText.rate(tax.rate)}", SaleText.plain(tax.taxAmount, receipt.currency), Modifier.padding(vertical = 4.dp))
        }
        GrandTotal(OrderText.total(receipt))
    }
}

/** One line of the order: "2 ×", what it is (two lines at most), and its total, which never wraps. */
@Composable
private fun Line(quantity: String, description: String, price: String) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 11.dp).semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // Never wraps: "0.5 ×" and "12 ×" stay on one line at any font scale.
        Text(
            quantity, fontSize = 16.sp, color = mk.muted, maxLines = 1, softWrap = false,
            modifier = Modifier.widthIn(min = 36.dp),
        )
        Text(description, fontSize = 16.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        Text(price, fontSize = 16.sp, maxLines = 1, softWrap = false)
    }
    Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
}

@Composable
private fun Total(name: String, value: String, modifier: Modifier) = Row(modifier.fillMaxWidth().semantics(mergeDescendants = true) {}) {
    Text(name, fontSize = 15.sp, color = mk.muted, modifier = Modifier.weight(1f))
    Text(value, fontSize = 15.sp, color = mk.muted)
}

/** The orange bar at the foot: "Show the code", the amount and the time left, and the code's glyph. */
@Composable
private fun ShowCodeBar(line: String, onShowCode: () -> Unit) = Box(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 20.dp)) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).clip(RoundedCornerShape(32.dp)).background(mk.accent)
            .clickable(onClick = onShowCode)
            .clearAndSetSemantics {
                contentDescription = "Show the code"
                role = Role.Button
                onClick { onShowCode(); true }
            }
            .padding(start = 22.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f).padding(vertical = 8.dp)) {
            Text("Show the code", fontSize = 16.sp, fontWeight = FontWeight.Bold, color = mk.onAccent)
            Text(line, fontSize = 13.sp, color = mk.onAccent, maxLines = 1, overflow = TextOverflow.Ellipsis, style = LocalTextStyle.current.merge(tabular))
        }
        Box(Modifier.size(48.dp).clip(CircleShape).background(mk.onAccent), contentAlignment = Alignment.Center) {
            Icon(MkIcons.Code, contentDescription = null, tint = Fixed.White, modifier = Modifier.size(22.dp))
        }
    }
}

/** The page's own words fade through its grow, and nothing else. */
private val OrderGrowOnly = setOf(Move.OrderGrow)
