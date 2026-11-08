package com.monokulo.till.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.LongState
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.compose.ui.unit.sp
import com.monokulo.till.CloseChoice
import com.monokulo.till.Closing
import com.monokulo.till.Ground
import com.monokulo.till.PosText
import com.monokulo.till.SaleHeader
import com.monokulo.till.Move
import com.monokulo.till.SaleText
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.StateIcon
import com.monokulo.till.Tone
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live
import com.monokulo.till.LocalFace
import com.monokulo.till.faceSurface
import com.monokulo.till.wholeOn
import com.monokulo.till.GroundLook
import com.monokulo.till.GroundTrack
import com.monokulo.till.sharedGround
import com.monokulo.till.words
import com.monokulo.till.rememberGroundTrack
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.constrainHeight
import androidx.compose.ui.unit.constrainWidth
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.takeWhile

/**
 * The till's point of sale, for every word on its screens that names it
 * ([PosText]): provided where the app's and the bubble's screens are set, null
 * for a till that runs without one.
 */
val LocalPointOfSale = compositionLocalOf<PointOfSale?> { null }

/** The point of sale that names [sale]'s words: the one that took it, else the till's ([PosText.of]). */
@Composable
fun posOf(sale: TillSale): PointOfSale? = PosText.of(sale, LocalPointOfSale.current)

/** Seconds from [now] until [expiresAt], Unix seconds both; 0 once it's passed. */
fun secondsLeft(expiresAt: Long, now: Long): Long = (expiresAt - now).coerceAtLeast(0)

/**
 * The time now, Unix seconds, ticking on each second's boundary: one clock
 * for a whole screen (`docs/motion.md` amendment 10), made at its root and
 * handed down, and read only by the [Countdown]s that show it, so a tick
 * recomposes those texts and nothing above them. A [LongState], so reading
 * it doesn't box.
 */
@Composable
fun rememberNowSeconds(): LongState {
    val now = remember { mutableLongStateOf(System.currentTimeMillis() / 1000) }
    LaunchedEffect(now) {
        while (true) {
            val ms = System.currentTimeMillis()
            now.longValue = ms / 1000
            delay(1000 - ms % 1000)
        }
    }
    return now
}

/**
 * A line with a countdown in it: [words] for the seconds left until
 * [expiresAt] at [now], in [tabular] figures so the digits tick without
 * the words moving. The clock is read here, so a tick recomposes this text
 * alone.
 */
@Composable
fun Countdown(
    now: LongState,
    expiresAt: Long,
    modifier: Modifier = Modifier,
    fontSize: TextUnit = TextUnit.Unspecified,
    color: Color = Color.Unspecified,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
    words: (secondsLeft: Long) -> String,
) = Text(
    words(secondsLeft(expiresAt, now.longValue)), modifier, color = color, fontSize = fontSize,
    overflow = overflow, maxLines = maxLines, style = LocalTextStyle.current.merge(tabular),
)

/** What the till's screens need to know about the shift. */
data class TillStatus(
    /** The store's name as last read; null until the first read answers. */
    val store: String?,
    val location: String?,
    val running: Boolean,
    val live: Boolean,
    val disconnected: Boolean,
    val problem: String?,
    val signInAgain: Boolean,
    /** When this shift began, Unix seconds, if it's known. */
    val startedAt: Long?,
)

/**
 * What the till's screens can do: events up to the view model. Made once
 * per window mode and never changed, so a screen handed it can skip.
 */
@Immutable
class TillActions(
    val onSettings: () -> Unit,
    /** A sale from the shift: its code, or its page. */
    val onOpen: (String) -> Unit,
    /** Back to the shift: the sale stays live, on the bubble and in the shift. */
    val onBack: (String) -> Unit,
    val onEnlarge: (String) -> Unit,
    val onShowOrder: (String) -> Unit,
    val onClose: (String, CloseChoice) -> Unit,
    val onRefund: (String) -> Unit,
    val onReopen: (String) -> Unit,
    val onCopy: (TillSale) -> Unit,
    val onCopyXmr: (TillSale) -> Unit,
    /**
     * Back to the point-of-sale app on this phone: Square's only, null for Clover, whose
     * point of sale is a device of its own (decision 135), and when Square is already beside
     * Till (split screen).
     */
    val onBackToPos: (() -> Unit)?,
    /** The refund sheet's "I've refunded it" (decision 130): ask the point of sale now. */
    val onRefundDone: (String) -> Unit,
    /** The refund sheet went away: "Not yet", Back or the scrim. */
    val onRefundSheetDismissed: () -> Unit,
    val onSignIn: () -> Unit,
    val onForget: () -> Unit,
    val onStartShift: () -> Unit,
    /** The point of sale the till's source watches ([RunnerState.source]), for the close sheet's "No" ([Closing.refundsThere]). */
    val source: PointOfSale = PointOfSale.Square,
)

