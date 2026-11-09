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
import com.monokulo.till.Move
import com.monokulo.till.GroundLook
import androidx.compose.runtime.remember
import com.monokulo.till.groundExcept
import com.monokulo.till.sharedGround
import com.monokulo.till.rememberGroundTrack
import com.monokulo.till.words
import androidx.compose.ui.graphics.Color
import uniffi.till_screens.OrderPageView

/**
 * The whole order, full screen (the design's PanelE), in the app, as the
 * engine wrote it ([SaleFace.order]): opened from the sale's order card.
 * Back ([OrderPageView.back], the engine's words) goes to the sale; "Show the code" ([onShowCode]) goes
 * there too. Android's back is the activity's, which previews the way back
 * to the sale.
 */
@Composable
fun OrderPage(sale: SaleFace, onBack: () -> Unit, onShowCode: () -> Unit) {
    // No ground of its own while it grows out of the sale's card: the sale's is under it, the same colour (decision 185).
    Surface(Modifier.fillMaxSize().groundExcept(Move.OrderGrow, mk.ground), color = Color.Transparent, contentColor = mk.ink) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            // As wide as the app's other pages ([Page]), centred on a tablet or a wide window.
            Column(Modifier.widthIn(max = 560.dp).fillMaxSize()) {
                val order = sale.order
                // The engine's words for the way back and the code's bar (docs/decisions/core.md 107).
                OrderBar(order?.back ?: "Back", onBack, order?.subtitle)
                if (order == null) {
                    // Another till's sale: what was bought stays on that till.
                    Muted("What was bought stays on the till that rang it up.", Modifier.padding(24.dp).weight(1f), size = 15)
                } else {
                    OrderCount(order.items, order.lines, Modifier.words(through = OrderGrowOnly))
                    // The sale's order card's ground grows into this one (M7) and its lines fade in over it.
                    Box(Modifier.weight(1f).padding(horizontal = 16.dp).fillMaxWidth()) {
                        val look = GroundLook(mk.card, CardCorner)
                        val ground = rememberGroundTrack("order/${sale.orderId}")
                        Box(Modifier.matchParentSize().sharedGround("order/${sale.orderId}", Move.OrderGrow, look, ground))
                        Column(Modifier.matchParentSize().words(over = Move.OrderGrow, on = ground).clip(CardShape).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp)) {
                            OrderRows(order)
                        }
                    }
                }
                Box(Modifier.words(through = OrderGrowOnly)) { ShowCodeBar(sale.code.showCode, onShowCode) }
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
fun OrderCount(items: String, lines: String, modifier: Modifier = Modifier) = Row(
    modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 8.dp, bottom = 16.dp),
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
 * Every line of the engine's [order], then its subtotal, extras, taxes and
 * total: the app's order page and the bubble's panel both draw it.
 */
@Composable
fun OrderRows(order: OrderPageView) {
    order.rows.forEach { Line(it.quantity, it.description, it.price) }
    order.sums.forEachIndexed { i, sum -> Total(sum.name, sum.value, Modifier.padding(top = if (i == 0) 12.dp else 4.dp, bottom = 4.dp)) }
    GrandTotal(order.total)
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
