package com.monokulo.till.overlay

import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.constrainWidth
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.OrderCount
import com.monokulo.till.ui.OrderRows
import com.monokulo.till.ui.MkIcons
import com.monokulo.till.ui.ReceiptRoom
import com.monokulo.till.ui.RoundIconButton
import com.monokulo.till.ui.mk
import com.monokulo.till.ui.motion
import kotlin.math.roundToInt
import uniffi.till_app.BubbleView
import uniffi.till_screens.OrderCard
import uniffi.till_screens.OrderPageView
import uniffi.till_screens.PanelButton
import uniffi.till_screens.PanelFace
import uniffi.till_screens.PanelHero
import uniffi.till_screens.PanelView
import uniffi.till_screens.Ring

/**
 * The sale the bubble's panel shows, as the panel draws it: the engine's
 * [PanelView] (its sale as the live screen shows it, the header, ring,
 * hero, whole order and ended words) in the shape the panel's parts take.
 * Every word is the engine's.
 */
@Immutable
data class PanelSale(
    val orderId: String,
    val face: PanelFace,
    /** The hero a paid or ended face folds into. */
    val hero: PanelHero,
    /** "Sale #k4Rt", or whom it's for. */
    val title: String,
    /** "4:07 PM", when the sale was rung up; null when that isn't known. */
    val created: String?,
    /** The header's "Front Counter" (or who took it), before "Open the shift". */
    val where: String,
    val amount: String,
    /** "0.05552 XMR". */
    val xmr: String,
    val code: String,
    val codeSaid: String,
    val status: String,
    /** "· 14:52 left" while the code counts down; null once a payment is on its way. */
    val left: String?,
    /** "14:52 left", the same countdown without its dot. */
    val timeLeft: String?,
    /** The phase ring: its step and the payment's confirmations. */
    val ring: Ring,
    /** "0.05552 XMR received", on the paid face. */
    val received: String,
    val endedHeading: String,
    val endedLine: String,
    /** The sale's order card, three lines and "+N more" at most, from this till's receipt. */
    val card: OrderCard?,
    /** The whole order, every line: the panel's order page. */
    val order: OrderPageView?,
    /** The buttons under the face, as the engine lists them. */
    val buttons: List<PanelButton>,
) {
    companion object {
        /** The panel's sale from the engine's [view]; null when it is about no sale. */
        fun of(view: BubbleView): PanelSale? = view.panel?.let(::of)

        /** The panel's sale from the engine's [panel]. */
        fun of(panel: PanelView): PanelSale {
            val sale = panel.sale
            return PanelSale(
                orderId = panel.orderId,
                face = panel.face,
                hero = panel.hero,
                title = sale.title,
                created = panel.created,
                where = panel.whereFrom,
                amount = sale.amount,
                xmr = sale.xmr,
                code = sale.code,
                codeSaid = sale.codeSaid,
                status = sale.status,
                left = panel.statusLeft,
                timeLeft = panel.timeLeft,
                ring = sale.ring,
                received = panel.received.orEmpty(),
                endedHeading = panel.endedHeading.orEmpty(),
                endedLine = panel.endedLine.orEmpty(),
                card = sale.order?.takeIf { it.lines.isNotEmpty() },
                order = panel.order,
                buttons = panel.buttons,
            )
        }
    }
}

private val CardShape = RoundedCornerShape(16.dp)

/**
 * The engine's order card, as the sale screen draws it: its summary row
 * folds the lines open, and "+3 more" opens the whole order. It opens by
 * itself only when the panel has room for it open ([room]); the cashier's
 * tap wins.
 */