/**
 * One sale (decision 70): its code for the customer while it's live
 * (SaleA), else the page its standing calls for (row 2b), so a sale that's
 * paid or ends while on screen turns into its page in place. [now] is
 * the screen's clock, read only by its countdowns.
 */
@Composable
fun SaleScreen(
    sale: TillSale,
    location: String?,
    refunding: Boolean,
    refundProblem: String?,
    refundSheet: RefundSheet?,
    actions: TillActions,
    now: LongState = rememberNowSeconds(),
) {
    // The sale is its row opened: tapped on the shift, the row's card grows into this ground (M7), which the row draws
    // since only it knows its own corners; the sale's words fade in over it at their own size.
    Box(Modifier.fillMaxSize()) {
        val ground = rememberGroundTrack("sale/${sale.orderId}")
        Box(Modifier.matchParentSize().sharedGround("sale/${sale.orderId}", Move.RowGrow, GroundLook(mk.ground, 0.dp), ground, fadesIn = Move.Rise))
        // Paid or ended on screen, the live sale turns into its page in place (M10): the code card closes into the
        // page's hero circle, and the live sale's words fade out before the page's fade in, never two receipts at once
        // (decision 188); the way back the same reversed ("Show the code again"). Per draw, never an offscreen layer.
        val m = motion
        AnimatedContent(
            targetState = sale.live,
            // Over the row's ground as it opens, clipped to it; the words inside fade one by one around the code, which
            // never fades (decision 205): opening, it is drawn whole wherever the ground has reached.
            modifier = Modifier.words(over = Move.RowGrow, on = ground, fades = false),
            transitionSpec = { (EnterTransition.None togetherWith ExitTransition.KeepUntilTransitionsFinished).using(null) },
            label = "sale face",
        ) { live ->
            val seen = transition.animateFloat(
                { if (targetState == EnterExitState.Visible) m.fadeThrough.fraction else m.fadeQuick.fraction }, label = "sale face seen",
            ) { if (it == EnterExitState.Visible) 1f else 0f }
            CompositionLocalProvider(LocalFace provides this) {
                Box(
                    Modifier.graphicsLayer {
                        alpha = seen.value
                        compositingStrategy = CompositingStrategy.ModulateAlpha
                    },
                ) {
                    if (live) {
                        LiveSale(sale, location, now, actions, ground)
                    } else {
                        // The enlarged code's white, when the sale was paid or ended on it: the page's words wait for it to pass.
                        val card = rememberGroundTrack(codeGround(sale.orderId))
                        SaleDetail(
                            sale, location, refunding, refundProblem, refundSheet, actions,
                            Modifier.words(through = SaleThrough, lift = SaleLift, on = ground, clear = card), card,
                        )
                    }
                }
            }
        }
    }
}

/** The moves a sale's words fade through: its row opening, its code or order growing, the sale rising by itself. */
private val SaleThrough = setOf(Move.RowGrow, Move.CodeGrow, Move.OrderGrow, Move.Rise)

/** The moves whose ground passes over a sale's words as they arrive: they are drawn over it (decision 204). */
private val SaleLift = setOf(Move.CodeGrow, Move.OrderGrow)

/** The order card's: not its own grow, whose ground it is (its lines fade on it). */
private val OrderCardThrough = setOf(Move.RowGrow, Move.CodeGrow, Move.Rise)
private val OrderCardLift = setOf(Move.CodeGrow)

/** A circle's corners, as a share of its side. */
private const val Circle = 0.5f

