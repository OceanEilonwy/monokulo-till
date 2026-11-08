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
import com.monokulo.till.core.SalePhase
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.MkIcons
import com.monokulo.till.ui.ReceiptRoom
import com.monokulo.till.ui.RoundIconButton
import com.monokulo.till.ui.mk
import com.monokulo.till.ui.motion
import kotlin.math.roundToInt
import uniffi.till_app.BubbleView
import uniffi.till_screens.OrderCard

/** What a folded panel's hero shows: the paid tick, or where an ended sale's code was, by how it ended. */
enum class Hero { Paid, Refunded, Reversed, Ended }

/**
 * The sale the bubble's panel shows, as the panel draws it: the engine's
 * [BubbleView] and its panel's [uniffi.till_screens.SaleView], in the
 * shape the panel's parts take. Every word is the engine's.
 */
@Immutable
data class PanelSale(
    val orderId: String,
    val face: PanelFace,
    /** The hero a paid or ended face folds into. */
    val hero: Hero,
    /** "Sale #k4Rt", or whom it's for. */
    val title: String,
    val amount: String,
    /** "0.05552 XMR". */
    val xmr: String,
    val code: String,
    val codeSaid: String,
    val status: String,
    /** "· 14:52 left" while the code counts down; null once a payment is on its way. */
    val left: String?,
    /** Which of the four steps the ring is at: 1 waiting, 4 paid, 0 ended. */
    val step: Int,
    /** "0.05552 XMR received", once paid. */
    val received: String,
    val endedHeading: String,
    val endedLine: String,
    /** What was bought, from this till's receipt. */
    val order: OrderCard?,
    val copyAddress: String,
) {
    /** The ring's phase for the shared ring and bubble, from its step. */
    val phase: SalePhase
        get() = when (step) {
            1 -> SalePhase.Waiting
            2 -> SalePhase.Seen
            3 -> SalePhase.Confirming
            4 -> SalePhase.Paid
            else -> SalePhase.Expired
        }

    companion object {
        /**
         * The panel's sale from the engine's [view]; null when it is about no sale.
         * A live code's hint ends with its time left ("· 14:52 left"), which the
         * status row shows beside the status.
         */
        fun of(view: BubbleView): PanelSale? {
            val panel = view.panel ?: return null
            val ended = view.endedHeading != null
            val face = when {
                ended -> PanelFace.Ended
                panel.spent -> PanelFace.Paid
                else -> PanelFace.Code
            }
            val step = panel.step.toInt()
            return PanelSale(
                orderId = view.orderId ?: return null,
                face = face,
                hero = when {
                    face == PanelFace.Paid -> Hero.Paid
                    view.endedLine == REFUNDED_LINE -> Hero.Refunded
                    view.endedHeading?.startsWith(REVERSED_HEADING) == true -> Hero.Reversed
                    else -> Hero.Ended
                },
                title = panel.title,
                amount = panel.amount,
                xmr = panel.xmr,
                code = panel.code,
                codeSaid = panel.codeSaid,
                status = panel.status,
                left = if (face == PanelFace.Code && step == 1 && " · " in panel.hint) "· " + panel.hint.substringAfterLast(" · ") else null,
                step = step,
                received = if (face == PanelFace.Paid) panel.hint else "",
                endedHeading = view.endedHeading.orEmpty(),
                endedLine = view.endedLine.orEmpty(),
                order = panel.order?.takeIf { it.lines.isNotEmpty() },
                copyAddress = panel.copyAddress.label,
            )
        }

        /** `till_screens::bubble::panel_ended_line` for a refunded sale. */
        private const val REFUNDED_LINE = "Nothing more to do."

        /** `panel_ended_heading` for a payment reversed. */
        private const val REVERSED_HEADING = "Payment reversed"
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
 * gesture, so its two ways back are on screen: the top bar, the lines,
 * which scroll, and the bar at the foot that shows the code again.
 */
@Composable
internal fun PanelOrderContent(sale: PanelSale, onBack: () -> Unit, modifier: Modifier = Modifier) = Column(modifier) {
    val card = sale.order
    Row(
        Modifier.fillMaxWidth().height(64.dp).padding(start = 4.dp, end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        RoundIconButton(MkIcons.Back, "Back to the code", onBack, iconSize = 22.dp)
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }) {
            Text("The order", fontSize = 20.sp, fontWeight = FontWeight.Medium)
            if (card != null) Text(card.summary, fontSize = 13.sp, color = mk.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    if (card == null) {
        Text(
            "What was bought stays on the till that rang it up.", fontSize = 15.sp, color = mk.muted,
            modifier = Modifier.padding(24.dp).weight(1f),
        )
    } else {
        Box(Modifier.weight(1f).padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().clip(CardShape).background(mk.card)) {
            Column(
                Modifier.verticalScroll(rememberScrollState()).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) { OrderLines(card, 16) }
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
                    "${sale.amount} · ${sale.xmr} · ${sale.left?.removePrefix("· ") ?: sale.status}",
                    fontSize = 13.sp, color = mk.onAccent, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
            Box(Modifier.size(48.dp).clip(CircleShape).background(mk.onAccent), contentAlignment = Alignment.Center) {
                Icon(MkIcons.Code, contentDescription = null, tint = Fixed.White, modifier = Modifier.size(22.dp))
            }
        }
    }
}