@Composable
internal fun PanelOrderCard(card: OrderCard, room: ReceiptRoom, onOpen: (() -> Unit)?) {
    val m = motion
    val tapped = room.tapped
    val unfolded = tapped.value ?: room.fits.value
    val shown = remember(room) { Animatable(if (unfolded) 1f else 0f) }
    LaunchedEffect(shown, unfolded, m) { shown.animateTo(if (unfolded) 1f else 0f, m.settle.fraction) }
    val window = LocalConfiguration.current.screenHeightDp
    val open = onOpen.takeIf { card.more != null }
    Layout(
        content = {
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp)
                    .clickable(
                        onClickLabel = if (unfolded) "Hide the order's lines" else "Show the order's lines",
                        role = Role.Button,
                        onClick = { tapped.value = !unfolded },
                    )
                    .semantics {
                        heading()
                        stateDescription = if (unfolded) "Expanded" else "Collapsed"
                    }
                    .padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(card.summary, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Icon(MkIcons.Fold, contentDescription = null, tint = mk.ink, modifier = Modifier.size(20.dp).graphicsLayer { rotationZ = shown.value * 180f })
            }
            Column(
                Modifier.fillMaxWidth()
                    .then(if (open != null) Modifier.clickable(onClickLabel = "See the whole order", onClick = open) else Modifier)
                    .padding(start = 16.dp, end = 16.dp, bottom = 12.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) { OrderLines(card, 14) }
        },
        modifier = Modifier.fillMaxWidth().clip(CardShape).background(mk.card),
    ) { (row, body), constraints ->
        val head = row.measure(constraints.copy(minHeight = 0))
        val all = body.measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
        val showing = (all.height * shown.value).roundToInt()
        val height = constraints.constrainHeight(head.height + showing)
        layout(constraints.constrainWidth(maxOf(head.width, all.width)), height) {
            room.judge(height, head.height + all.height, window)
            head.place(0, 0)
            if (showing > 0) all.place(0, head.height)
        }
    }
}

/** The card's lines, "+N more", and its foot and total. */
@Composable
private fun OrderLines(card: OrderCard, size: Int) {
    for (line in card.lines) {
        Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(line.label, fontSize = size.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Text(line.price, fontSize = size.sp, color = mk.muted, softWrap = false)
        }
    }
    card.more?.let { Text(it, fontSize = size.sp, color = mk.link) }
    Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
    Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(card.foot, fontSize = size.sp, color = mk.muted, modifier = Modifier.weight(1f))
        Text(card.total, fontSize = size.sp, fontWeight = FontWeight.Bold, softWrap = false)
    }
}

/**
 * The whole order in the panel (the design's PanelE), which has no back
 * gesture, so its two ways back are on screen: the top bar, the count and
 * the lines, which scroll, as the app's own order page draws them
 * ([OrderCount], [OrderRows]), and the bar at the foot that shows the code
 * again.
 */
@Composable
internal fun PanelOrderContent(sale: PanelSale, onBack: () -> Unit, modifier: Modifier = Modifier) = Column(modifier) {
    val order = sale.order
    Row(
        Modifier.fillMaxWidth().height(64.dp).padding(start = 4.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        RoundIconButton(MkIcons.Back, "Back to the code", onBack, iconSize = 22.dp)
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }) {
            Text("The order", fontSize = 20.sp, fontWeight = FontWeight.Medium)
            if (order != null) Text(order.subtitle, fontSize = 13.sp, color = mk.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    if (order == null) {
        Text(
            "What was bought stays on the till that rang it up.", fontSize = 15.sp, color = mk.muted,
            modifier = Modifier.padding(24.dp).weight(1f),
        )
    } else {
        OrderCount(order.items, order.lines)
        Box(Modifier.weight(1f).padding(horizontal = 16.dp).fillMaxWidth().clip(CardShape).background(mk.card)) {
            Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp)) { OrderRows(order) }
        }
    }
    Box(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 20.dp)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 64.dp).clip(RoundedCornerShape(32.dp)).background(mk.accent)
                .clickable(onClick = onBack)
                .clearAndSetSemantics {
                    contentDescription = "Show the code"
                    role = Role.Button
                    onClick { onBack(); true }
                }
                .padding(start = 22.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(Modifier.weight(1f).padding(vertical = 8.dp)) {
                Text("Show the code", fontSize = 16.sp, fontWeight = FontWeight.Bold, color = mk.onAccent)
                Text(
                    "${sale.amount} · ${sale.xmr} · ${sale.timeLeft ?: sale.status}",
                    fontSize = 13.sp, color = mk.onAccent, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
            Box(Modifier.size(48.dp).clip(CircleShape).background(mk.onAccent), contentAlignment = Alignment.Center) {
                Icon(MkIcons.Code, contentDescription = null, tint = Fixed.White, modifier = Modifier.size(22.dp))
            }
        }
    }
}