/** A live sale full screen, for the customer to scan (SaleA); split in two on a wide window (decision 45). */
@Composable
private fun LiveSale(sale: TillSale, location: String?, now: LongState, actions: TillActions, ground: GroundTrack) {
    // The code card's white: words it still passes over as it shrinks back into the card wait for it to pass (decision 204).
    val card = rememberGroundTrack(codeGround(sale.orderId))
    val words = Modifier.words(through = SaleThrough, lift = SaleLift, on = ground, clear = card)
    val orderWords = Modifier.words(through = OrderCardThrough, lift = OrderCardLift, on = ground, clear = card)
    var asking by rememberSaveable(sale.orderId) { mutableStateOf(false) }
    val header = SaleHeader.of(sale, LocalPointOfSale.current, location)
    // A receipt reaches the till about a second after its sale: its card fades in then, rather than popping (N3).
    val late = remember(sale.orderId) { sale.receipt == null }
    // Folded unless the page has room for its order card open.
    val room = rememberReceiptRoom(sale.orderId, live = true)
    // On a wide window, whether the late card has settled into the column beside the code (or there was none to wait for).
    var grown by remember(sale.orderId) { mutableStateOf(!late) }
    KeepScreenOn()
    // Its ground is the screen's (SaleScreen), so the page it turns into fades over one ground, not a second.
    Surface(Modifier.fillMaxSize(), color = Color.Transparent, contentColor = mk.ink) {
        BoxWithConstraints(Modifier.fillMaxSize().safeDrawingPadding()) {
            val wide = maxWidth >= 840.dp
            Column(Modifier.fillMaxSize()) {
                Box(words) {
                    SaleBar(
                        sale, header.place, actions,
                        onClose = { asking = true }.takeIf { Closing.choices(sale).isNotEmpty() },
                    )
                }
                if (wide) {
                    Row(
                        Modifier.weight(1f).fillMaxWidth().padding(horizontal = 32.dp),
                        horizontalArrangement = Arrangement.spacedBy(48.dp, Alignment.CenterHorizontally),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Code(sale, min(this@BoxWithConstraints.maxHeight - 120.dp, 440.dp), card, ground) { actions.onEnlarge(sale.orderId) }
                        Column(
                            // Centred beside the code (which is not in it), it settles to the late card's height rather than
                            // jumping; then it lets go, so it never chases (nor clips the buttons under) the card's own fold.
                            Modifier.widthIn(max = 420.dp)
                                .then(if (grown) Modifier else Modifier.animateContentSize(motion.settle.spec(IntSize.VisibilityThreshold)) { _, _ -> if (sale.receipt != null) grown = true })
                                .then(room.window).verticalScroll(rememberScrollState()).then(room.content),
                            verticalArrangement = Arrangement.spacedBy(20.dp),
                            horizontalAlignment = Alignment.Start,
                        ) {
                            Box(words) { Amount(sale, actions.onCopyXmr, start = true) }
                            Box(words) { StatusPill(sale, now) }
                            sale.receipt?.let { Box(orderWords) { Late(late) { OrderCard(it, header.pos, room, onOpen = { actions.onShowOrder(sale.orderId) }, shared = "order/${sale.orderId}") } } }
                            Box(words) { Buttons(sale, header, actions) }
                        }
                    }
                } else {
                    // As wide as the detail pages at most, so on a 600-839 dp window
                    // the order card and buttons don't stretch edge to edge.
                    Column(
                        Modifier.weight(1f).align(Alignment.CenterHorizontally).widthIn(max = PhoneMaxWidth).fillMaxWidth()
                            .then(room.window).verticalScroll(rememberScrollState()).then(room.content)
                            .padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 12.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Box(words) { Amount(sale, actions.onCopyXmr) }
                        // The status is the code card's foot (board 8): one white card, the code its own surface above it.
                        val side = codeSide(this@BoxWithConstraints.maxWidth)
                        Column(Modifier.width(side)) {
                            Code(sale, side, card, ground, footed = true) { actions.onEnlarge(sale.orderId) }
                            Box(words) { StatusFoot(sale, now) }
                        }
                        sale.receipt?.let { Box(orderWords) { Late(late) { OrderCard(it, header.pos, room, onOpen = { actions.onShowOrder(sale.orderId) }, shared = "order/${sale.orderId}") } } }
                    }
                    Box(
                        Modifier.align(Alignment.CenterHorizontally).widthIn(max = PhoneMaxWidth)
                            .padding(start = 20.dp, end = 20.dp, bottom = 16.dp).then(words),
                    ) { Buttons(sale, header, actions) }
                }
            }
        }
    }
    if (asking) {
        CloseSheet(sale, actions.source, now, onChoice = { actions.onClose(sale.orderId, it) }, onDismiss = { asking = false })
    }
}

/**
 * [content], faded in with [Motion.fade] as it first shows when it is
 * [late] (it wasn't there when the screen came up), at once when it was:
 * the alpha read per draw, so the fade recomposes nothing and makes no
 * layer.
 */
