package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.till.Fact
import com.monokulo.till.Ground
import com.monokulo.till.PageAction
import com.monokulo.till.PosText
import com.monokulo.till.SaleHeader
import com.monokulo.till.SalePage
import com.monokulo.till.SaleText
import com.monokulo.till.sales.TillSale
import com.monokulo.till.Layers
import com.monokulo.till.Move
import com.monokulo.till.faceSurface
import com.monokulo.till.GroundLook
import com.monokulo.till.GroundTrack
import com.monokulo.till.groundEnd
import com.monokulo.till.words
import com.monokulo.till.sharedSurface

/**
 * A sale's page when its code isn't up (the design's row 2b): paid, paid
 * late, closed, expired, reversed, refunded, or another till's. What it
 * says comes from [SalePage]; a refund's answer turns it into the
 * Refunded page in place, or shows [refundProblem] above its buttons. At a
 * point of sale the till can't refund through, Refund opens [refundSheet]
 * (decision 130) until the sale shows refunded.
 * [words] goes on each of its own parts (everything but the hero), so
 * each fades through a screen change by its own place under a moving
 * ground, never the page as one block (decision 233). [card] is the
 * enlarged code's white, which closes into the hero when the sale is paid
 * or ends while its code is enlarged, the hero coming in over it (241).
 */
// [words] is not this page's modifier but its words' (each of its parts takes it), so it is not named or placed as one.
@Suppress("ModifierParameter")
@Composable
fun SaleDetail(
    sale: TillSale,
    location: String?,
    refunding: Boolean,
    refundProblem: String?,
    refundSheet: RefundSheet?,
    actions: TillActions,
    words: Modifier = Modifier,
    card: GroundTrack? = null,
) {
    val header = SaleHeader.of(sale, LocalPointOfSale.current, location)
    val pos = header.pos
    val page = SalePage.of(sale, pos, now = System.currentTimeMillis() / 1000, elsewhere = header.elsewhere)
    // Folded unless the page has room for its order card open.
    val room = rememberReceiptRoom(sale.orderId, live = false)
    // The time before the place, so a narrow bar's ellipsis takes "this till" first, not the time.
    val subtitle = sale.createdAt.takeIf { it > 0 }?.let { SaleText.time(it) + " · " }.orEmpty() + header.place
    // Its ground is the screen's (SaleScreen), so the live sale it turns from fades over one ground.
    Surface(Modifier.fillMaxSize(), color = Color.Transparent, contentColor = mk.ink) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 560.dp).fillMaxSize()) {
                Box(words) { SaleBar(sale, subtitle, actions) }
                Column(
                    Modifier.weight(1f).fillMaxWidth().then(room.window).verticalScroll(rememberScrollState()).then(room.content)
                        .padding(start = 20.dp, end = 20.dp, top = 24.dp, bottom = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp),
                ) {
                    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        // The code card closed into this (M10); stepping back to the shift, this becomes the row's badge.
                        // Paid on the enlarged code, the code's white closes into the hero and the hero fades in over it.
                        val overWhite = if (card == null) Modifier else Modifier.words(through = HeroThrough, lift = HeroThrough, on = card)
                        val whiteEnd = if (card == null) Modifier else Modifier.groundEnd(card, Move.CodeGrow, HeroWhite)
                        // From the live card's corners, which the window's width sizes.
                        val share = with(LocalDensity.current) { codeCornerShare(LocalWindowInfo.current.containerSize.width.toDp()) }
                        Box(overWhite.sharedSurface(badgeKey(sale.orderId), Move.RowGrow, CircleShape, layer = Layers.OVER_GROUND)) {
                            Box(whiteEnd.faceSurface(heroKey(sale.orderId), HeroSize / 2, other = share).testTag(HeroTag)) {
                                StateBadge(sale, page.icon, page.ground, HeroSize, 38.dp)
                            }
                        }
                        Text(
                            page.heading, style = display(26, 0.0), textAlign = TextAlign.Center,
                            color = if (page.headingWarn) mk.warn else mk.ink,
                            modifier = words.semantics { heading(); liveRegion = LiveRegionMode.Polite },
                        )
                        Text(SaleText.amount(sale), style = display(40, -0.8), color = if (page.amountMuted) mk.muted else mk.ink, modifier = words)
                        Text(page.line, fontSize = 15.sp, color = mk.muted, textAlign = TextAlign.Center, modifier = words.offset(y = (-8).dp))
                    }
                    page.note?.let { note ->
                        val danger = note.ground == Ground.Danger
                        Text(
                            note.text, fontSize = 14.sp, lineHeight = 21.sp, color = if (danger) mk.onDangerGround else mk.ink,
                            modifier = words.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(groundColor(note.ground))
                                .padding(horizontal = 14.dp, vertical = 12.dp),
                        )
                    }
                    if (page.order) sale.receipt?.let { Box(words) { OrderCard(it, pos, room) } }
                    if (page.facts.isNotEmpty()) Box(words) { Facts(page.facts) }
                }
                if (page.actions.isNotEmpty()) {
                    Column(words.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Notice(refundProblem)
                        for (action in page.actions) {
                            when (action) {
                                PageAction.Refund -> PrimaryButton(
                                    PosText.refundButton(pos, SaleText.amount(sale)), { actions.onRefund(sale.orderId) }, Modifier.fillMaxWidth(),
                                    enabled = !refunding, height = 56.dp, size = 16,
                                )
                                PageAction.ShowCodeAgain -> OutlineButton(
                                    "Show the code again", { actions.onReopen(sale.orderId) }, Modifier.fillMaxWidth(), height = 52.dp,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
    if (refundSheet != null && refundSheet.orderId == sale.orderId && !sale.refunded) {
        RefundInPosSheet(sale, refundSheet, onRefunded = { actions.onRefundDone(sale.orderId) }, onNotYet = actions.onRefundSheetDismissed)
    }
}

/** The move the hero fades in over the code's white by: the code shrinking into it. */
private val HeroThrough = setOf(Move.CodeGrow)

/** The enlarged code's white as it lands on the hero: a white circle the hero's own covers. */
private val HeroWhite = GroundLook(Fixed.White, HeroSize / 2)

/** The hero's tag, for the device tests that look at it mid-move; it has nothing to say to a screen reader (the heading does). */
const val HeroTag = "hero"

/** The timeline card: when it was rung up, seen, closed, paid; another till's sale's till. */
@Composable
private fun Facts(facts: List<Fact>) = Column(
    Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(mk.card).padding(horizontal = 16.dp, vertical = 12.dp),
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    for (fact in facts) {
        Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(fact.label, fontSize = 14.sp, color = mk.muted, modifier = Modifier.weight(1f))
            Text(fact.value, fontSize = 14.sp, textAlign = TextAlign.End)
        }
    }
}
