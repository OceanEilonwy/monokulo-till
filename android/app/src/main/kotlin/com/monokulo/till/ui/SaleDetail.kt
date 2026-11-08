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
import com.monokulo.till.Layers
import com.monokulo.till.Move
import com.monokulo.till.faceSurface
import com.monokulo.till.GroundLook
import com.monokulo.till.GroundTrack
import com.monokulo.till.groundEnd
import com.monokulo.till.words
import com.monokulo.till.sharedSurface
import uniffi.till_app.Action
import uniffi.till_screens.Fact
import uniffi.till_screens.Ground
import uniffi.till_screens.PageAction

/**
 * A sale's page when its code isn't up (the design's row 2b): paid, paid
 * late, closed, expired, reversed, refunded, or another till's, as the
 * engine wrote it ([SaleFace.page]); a refund's answer turns it into the
 * Refunded page in place, or shows the point of sale's refusal above its
 * buttons. [words] goes on each of its own parts (everything but the
 * hero), so each fades through a screen change by its own place under a
 * moving ground, never the page as one block (decision 233). [card] is the
 * enlarged code's white, which closes into the hero when the sale is paid
 * or ends while its code is enlarged, the hero coming in over it (241).
 */
// [words] is not this page's modifier but its words' (each of its parts takes it), so it is not named or placed as one.
@Suppress("ModifierParameter")
@Composable
fun SaleDetail(
    sale: SaleFace,
    actions: TillActions,
    words: Modifier = Modifier,
    card: GroundTrack? = null,
) {
    val page = sale.page
    // Folded unless the page has room for its order card open.
    val room = rememberReceiptRoom(sale.orderId, live = false)
    // Its ground is the screen's (SaleScreen), so the live sale it turns from fades over one ground.
    Surface(Modifier.fillMaxSize(), color = Color.Transparent, contentColor = mk.ink) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 560.dp).fillMaxSize()) {
                Box(words) { SaleBar(page.title, page.titleLine, sale.orderId, actions) }
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
                                StateBadge(sale.view.ring, page.icon, page.ground, HeroSize, 38.dp)
                            }
                        }
                        Text(
                            page.heading, style = display(26, 0.0), textAlign = TextAlign.Center,
                            color = if (page.headingWarn) mk.warn else mk.ink,
                            modifier = words.semantics { heading(); liveRegion = LiveRegionMode.Polite },
                        )
                        Text(page.amount, style = display(40, -0.8), color = if (page.amountMuted) mk.muted else mk.ink, modifier = words)
                        Text(page.line, fontSize = 15.sp, color = mk.muted, textAlign = TextAlign.Center, modifier = words.offset(y = (-8).dp))
                    }
                    page.note?.let { note ->
                        val danger = note.ground == Ground.DANGER
                        Text(
                            note.text, fontSize = 14.sp, lineHeight = 21.sp, color = if (danger) mk.onDangerGround else mk.ink,
                            modifier = words.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(groundColor(note.ground))
                                .padding(horizontal = 14.dp, vertical = 12.dp),
                        )
                    }
                    page.order?.let { Box(words) { OrderCard(it, room) } }
                    if (page.facts.isNotEmpty()) Box(words) { Facts(page.facts) }
                }
                if (page.buttons.isNotEmpty()) {
                    Column(words.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Notice(page.refundProblem)
                        for (button in page.buttons) {
                            when (button.action) {
                                PageAction.REFUND -> PrimaryButton(
                                    button.press.label, { actions.send(Action.Refund(sale.orderId)) }, Modifier.fillMaxWidth(),
                                    enabled = button.press.enabled, height = 56.dp, size = 16,
                                )
                                PageAction.SHOW_CODE_AGAIN -> OutlineButton(
                                    button.press.label, { actions.send(Action.Reopen(sale.orderId)) }, Modifier.fillMaxWidth(),
                                    enabled = button.press.enabled, height = 52.dp,
                                )
                            }
                        }
                    }
                }
            }
        }
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