@Composable
private fun Late(late: Boolean, content: @Composable () -> Unit) {
    val m = motion
    val seen = remember { Animatable(if (late) 0f else 1f) }
    LaunchedEffect(seen) { seen.animateTo(1f, m.fade.fraction) }
    Box(
        Modifier.graphicsLayer {
            alpha = seen.value
            compositingStrategy = CompositingStrategy.ModulateAlpha
        },
    ) { content() }
}

/** A card's corners: the order card, the order page's list. */
internal val CardCorner = 20.dp
internal val CardShape = RoundedCornerShape(CardCorner)

/** The sale's code on a phone at most: the design's 372 dp, a 412 dp phone less 20 dp a side (board 8). */
private val CodeSide = 372.dp

/** The code card's side on a phone [maxWidth] wide: the window less 20 dp a side, up to [CodeSide]. */
internal fun codeSide(maxWidth: Dp): Dp = min(maxWidth - 40.dp, CodeSide)

/** The code card's quiet zone, round the code inside its white. */
private val CodeQuiet = 18.dp

/** How much of the code is left once its sale is paid: the web checkout's spent code, so no one scans it again. */
internal const val SpentCode = 0.14f

private val CodeShape = RoundedCornerShape(CodeCorner)

/** A footed code card's: its foot has the card's lower corners ([FootShape]). */
private val CodeTopShape = RoundedCornerShape(topStart = CodeCorner, topEnd = CodeCorner)
private val FootShape = RoundedCornerShape(bottomStart = CodeCorner, bottomEnd = CodeCorner)

/**
 * The code card's corners as a share of its side on a phone window [width]
 * wide: what the hero's circle opens from, and closes back into. Exact for
 * the phone's card at any width; a wide window's card, sized by its height,
 * opens from about the cap's share, as does a window not yet measured.
 */
internal fun codeCornerShare(width: Dp): Float = CodeCorner / (codeSide(width).takeIf { it > CodeCorner * 2 } ?: CodeSide)

/** The phone layout's widest, the detail pages' (SaleDetail). */
private val PhoneMaxWidth = 560.dp

/**
 * A sale's top bar: Back to the shift, the sale ([SaleText.heading]) and [subtitle], Settings,
 * and on a live sale the tinted X that asks whether the customer is paying.
 */
@Composable
fun SaleBar(sale: TillSale, subtitle: String, actions: TillActions, onClose: (() -> Unit)? = null) = Row(
    Modifier.fillMaxWidth().height(64.dp).padding(start = 4.dp, end = 12.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(4.dp),
) {
    RoundIconButton(MkIcons.Back, "Back to the shift", { actions.onBack(sale.orderId) }, iconSize = 22.dp)
    Column(Modifier.weight(1f).semantics(mergeDescendants = true) { heading() }) {
        Text(SaleText.heading(sale), fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        // The till's own words for where the sale is from replace Square's once its receipt is in: they fade through.
        val through = motion.fadeThrough.fraction
        val quick = motion.fadeQuick.fraction
        AnimatedContent(SaleText.headingLine(sale, subtitle), transitionSpec = { (fadeIn(through) togetherWith fadeOut(quick)).using(null) }, label = "sale from") { words ->
            Text(words, fontSize = 13.sp, color = mk.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    CircleButton(MkIcons.Settings, "Settings", actions.onSettings, ground = mk.chip, tint = mk.ink, iconSize = 20.dp)
    if (onClose != null) {
        Spacer(Modifier.width(4.dp))
        CircleButton(MkIcons.Close, "Close sale", onClose, ground = mk.dangerGround, tint = mk.danger, iconSize = 16.dp)
    }
}

/** The amount, and the XMR to pay under it, which copies itself when tapped ([onCopyXmr]; acceptance D7). */
@Composable
private fun Amount(sale: TillSale, onCopyXmr: (TillSale) -> Unit, start: Boolean = false) = Column(
    horizontalAlignment = if (start) Alignment.Start else Alignment.CenterHorizontally,
) {
    val align = if (start) TextAlign.Start else TextAlign.Center
    Text(SaleText.amount(sale), style = display(52, -1.0), textAlign = align)
    Text(
        SaleText.xmr(sale), fontSize = 17.sp, color = mk.muted, textAlign = align,
        modifier = Modifier.clip(RoundedCornerShape(8.dp))
            .clickable(onClickLabel = "Copy the XMR amount") { onCopyXmr(sale) }
            .padding(horizontal = 6.dp, vertical = 2.dp),
    )
}

/**
 * The code, which enlarges when tapped (CodeB), with the expand badge on
 * its corner. [footed], the card's foot is under it ([StatusFoot]): its
 * own lower corners are square, its white filled out to them, so the two
 * are one card while the code keeps its own bounds for the enlarge and the
 * paid fold. Paid, the code fades to [SpentCode] under a white veil on
 * [Motion.fade], read per draw.
 */
@Composable
private fun Code(sale: TillSale, side: Dp, card: GroundTrack, ground: GroundTrack, footed: Boolean = false, onEnlarge: () -> Unit) {
    val veil by animateFloatAsState(if (sale.phase == SalePhase.Paid) 1f - SpentCode else 0f, motion.fade.fraction, label = "spent code")
    Box(
        // Opening from its row or shrinking back into it, the code and its white are drawn whole or not at all: only on
        // the frames the row's ground holds all of the card (decision 234). Paid or ended, the card closes into the page's
        // hero (M10).
        Modifier.wholeOn(ground, Move.RowGrow)
            .faceSurface(heroKey(sale.orderId), CodeCorner, other = Circle, code = true, bottom = if (footed) 0.dp else CodeCorner)
            .clip(if (footed) CodeTopShape else CodeShape)
            .then(if (footed) Modifier.drawBehind { drawRect(Fixed.White, Offset(0f, size.height - CodeCorner.toPx()), Size(size.width, CodeCorner.toPx())) } else Modifier)
            .clickable(onClickLabel = "Enlarge the payment code", role = Role.Button, onClick = onEnlarge),
    ) {
        QrCode(
            sale.code.uri,
            "Payment code for ${SaleText.amount(sale)}" + (if (sale.code.hasReceipt) ", with the receipt in it" else "") +
                if (sale.phase == SalePhase.Paid) ", paid: no need to scan it" else "",
            Modifier.width(side),
            padding = CodeQuiet,
            radius = CodeCorner,
            shared = sale.orderId,
            ground = card,
        )
        // Ink at 14 % on white is white at 86 % over the ink: the same pixels, with no layer on the code and its white kept.
        Box(Modifier.matchParentSize().padding(CodeQuiet).drawBehind { if (veil > 0f) drawRect(Fixed.White, alpha = veil) })
        Box(
            Modifier.align(Alignment.BottomEnd).padding(10.dp).size(32.dp).clip(CircleShape).background(Fixed.Ink),
            contentAlignment = Alignment.Center,
        ) { Icon(MkIcons.Expand, contentDescription = null, tint = Fixed.White, modifier = Modifier.size(16.dp)) }
    }
}

/**
 * "Waiting for payment" with the phase ring, on its pill beside the code
 * on a wide window; the words, not the countdown, are announced. New words
 * crossfade over the old and the pill settles to their width (M12): a
 * two-text subtree, never anything with the code in it.
 */
@Composable
internal fun StatusPill(sale: TillSale, now: LongState) = StatusLine(
    sale, now,
    Modifier.clip(RoundedCornerShape(28.dp)).background(mk.card)
        .animateContentSize(motion.settle.spec(IntSize.VisibilityThreshold))
        .padding(start = 12.dp, end = 18.dp, top = 10.dp, bottom = 10.dp),
)

/**
 * The status as the code card's foot on a phone (board 8): under a line
 * across the card, white in every state and theme as the code's card is,
 * so its words and ring take the light theme's roles; it settles to the
 * words' height as they change.
 */
@Composable
internal fun StatusFoot(sale: TillSale, now: LongState) = CompositionLocalProvider(LocalTokens provides LightTokens, LocalContentColor provides LightTokens.ink) {
    Column(Modifier.fillMaxWidth().background(Fixed.White, FootShape)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
        StatusLine(
            sale, now,
            Modifier.fillMaxWidth().animateContentSize(motion.settle.spec(IntSize.VisibilityThreshold))
                .padding(start = 14.dp, end = 18.dp, top = 12.dp, bottom = 14.dp),
        )
    }
}

/** The ring, the tick in it once paid, and the status words over the hint: the pill's and the foot's. */
@Composable
private fun StatusLine(sale: TillSale, now: LongState, modifier: Modifier) = Row(
    modifier.semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    Box(contentAlignment = Alignment.Center) {
        PhaseRing(sale, 30.dp)
        RingTick(sale.phase == SalePhase.Paid)
    }
    Column {
        val through = motion.fadeThrough.fraction
        val fadeQuick = motion.fadeQuick.fraction
        AnimatedContent(
            targetState = SaleText.status(sale, posOf(sale)),
            // The pill's own size animates; the words' box takes the wider of the two while they change. The new words
            // fade through, in once the old are out, never over them ("Paid" over "Waiting for payment": decision 225).
            transitionSpec = { (fadeIn(through) togetherWith fadeOut(fadeQuick)).using(null) },
            label = "status words",
        ) { words ->
            // Polite, so "Confirming · 4 of 10" is said once a block (about every two minutes) while the customer waits, on purpose.
            Text(
                words, fontSize = 16.sp, fontWeight = FontWeight.Bold,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
        Countdown(now, sale.expiresAt, fontSize = 13.sp, color = mk.muted) { SaleText.hint(sale, it) }
    }
}

/** The paid tick inside the closed ring: in as the ring's paid arc comes in, out as it goes, per draw. */
@Composable
private fun RingTick(paid: Boolean) {
    val m = motion
    val shown by animateFloatAsState(if (paid) 1f else 0f, (if (paid) m.fadeThrough else m.fadeQuick).fraction, label = "ring tick")
    Icon(
        MkIcons.Tick, contentDescription = null, tint = mk.paid,
        modifier = Modifier.size(18.dp).graphicsLayer {
            alpha = shown
            compositingStrategy = CompositingStrategy.ModulateAlpha
        },
    )
}

@Composable
private fun Buttons(sale: TillSale, header: SaleHeader, actions: TillActions) = Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
    OutlineButton("Copy address", { actions.onCopy(sale) }, Modifier.weight(1f), height = 56.dp)
    actions.onBackToPos?.takeIf { header.backToPos }?.let { back ->
        PrimaryButton(PosText.backTo(header.pos), back, Modifier.weight(1f), height = 56.dp, size = 16)
    }
}

/**
 * What was bought, from this till's own receipt (decision 63). Folded, one
 * row says whose receipt it is, how many items and the total; unfolded, each
 * line and its price, the tax and the total follow under it. The row folds
 * and unfolds it, its chevron turning as the card's height follows on
 * [Motion.settle], read in layout. It opens by itself only when the page
 * with it open needs no scrolling ([room]); a tap is the cashier's and wins
 * while the page is up. On the sale ([onOpen] set) a long order shows three
 * lines and "+N more", and the lines open the whole order (decision 66); a
 * detail page lists every line. [pos] names the receipt ("Clover order …").
 * On the sale, [shared] is the key it grows into the order page by (M7).
 */
@Composable
fun OrderCard(receipt: Receipt, pos: PointOfSale?, room: ReceiptRoom, onOpen: (() -> Unit)? = null, shared: String? = null) {
    val (lines, more) = if (onOpen != null) SaleText.shownLines(receipt) else receipt.lineItems to 0
    val open = onOpen.takeIf { more > 0 }
    val m = motion
    val tapped = room.tapped
    val unfolded = tapped.value ?: room.fits.value
    // From the fold the sale's page last had, so a page come back to (from its order, its code, a turn) opens as it was.
    val shown = remember(room) { Animatable(if (unfolded) 1f else 0f) }
    LaunchedEffect(shown, unfolded, m) { shown.animateTo(if (unfolded) 1f else 0f, m.settle.fraction) }
    // Opened by the cashier's tap, the card follows its own growth into view, so its lines never open below the page's
    // edge; a card that opens by itself had the room for them, and one come back to open stays where the page puts it.
    // Read only in the effect's flow, never in composition, so a fold's frames still recompose nothing.
    val into = remember { BringIntoViewRequester() }
    val opening = remember(room) { mutableStateOf(false) }
    LaunchedEffect(opening, into) {
        snapshotFlow { opening.value }.collectLatest { asked ->
            if (!asked) return@collectLatest
            snapshotFlow { shown.value }.takeWhile { it < 1f }.collectLatest { into.bringIntoView() }
            withFrameNanos {}
            into.bringIntoView()
            opening.value = false
        }
    }
    val window = LocalConfiguration.current.screenHeightDp
    val fold = ReceiptFold(unfolded, shown, onToggle = { opening.value = !unfolded; tapped.value = !unfolded }) { now, whole -> room.judge(now, whole, window) }
    // On the sale, the card's ground grows into the order page's list (M7) while its three lines fade out and the
    // page's whole list fades in, each at its own size (decisions 185, 188).
    Box(Modifier.fillMaxWidth().bringIntoViewRequester(into)) {
        if (shared != null) {
            val ground = rememberGroundTrack(shared)
            Box(Modifier.matchParentSize().sharedGround(shared, Move.OrderGrow, GroundLook(mk.card, CardCorner), ground))
            OrderLines(receipt, pos, lines, more, open, fold, Modifier.words(over = Move.OrderGrow, on = ground))
        } else {
            Box(Modifier.matchParentSize().clip(CardShape).background(mk.card))
            OrderLines(receipt, pos, lines, more, open, fold, Modifier)
        }
    }
}

/**
 * Each sale's order card folds, kept above the till's stage (in the view
 * model, so a turn keeps them too): a sale's page leaves composition when
 * its order or its code opens over it, and comes back open or folded as
 * it was. The last [Kept] sales' only, so the map never grows with a shift.
 */
class ReceiptFolds {
    private val sales = object : LinkedHashMap<String, Pair<ReceiptRoom, ReceiptRoom>>(Kept, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Pair<ReceiptRoom, ReceiptRoom>>) = size > Kept
    }

    /** [orderId]'s live sale's room, or its page's: one tap for both, so a sale paid on screen keeps the cashier's fold. */
    fun room(orderId: String, live: Boolean): ReceiptRoom =
        sales.getOrPut(orderId) { mutableStateOf<Boolean?>(null).let { ReceiptRoom(it) to ReceiptRoom(it) } }
            .let { if (live) it.first else it.second }

    private companion object {
        const val Kept = 8
    }
}

/** The till's [ReceiptFolds], provided at its root; a screen composed without them keeps its own while it is up. */
val LocalReceiptFolds = staticCompositionLocalOf<ReceiptFolds?> { null }

/** [orderId]'s room for its live sale's order card or its page's ([live] false), from [LocalReceiptFolds]. */
@Composable
fun rememberReceiptRoom(orderId: String, live: Boolean): ReceiptRoom {
    val folds = LocalReceiptFolds.current
    return remember(folds, orderId, live) { (folds ?: ReceiptFolds()).room(orderId, live) }
}

/**
 * How much of a page shows and how tall all of it is, so its order card
 * opens by itself only when the page with the card open needs no
 * scrolling: plain numbers the page's layout writes and the card's
 * placement, which follows it, reads; never state, so they cost no
 * recomposition and no frame. Judged once, the first time the card is
 * placed, and again only in a window of another height: a page whose
 * status wraps or whose buttons change never folds or opens its card by
 * itself. [tapped] is the cashier's, which wins.
 */
class ReceiptRoom(internal val tapped: MutableState<Boolean?>) {
    private var shown = Constraints.Infinity
    private var whole = 0

    /** Whether the page fits its card open, as last judged. */
    internal val fits = mutableStateOf(false)

    /** The window height (dp) [fits] was judged in; -1 not yet. */
    private var judgedIn = -1

    /** On the page's scrolling column, before its `verticalScroll`: the height the page shows. */
    val window: Modifier = Modifier.layout { measurable, constraints ->
        shown = constraints.maxHeight
        val placeable = measurable.measure(constraints)
        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }

    /**
     * After its `verticalScroll`: the height of all of the page, the card as it is now. The scroll stretches its
     * content to the window at least, so the content is measured free of that and laid out at the top of it, as it is.
     */
    val content: Modifier = Modifier.layout { measurable, constraints ->
        val placeable = measurable.measure(constraints.copy(minHeight = 0))
        whole = placeable.height
        layout(placeable.width, constraints.constrainHeight(placeable.height)) { placeable.place(0, 0) }
    }

    /** Whether the page fits with a card [now] px tall shown [open] px tall instead. */
    private fun fits(now: Int, open: Int): Boolean = (shown == Constraints.Infinity || whole - now + open <= shown)

    /** From the card's placement, a card [now] px tall and [open] px open, in a window [window] dp tall. */
    internal fun judge(now: Int, open: Int, window: Int) {
        if (tapped.value != null || judgedIn == window) return
        judgedIn = window
        fits(now, open).let { if (it != fits.value) fits.value = it }
    }
}

/** An order card's fold: [unfolded] the cashier's or the room's, [shown] how much of its lines show, 0..1. */
private class ReceiptFold(
    val unfolded: Boolean,
    val shown: Animatable<Float, AnimationVector1D>,
    val onToggle: () -> Unit,
    /** Told the card's height now and open, in its placement. */
    val placed: (now: Int, open: Int) -> Unit,
)

@Composable
private fun OrderLines(
    receipt: Receipt,
    pos: PointOfSale?,
    lines: List<Receipt.LineItem>,
    more: Int,
    open: (() -> Unit)?,
    fold: ReceiptFold,
    modifier: Modifier,
) {
    val shown = fold.shown
    Layout(
        content = {
            // The whole row is the fold's button, 48 dp at least; its words are the card's heading folded or not.
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp)
                    .clickable(onClickLabel = if (fold.unfolded) "Hide the order's lines" else "Show the order's lines", role = Role.Button, onClick = fold.onToggle)
                    .semantics {
                        heading()
                        stateDescription = if (fold.unfolded) "Expanded" else "Collapsed"
                    }
                    .padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(SaleText.receiptSummary(receipt, pos), fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Icon(MkIcons.Fold, contentDescription = null, tint = mk.ink, modifier = Modifier.size(20.dp).graphicsLayer { rotationZ = shown.value * 180f })
            }
            Column(
                Modifier.fillMaxWidth()
                    .then(if (open != null) Modifier.clickable(onClickLabel = "See the whole order", onClick = open) else Modifier)
                    .padding(start = 16.dp, end = 16.dp, bottom = 12.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                for (line in lines) {
                    Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(SaleText.line(line), fontSize = 14.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                        Text(SaleText.plain(line.lineTotal, receipt.currency), fontSize = 14.sp, color = mk.muted, softWrap = false)
                    }
                }
                if (more > 0) Text("+$more more", fontSize = 14.sp, color = mk.link)
                Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
                Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(SaleText.tax(receipt) ?: "Total", fontSize = 14.sp, color = mk.muted, modifier = Modifier.weight(1f))
                    Text(SaleText.total(receipt), fontSize = 14.sp, fontWeight = FontWeight.Bold, softWrap = false)
                }
            }
        },
        modifier = Modifier.fillMaxWidth().then(modifier).clip(CardShape),
    ) { (row, body), constraints ->
        // Both are measured whatever the fold, so the room is judged by the card open; the lines are clipped to how
        // much of them shows (read here, per frame, never in composition) and left unplaced when none does.
        val head = row.measure(constraints.copy(minHeight = 0))
        val all = body.measure(constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity))
        val showing = (all.height * shown.value).roundToInt()
        val height = constraints.constrainHeight(head.height + showing)
        layout(constraints.constrainWidth(maxOf(head.width, all.width)), height) {
            fold.placed(height, head.height + all.height)
            head.place(0, 0)
            if (showing > 0) all.place(0, head.height)
        }
    }
}

/** The key of the surface a sale's code card closes into: its page's hero. */
fun heroKey(orderId: String) = "hero/$orderId"

/** The key of a sale's state glyph between its page's hero and its shift row's badge. */
fun badgeKey(orderId: String) = "badge/$orderId"

/** A page's hero: the state's glyph at the top of a sale's page. */
val HeroSize = 72.dp

/** A state's glyph on its ground: a shift row's 36 dp circle, a page's [HeroSize] hero. */
@Composable
fun StateBadge(sale: TillSale, icon: StateIcon, ground: Ground, size: Dp, glyph: Dp, quiet: Boolean = false) {
    if (icon == StateIcon.Ring) {
        PhaseRing(sale, size)
        return
    }
    val (vector, tint) = when (icon) {
        StateIcon.Clock -> MkIcons.Clock to mk.warn
        StateIcon.Warning -> MkIcons.Warning to mk.danger
        StateIcon.Tick -> MkIcons.Tick to mk.paid
        StateIcon.Refund -> MkIcons.Refund to if (quiet) mk.muted else mk.ink
        StateIcon.Close, StateIcon.Ring -> MkIcons.Close to if (quiet) mk.muted else mk.ink
    }
    Box(Modifier.size(size).clip(CircleShape).background(groundColor(ground)), contentAlignment = Alignment.Center) {
        Icon(vector, contentDescription = null, tint = tint, modifier = Modifier.size(glyph))
    }
}

/** The theme's colour for a [Ground] role. */
@Composable
fun groundColor(ground: Ground): Color = when (ground) {
    Ground.Paid -> mk.paidGround
    Ground.Warn -> mk.warnGround
    Ground.Danger -> mk.dangerGround
    Ground.Chip -> mk.chip
    Ground.Card -> mk.card
}

/** The theme's colour for a [Tone] role. */
@Composable
fun toneColor(tone: Tone): Color = when (tone) {
    Tone.Link -> mk.link
    Tone.Warn -> mk.warn
    Tone.Danger -> mk.danger
    Tone.Paid -> mk.paid
    Tone.Muted -> mk.muted
}
