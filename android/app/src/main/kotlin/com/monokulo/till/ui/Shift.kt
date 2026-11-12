package com.monokulo.till.ui

import android.content.Context
import android.view.accessibility.AccessibilityManager
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitHorizontalTouchSlopOrCancellation
import androidx.compose.foundation.gestures.awaitVerticalTouchSlopOrCancellation
import androidx.compose.foundation.gestures.horizontalDrag
import androidx.compose.foundation.gestures.verticalDrag
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.indication
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.absolutePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemGestures
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.FloatState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.AbsoluteAlignment
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.layout.findRootCoordinates
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.till.FoldStack
import com.monokulo.till.GroundLook
import com.monokulo.till.Layers
import com.monokulo.till.LocalStage
import com.monokulo.till.Move
import com.monokulo.till.rememberGroundTrack
import com.monokulo.till.RowSwipe
import com.monokulo.till.sharedGround
import com.monokulo.till.sharedSurface
import com.monokulo.till.words
import com.monokulo.till.engine.CloseSheet
import uniffi.till_app.Action
import uniffi.till_screens.CloseThen
import uniffi.till_screens.Ground
import uniffi.till_screens.ringOf
import uniffi.till_screens.RowEnds
import uniffi.till_screens.RowOpens
import uniffi.till_screens.ShiftEntry
import uniffi.till_screens.ShiftFilter
import uniffi.till_screens.ShiftHeader
import uniffi.till_screens.ShiftIdle
import uniffi.till_screens.ShiftRow
import uniffi.till_screens.ShiftView
import uniffi.till_screens.StateIcon
import uniffi.till_screens.StatusChip
import uniffi.till_screens.Tone
import uniffi.till_screens.FilterChip as ViewChip
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * The shift (the design's ShiftList; decisions 60, 64, 72), as the engine
 * wrote it: what this till is doing, this shift's summary, the filter chips
 * and one row per sale, newest first. Rows have no buttons: a live sale's
 * row opens its code, any other its page. [place] is where the list was
 * when a row was tapped, kept over the stage (the activity's) so a Back
 * comes back to it. [close] is the swiped row's sheet, drawn here while the
 * shift is the screen the stage goes to.
 */
@Composable
fun ShiftScreen(
    view: ShiftView,
    actions: TillActions,
    place: ShiftPlace = rememberShiftPlace(),
    close: CloseSheet? = null,
) {
    // Back from a sale (or a paid one stepping back): the sale the stage is leaving, read once as the change starts
    // and once as it lands, never a frame. Its row must be there for the ground to shrink into, so the engine opens a
    // tray whose fold would hide it before the change starts (decisions 269, 289). Once landed, focus goes to that row.
    val stage = LocalStage.current
    val cameFrom = stage?.leavingSale
    val items = view.entries
    val head = remember(view.disconnected, view.idle, view.chips.isEmpty()) { Head.of(view) }
    val trayFolds = remember(items) { items.any { it is ShiftEntry.Fold } }
    val folded = remember(items) { items.any { it is ShiftEntry.Fold && !it.open } }
    val list = rememberLazyListState()
    // The shift is composed afresh by a Back (the stage composes only the screens it shows), its list at the top: a
    // row tapped far down would be off screen, with nothing for the ground to shrink into. So in this same first
    // composition the list is asked to put the row where it was when tapped, laid out on the change's first frame
    // (decision 296). Asked once by this list; the place is forgotten only once the shift has landed, so a back
    // gesture peeked and cancelled (which composes the shift and leaves it again) leaves it for the Back that follows
    // (decision 297). A Back from a sale not left from its row asks nothing.
    // The list is put back where it was first (decision 300): a sale paid meanwhile has moved from the tray to the top
    // of Earlier, and holding its row under the finger would scroll the whole list, the tray's waiting sales off the
    // top. Only a row that would then be off screen is put back under the finger instead.
    if (cameFrom != null && place.sale == cameFrom && place.askedBy !== list) {
        val index = rowIndex(head, items, cameFrom)
        SideEffect {
            place.askedBy = list
            if (index >= 0) place.restore(list, index)
        }
    }
    val focusOn = remember { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val screenReader = LocalScreenReader.current
    LaunchedEffect(cameFrom) {
        if (cameFrom == null) {
            // At rest on the shift (a change onto it landed, or it was there first): the place has been used, if at all.
            if (stage == null || stage.landed()) place.forget()
            return@LaunchedEffect
        }
        // Focus moves for a screen reader only: to a sighted cashier a focused row is a grey row nobody asked for.
        if (screenReader(context)) focusOn.value = cameFrom
    }
    val arrive = motion.fade.fraction
    val leave = motion.fadeQuick.fraction
    val placement = motion.settle.spec(IntOffset.VisibilityThreshold)
    // A fold kept under the finger moves no item: the list is asked to hold the fold row where it is, and the rows
    // that stay above it come in from off screen, where the list would otherwise slide them in from its edge (and the
    // fold row with them). So a scrolled tray folded is placed with no placement move, for as long as it shows that
    // fold of those sales: the next change (a sale paid, the tray opened, a filter chosen) moves its rows as ever. The
    // rows compose in the list's measure, after this composition, so the flag is a fact about what is shown, not a frame.
    var keptFor by remember { mutableStateOf<List<String>?>(null) }
    val keeping = folded && keptFor == foldedLook(items)
    Surface(Modifier.fillMaxSize(), color = mk.ground, contentColor = mk.ink) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            // The shift's own words, its chrome as much as its rows, fade through a row's grow and shrink at their own
            // size, so they are never read together with the sale's (decision 221).
            val chrome = Modifier.words(through = ShiftThrough)
            LazyColumn(Modifier.widthIn(max = 640.dp).fillMaxSize(), state = list) {
                // The items above the rows, from the one list a Back's place counts them by too (rowIndex).
                for (key in head) item(key = key) {
                    when (key) {
                        Head.Top -> Box(chrome) { TopRow(view.top.place, view.top.state, view.top.tone, view.top.said, actions) }
                        Head.Disconnected -> view.disconnected?.let { gone ->
                            Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                                Heading(gone.heading)
                                Lead(gone.lead)
                                PrimaryButton(gone.connectAgain.label, { actions.send(Action.Forget) }, Modifier.fillMaxWidth(), enabled = gone.connectAgain.enabled)
                            }
                        }
                        Head.Problem -> {
                            Notice(view.problem, chrome.padding(start = 16.dp, end = 16.dp, bottom = 12.dp))
                            view.signInAgain?.let { again ->
                                PrimaryButton(again.label, { actions.send(Action.SignIn(null)) }, chrome.padding(start = 16.dp, end = 16.dp, bottom = 12.dp).fillMaxWidth(), enabled = again.enabled)
                            }
                        }
                        Head.Summary -> Column(chrome.padding(start = 24.dp, end = 24.dp, top = 4.dp, bottom = 16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            val header = view.header
                            if (header != null) {
                                NewShiftHeader(view.summaryLabel, header, actions)
                            } else {
                                Text(view.summaryLabel, style = display(28, -0.4), modifier = Modifier.semantics { heading() })
                                Text(view.summary, fontSize = 14.sp, color = mk.muted)
                            }
                            // Cash paid in Monero, and whose sales show (docs/decisions/core.md 114, 117), drawn plainly.
                            view.cashInMonero?.let { Text(it, fontSize = 14.sp, color = mk.muted) }
                            view.counters?.let { menu ->
                                Text(menu.heading, fontSize = 13.sp, color = mk.muted)
                                for (counter in menu.items) {
                                    val words = listOfNotNull(counter.name, counter.count).joinToString(" · ")
                                    LinkButton(
                                        if (counter.selected) "✓ $words" else words,
                                        { actions.send(Action.ChooseCounter(counter.choice)) },
                                        size = 14,
                                    )
                                }
                            }
                        }
                        Head.Idle -> view.idle?.let { Idle(it, view.status, actions) }
                        // A filter chosen is a change of what is shown: its rows move as ever, even over a fold kept under the finger.
                        Head.Chips -> Box(chrome) {
                            Chips(view.chips) {
                                actions.send(Action.ChooseFilter(it))
                                keptFor = null
                            }
                        }
                    }
                }
                if (view.disconnected != null) return@LazyColumn
                view.empty?.let { empty ->
                    item(key = "empty") {
                        Text(
                            empty, fontSize = 15.sp, color = mk.muted, textAlign = TextAlign.Center,
                            modifier = chrome.fillMaxWidth().padding(horizontal = 24.dp, vertical = 48.dp),
                        )
                    }
                }
                // One list for the tray and Earlier (A4): a row paid or ended is the same keyed item moving from the
                // tray to the top of Earlier, so the rows' own three specs move everything (M11) and the tray, which
                // has no node of its own, is its items' amber slices: its height is its rows'.
                items(items, key = ::itemKey, contentType = ::itemType) { item ->
                    val moves = Modifier.animateItem(fadeInSpec = arrive, placementSpec = if (keeping) null else placement, fadeOutSpec = leave)
                    when (item) {
                        is ShiftEntry.TrayHeader -> TrayHeader(item, moves.then(chrome))
                        is ShiftEntry.Earlier -> SectionHeader(item.label, EarlierTop, moves.then(chrome))
                        is ShiftEntry.Fold -> FoldRow(item, moves.padding(horizontal = 16.dp).fillMaxWidth().background(mk.warnGround, TrayBottom), chrome) {
                            // Folding: the tray's rows past the third go, and the look the fold leaves is kept under the finger.
                            keptFor = if (item.open && keepFoldInPlace(list, item.hidden.toInt())) foldedLook(items) else null
                            actions.send(Action.FoldTray(item.open))
                        }
                        is ShiftEntry.Row -> {
                            // Opened by its id alone, so a row whose corners change keeps the one it had and skips.
                            val id = item.orderId
                            val ask = if (item.closes) ({ actions.send(Action.AskClose(id)) }) else null
                            SaleItem(item, trayFolds, focusOn, moves, ask) {
                                place.leave(list, id)
                                actions.send(Action.OpenSale(id))
                            }
                        }
                    }
                }
                if (items.isNotEmpty()) item(key = "end") { Spacer(Modifier.height(24.dp)) }
            }
        }
    }
    // The sale page's X's sheet, the same answers doing the same (decisions 62, 70). Paid or ended meanwhile, the
    // engine takes it down. Where each answer lands is the engine's (docs/decisions/core.md 96): "No: refund it" opens
    // the sale, whose page shows the refund, so the sale grows out of its row here.
    val asked = close?.view
    val id = close?.orderId
    if (asked != null && id != null) {
        CloseSheet(
            asked,
            onChoice = { choice ->
                if (asked.answers.firstOrNull { it.choice == choice }?.then == CloseThen.SALE) place.leave(list, id)
                actions.send(Action.Close(id, choice))
            },
            onDismiss = { actions.send(Action.DismissClose) },
        )
    }
}

/** What a folded tray leaves standing: the Earlier rows and the tray's count, the same before and after the fold. */
private fun foldedLook(items: List<ShiftEntry>): List<String> = items.mapNotNull {
    when (it) {
        is ShiftEntry.Row -> it.orderId.takeUnless { _ -> it.tray }
        is ShiftEntry.TrayHeader -> "tray ${it.count}"
        else -> null
    }
}

/**
 * One sale's row, in the tray or under Earlier. Tapped, the row's card grows into its sale (M7) and the sale shrinks
 * back into it; only the ground moves, the row's words fade out at their own size as the sale's fade in (decision 185).
 * In the tray the card sits in its slice of the amber, 4 dp in, so it is concentric with the tray's corners.
 */
@Composable
private fun SaleItem(
    item: ShiftEntry.Row,
    trayFolds: Boolean,
    focusOn: MutableState<String?>,
    modifier: Modifier = Modifier,
    onAsk: (() -> Unit)? = null,
    onOpen: () -> Unit,
) {
    val id = item.orderId
    // Only the row the cashier came back to reads where focus goes, and asks for it once the stage has landed. A row
    // that can be clicked takes no focus in touch mode, so this one row may, for as long as it is the one came back to.
    val focused by remember(id) { derivedStateOf { focusOn.value == id } }
    val focus = if (focused) remember { FocusRequester() } else null
    if (focus != null) {
        val stage = LocalStage.current
        LaunchedEffect(focus) {
            if (stage != null) snapshotFlow { stage.landed() }.first { it }
            focus.requestFocus()
        }
    }
    val ends = item.ends
    val shape = rowShape(ends)
    val top = ends == RowEnds.FIRST || ends == RowEnds.ONLY
    val bottom = (ends == RowEnds.LAST || ends == RowEnds.ONLY) && !trayFolds
    val slice = if (!item.tray) Modifier else Modifier.background(mk.warnGround, traySlice(top, bottom))
        .padding(start = TrayPadding, end = TrayPadding, top = if (top) TrayPadding else 0.dp, bottom = if (bottom) TrayPadding else 0.dp)
    // A row whose sale has the X swipes left over the X's ground ([RowSwipe]): the card and its words move as
    // one, by a pull in dp held and settled as the fold's stack's is.
    val pull = if (onAsk != null) remember { StackPull() } else null
    Box(modifier.padding(horizontal = 16.dp).fillMaxWidth().then(slice)) {
        Box(if (pull != null && onAsk != null) Modifier.swipesToAsk(pull, onAsk) else Modifier) {
            if (pull != null) SwipeBacking(pull.dp, shape)
            Box(if (pull != null) Modifier.absoluteOffset { IntOffset(pull.dp.floatValue.dp.roundToPx(), 0) } else Modifier) {
                val ground = rememberGroundTrack("sale/$id")
                Box(Modifier.matchParentSize().sharedGround("sale/$id", Move.RowGrow, rowLook(ends, mk.card), ground))
                Column(Modifier.fillMaxWidth().words(over = Move.RowGrow, on = ground).clip(shape)) {
                    if (ends == RowEnds.MIDDLE || ends == RowEnds.LAST) Box(Modifier.padding(start = 64.dp).fillMaxWidth().height(1.dp).background(mk.divider))
                    SaleRow(id, RowWords.of(item.row), if (focus != null) Modifier.focusRequester(focus).focusProperties { canFocus = true } else Modifier, onAsk, onOpen)
                }
            }
        }
    }
}

/**
 * A row swiped left ([RowSwipe]): horizontal only, so the list's scroll and the fold's pull down keep theirs,
 * and not from inside the system's gesture zones, where a swipe in from the edge is Back. Left as drawn in any layout
 * direction (the owner's "left swipe"; the finger, the offset and the backing are all absolute). The row follows the finger
 * on [pull] (not under reduced motion), and let go it settles back on [Motion.settle] with the finger's speed; past
 * the reach or flung, [onAsk] too. A new touch catches it where it is.
 */
@Composable
private fun Modifier.swipesToAsk(pull: StackPull, onAsk: () -> Unit): Modifier {
    val m = motion
    val scope = rememberCoroutineScope()
    val ask by rememberUpdatedState(onAsk)
    val zones = WindowInsets.systemGestures
    val placed = remember { Placed() }
    return onPlaced { placed.at = it }.pointerInput(pull, m) {
        val back = m.settle.spec<Float>(StackThreshold)
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            if (placed.inZone(down.position, zones, this)) return@awaitEachGesture
            var raw = 0f
            // Left only: a swipe to the right is nobody's, one up or down the list's.
            val start = awaitHorizontalTouchSlopOrCancellation(down.id) { change, over ->
                if (over < 0f) {
                    change.consume()
                    raw = over
                }
            } ?: return@awaitEachGesture
            val width = size.width / density
            // Caught settling, the row goes on from where it is, not from its rest.
            val from = pull.dp.floatValue
            pull.hold(if (m.still) 0f else RowSwipe.pulled(from + raw / density, width))
            // Cut off under the finger (the system took it for Back, the row left the list), the row still settles.
            var let = false
            try {
                val tracker = VelocityTracker()
                tracker.addPointerInputChange(start)
                val ended = horizontalDrag(start.id) { change ->
                    raw += change.positionChange().x
                    change.consume()
                    tracker.addPointerInputChange(change)
                    if (!m.still) pull.hold(RowSwipe.pulled(from + raw / density, width))
                }
                val velocity = tracker.calculateVelocity().x / density
                let = true
                pull.settle(scope, pull.dp.floatValue, back, velocity)
                if (ended && RowSwipe.asks(RowSwipe.pulled(from + raw / density, width), width, velocity)) ask()
            } finally {
                if (!let) pull.settle(scope, pull.dp.floatValue, back)
            }
        }
    }
}

/** Where a swiping row is in its window, kept as it is placed and read only as a touch lands. */
private class Placed {
    var at: LayoutCoordinates? = null

    /** Whether [position], on the row, is inside the system's gesture zones at the window's sides (left and right as drawn). */
    fun inZone(position: Offset, zones: WindowInsets, density: Density): Boolean {
        val at = at?.takeIf { it.isAttached } ?: return false
        val x = at.localToWindow(position).x
        val window = at.findRootCoordinates().size.width
        return x < zones.getLeft(density, LayoutDirection.Ltr) || x > window - zones.getRight(density, LayoutDirection.Ltr)
    }
}

/**
 * What a swiped row uncovers: the X's danger ground in the row's [shape], its X at the right, drawn only in the strip
 * the card has moved off ([pull], read per draw), so nothing is drawn at rest and nothing shows round the card. The strip
 * reaches a row corner further in, under the card, so the card's own rounded corners show the danger ground behind them,
 * not the tray's amber.
 */
@Composable
private fun BoxScope.SwipeBacking(pull: FloatState, shape: Shape) {
    val ground = mk.dangerGround
    Box(
        Modifier.matchParentSize().drawWithCache {
            val outline = shape.createOutline(size, layoutDirection, this)
            onDrawWithContent {
                val shown = -pull.floatValue.dp.toPx()
                if (shown <= 0f) return@onDrawWithContent
                clipRect(left = (size.width - shown - RowCorner.toPx()).coerceAtLeast(0f)) {
                    drawOutline(outline, ground)
                    this@onDrawWithContent.drawContent()
                }
            }
        },
        contentAlignment = AbsoluteAlignment.CenterRight,
    ) {
        Icon(MkIcons.Close, contentDescription = null, tint = mk.danger, modifier = Modifier.absolutePadding(right = 24.dp).size(16.dp))
    }
}

/** "Waiting for payment" and how many, over the tray: one heading to TalkBack. */
@Composable
private fun TrayHeader(header: ShiftEntry.TrayHeader, modifier: Modifier) = Row(
    modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 18.dp, bottom = 8.dp)
        .clearAndSetSemantics { heading(); contentDescription = header.said },
    verticalAlignment = Alignment.CenterVertically,
) {
    Text(header.label, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = mk.ink, modifier = Modifier.weight(1f))
    Text("${header.count}", style = tabular, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = mk.muted)
}

/** A section's header over its rows. */
@Composable
private fun SectionHeader(text: String, top: Dp, modifier: Modifier) = Text(
    text, fontSize = 14.sp, fontWeight = FontWeight.Bold, color = mk.ink,
    modifier = modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = top, bottom = 8.dp).semantics { heading() },
)

/**
 * The tray's last slice (the fixes canvas's board 4): folded, the hidden sales peek as two card edges under the third
 * row over a "+N" pill; open, the pill reads "Show fewer". In ink on the card (a neutral button: the link orange is
 * under 4.5:1 on the tint), the chevron turning over as it changes. The whole slice is the button, at least 48 dp
 * tall, its press shown on the pill. Folded, the stack can be pulled down: it follows the finger and, let go far
 * enough or flung, opens the tray ([FoldStack]); else it settles back. The pull is read only in layout and draw.
 * Focus stays on it as the rows come and go.
 */
@Composable
private fun FoldRow(fold: ShiftEntry.Fold, modifier: Modifier, words: Modifier, onToggle: () -> Unit) {
    val m = motion
    val through = m.fadeThrough.fraction
    val quick = m.fadeQuick.fraction
    val turn by animateFloatAsState(if (fold.open) 180f else 0f, m.settle.spec(FoldTurnThreshold), label = "fold chevron")
    val pull = remember { StackPull() }
    val scope = rememberCoroutineScope()
    val toggle = {
        // The edges come or go at once; the pull starts a gap off and settles, so the pill stays where it was.
        if (m.still) pull.hold(0f) else pull.settle(scope, if (fold.open) -FoldStack.GAP_DP else pull.dp.floatValue + FoldStack.GAP_DP, m.settle.spec(StackThreshold))
        onToggle()
    }
    val toggleNow by rememberUpdatedState(toggle)
    val source = remember { MutableInteractionSource() }
    val t = mk
    val tones = remember(t) { StackTones.of(t) }
    val drag = if (fold.open) Modifier else Modifier.pointerInput(pull, m) {
        val back = m.flick.spec<Float>(StackThreshold)
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            var raw = 0f
            // Down only: a drag up from the stack is the list's, to scroll.
            val start = awaitVerticalTouchSlopOrCancellation(down.id) { change, over ->
                if (over > 0f) {
                    change.consume()
                    raw = over
                }
            } ?: return@awaitEachGesture
            pull.hold(FoldStack.pulled(raw / density))
            // Cut off under the finger (reduced motion switched, the tray opened another way), the stack still settles.
            var let = false
            try {
                val tracker = VelocityTracker()
                tracker.addPointerInputChange(start)
                val ended = verticalDrag(start.id) { change ->
                    raw += change.positionChange().y
                    change.consume()
                    tracker.addPointerInputChange(change)
                    pull.hold(FoldStack.pulled(raw / density))
                }
                val velocity = tracker.calculateVelocity().y / density
                let = true
                if (ended && FoldStack.opens(pull.dp.floatValue, velocity)) toggleNow() else pull.settle(scope, pull.dp.floatValue, back, velocity)
            } finally {
                if (!let) pull.settle(scope, pull.dp.floatValue, back)
            }
        }
    }
    Column(
        modifier.then(drag)
            .clickable(interactionSource = source, indication = null, role = Role.Button, onClick = toggle)
            .clearAndSetSemantics {
                role = Role.Button
                contentDescription = fold.said
                stateDescription = if (fold.open) "Expanded" else "Collapsed"
                onClick { toggle(); true }
            },
    ) {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
            Spacer(Modifier.fillMaxWidth().stackEdges(fold.open, pull.dp, tones))
            Row(
                Modifier.padding(top = if (fold.open) 10.dp else 8.dp, bottom = 10.dp).heightIn(min = 28.dp)
                    .animateContentSize(m.settle.spec(IntSize.VisibilityThreshold))
                    .clip(PillShape).background(mk.card).indication(source, ripple())
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                // Only the words fade with the screen ([words]): the edges and the pill's card are ground, and stay.
                Row(words, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    // Two different words, so through, not across: the old out quickly, the new in once it has gone
                    // (decision 188; the canvas's "crossfades", decision 291).
                    AnimatedContent(targetState = fold.label, transitionSpec = { (fadeIn(through) togetherWith fadeOut(quick)).using(null) }, label = "fold label") { label ->
                        Text(label, fontFamily = Manrope, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = mk.ink, maxLines = 1)
                    }
                    Icon(MkIcons.Fold, contentDescription = null, tint = mk.ink, modifier = Modifier.size(14.dp).graphicsLayer { rotationZ = turn })
                }
            }
        }
    }
}

/**
 * How far the fold's stack is pulled past its rest, in dp: held by the finger, or settling back to rest on a preset.
 * A new hold stops a settle where it is, so a finger catches the stack mid-way. Read only in layout and draw.
 */
private class StackPull {
    val dp = mutableFloatStateOf(0f)
    private var settling: Job? = null

    fun hold(at: Float) {
        settling?.cancel()
        settling = null
        dp.floatValue = at
    }

    fun settle(scope: CoroutineScope, from: Float, spec: AnimationSpec<Float>, velocity: Float = 0f) {
        hold(from)
        settling = scope.launch { animate(from, 0f, velocity, spec) { v, _ -> dp.floatValue = v } }
    }
}

/** The stack's two edges, from the card toward the tray's amber, and the line under each, the amber toward its ink. */
private class StackTones(val near: Color, val far: Color, val line: Color) {
    fun edge(i: Int) = if (i == 0) near else far

    companion object {
        fun of(t: Tokens) = StackTones(lerp(t.card, t.warnGround, 0.2f), lerp(t.card, t.warnGround, 0.6f), lerp(t.warnGround, t.warn, 0.1f))
    }
}

/**
 * The stack's edges, as tall as [pull] makes them (none open, past the pull a settle has left), laid out and drawn
 * from it, never composed: under the card's 4 dp of tray each side, square at the top, rounded at the bottom, the
 * lower first so the upper's line lies over it.
 */
private fun Modifier.stackEdges(open: Boolean, pull: FloatState, tones: StackTones): Modifier =
    layout { measurable, constraints ->
        val p = pull.floatValue
        val h = (if (open) p.coerceAtLeast(0f) else FoldStack.area(p)).dp.roundToPx()
        val placeable = measurable.measure(constraints.copy(minHeight = h, maxHeight = h))
        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }.drawBehind {
        if (open) return@drawBehind
        val p = pull.floatValue
        val line = 1.dp.toPx()
        for (i in FoldStack.EDGES - 1 downTo 0) {
            val h = FoldStack.height(i, p).dp.toPx()
            if (h <= 0f) continue
            val top = FoldStack.top(i, p).dp.toPx()
            val x = (TrayPadding + FoldStack.inset(i, p).dp).toPx()
            val r = FoldStack.corner(i).dp.toPx()
            // A rounded rect reaching a corner above the edge, clipped there: square at the top, round at the bottom.
            clipRect(top = top) {
                drawRoundRect(tones.line, Offset(x, top - r + line), Size(size.width - 2 * x, h + r), CornerRadius(r))
                drawRoundRect(tones.edge(i), Offset(x, top - r), Size(size.width - 2 * x, h + r), CornerRadius(r))
            }
        }
    }

/**
 * Folding an open tray scrolled down past its third row: the rows above the fold row go, and with them the item the
 * list keeps its place by, so the fold row would jump off the top of the screen. The list is asked instead to put it
 * where it is now, under the finger, to the pixel. Found by its key among the items on screen (the list's indices
 * count the items above the tray too), with [hidden] rows going it is the item [hidden] earlier after the fold, asked
 * to sit as far below the list's top as it does now: a negative offset, so the list lays out the rows that stay above
 * it, whatever their heights. Whether it asked: the change is then placed with no placement move.
 */
private fun keepFoldInPlace(list: LazyListState, hidden: Int): Boolean {
    val visible = list.layoutInfo.visibleItemsInfo
    val at = visible.firstOrNull { it.key == FoldKey } ?: return false
    val after = at.index - hidden
    // Anchored on an item that stays (the third row or above), the list keeps its place by itself.
    if (visible.first().index < after) return false
    list.requestScrollToItem(after, scrollOffset = -at.offset)
    return true
}

/**
 * Where the shift's list was when the cashier left it from a row: that row's sale, how far below the list's top it
 * sat, and the list's own place. The shift leaves composition while a sale is up, its list state with it, so this is
 * kept above the stage (the activity's), and a Back puts the list there again ([ShiftScreen], decisions 296, 297, 300).
 * Plain fields: written by a tap, read by the composition a Back starts, forgotten once the shift has landed; nothing
 * draws or recomposes from them. Not marked stable, since it is not (decision 302): one instance for the activity's
 * life, so a screen taking it still skips under strong skipping, which compares it by identity.
 */
class ShiftPlace {
    internal var sale: String? = null
    internal var offset = 0

    /** The list's own place when the row was tapped: its first item shown, how far that is scrolled, and the last item shown whole. */
    internal var first = 0
    internal var firstOffset = 0
    internal var lastWhole = -1

    /** The list that has asked for this place: a shift composed afresh (a new list) asks again, the same list never. */
    internal var askedBy: LazyListState? = null

    /** The shift has landed: this place is used. */
    internal fun forget() {
        sale = null
        askedBy = null
    }

    /** The cashier leaves the shift from [id]'s row: where it is on screen now, and where the list is. */
    internal fun leave(list: LazyListState, id: String) {
        val info = list.layoutInfo
        val visible = info.visibleItemsInfo
        val at = visible.firstOrNull { it.key == id }
        sale = at?.let { id }
        offset = at?.offset ?: 0
        first = list.firstVisibleItemIndex
        firstOffset = list.firstVisibleItemScrollOffset
        lastWhole = visible.lastOrNull { it.offset + it.size <= info.viewportEndOffset }?.index ?: -1
    }

    /**
     * Puts [list], composed afresh by a Back, where it was when the row was tapped, if the row (now at [index], the sale
     * may have moved from the tray to Earlier) is shown whole there; otherwise puts the row where it was, under the
     * finger. Either way the row is laid out on the change's first frame for the ground to land in.
     */
    internal fun restore(list: LazyListState, index: Int) {
        val shown = (index > first || (index == first && firstOffset == 0)) && index <= lastWhole
        if (shown) list.requestScrollToItem(first, firstOffset) else list.requestScrollToItem(index, scrollOffset = -offset)
    }
}

/** A [ShiftPlace] kept across a rotation. */
@Composable
fun rememberShiftPlace(): ShiftPlace = rememberSaveable(saver = ShiftPlaceSaver) { ShiftPlace() }

private val ShiftPlaceSaver = listSaver<ShiftPlace, Any>(
    save = { p -> p.sale?.let { listOf(it, p.offset, p.first, p.firstOffset, p.lastWhole) }.orEmpty() },
    restore = { l ->
        ShiftPlace().apply {
            sale = l.getOrNull(0) as String?
            offset = l.getOrNull(1) as Int? ?: 0
            first = l.getOrNull(2) as Int? ?: 0
            firstOffset = l.getOrNull(3) as Int? ?: 0
            lastWhole = l.getOrNull(4) as Int? ?: -1
        }
    },
)

/**
 * [id]'s row's index in the shift's list, -1 if it has none: after the list's own items above the rows, [head], the
 * same list [ShiftScreen] lays them out from, so an item added there is counted here too.
 */
private fun rowIndex(head: List<String>, items: List<ShiftEntry>, id: String): Int {
    if (Head.Disconnected in head) return -1
    val at = items.indexOfFirst { it is ShiftEntry.Row && it.orderId == id }
    return if (at < 0) -1 else head.size + at
}

/** The shift list's items above its rows, by key, in the order it shows them. */
private object Head {
    const val Top = "top"
    const val Disconnected = "disconnected"
    const val Problem = "problem"
    const val Summary = "summary"
    const val Idle = "idle"
    const val Chips = "chips"

    /** The top row; disconnected, nothing else; or the notice, the summary, "Not watching" or nothing yet, and the chips. */
    fun of(view: ShiftView): List<String> = buildList {
        add(Top)
        if (view.disconnected != null) {
            add(Disconnected)
            return@buildList
        }
        add(Problem)
        add(Summary)
        if (view.idle != null) add(Idle)
        if (view.chips.isNotEmpty()) add(Chips)
    }
}

/** A list item's key: the tray's three parts by name, a row by its sale, so a row keeps its item between sections. */
private fun itemKey(item: ShiftEntry): Any = when (item) {
    is ShiftEntry.TrayHeader -> "tray"
    is ShiftEntry.Fold -> FoldKey
    is ShiftEntry.Earlier -> "earlier"
    is ShiftEntry.Row -> item.orderId
}

private fun itemType(item: ShiftEntry): Any = when (item) {
    is ShiftEntry.TrayHeader -> 0
    is ShiftEntry.Fold -> 1
    is ShiftEntry.Earlier -> 2
    is ShiftEntry.Row -> 3
}

/** The fold row's key, by which [keepFoldInPlace] finds it on screen. */
private const val FoldKey = "fold"

/** The chevron's turn is in degrees: a tenth of one is still. */
private const val FoldTurnThreshold = 0.1f

/** The stack's pull is in dp: a tenth of one is still. */
private const val StackThreshold = 0.1f

/** The fold's pill: 28 dp tall, round at its ends. */
private val PillShape = RoundedCornerShape(14.dp)

/** The tray: its corners, and how far inside them the rows' card sits (24 − 4 is the card's 20). */
private val TrayCorner = 24.dp
private val TrayPadding = 4.dp
private val TrayTop = RoundedCornerShape(topStart = TrayCorner, topEnd = TrayCorner)
private val TrayBottom = RoundedCornerShape(bottomStart = TrayCorner, bottomEnd = TrayCorner)
private val TrayOnly = RoundedCornerShape(TrayCorner)

/** A tray row's slice of the amber: rounded at the tray's top, its bottom (when no fold row closes it), or both. */
private fun traySlice(top: Boolean, bottom: Boolean): Shape = when {
    top && bottom -> TrayOnly
    top -> TrayTop
    bottom -> TrayBottom
    else -> RectangleShape
}

/** The Earlier header's space above it, more than the tray header's: it follows the tray. */
private val EarlierTop = 22.dp

/**
 * Whether a screen reader is exploring by touch, asked as a Back lands on the shift: only then does focus go to the
 * row the cashier came from (§12.3's TalkBack). A test seam, as [com.monokulo.till.LocalGroundTracks] is: the app's reads
 * the system's accessibility manager; a device test provides its own.
 */
val LocalScreenReader = staticCompositionLocalOf<(Context) -> Boolean> {
    { context -> context.getSystemService(AccessibilityManager::class.java)?.isTouchExplorationEnabled == true }
}

/** The moves the shift's chrome fades through: a row growing into its sale, and back. */
private val ShiftThrough = setOf(Move.RowGrow)

/** The rows' card's corner. */
private val RowCorner = 20.dp
private val OnlyRow = RoundedCornerShape(RowCorner)
private val FirstRow = RoundedCornerShape(topStart = RowCorner, topEnd = RowCorner)
private val LastRow = RoundedCornerShape(bottomStart = RowCorner, bottomEnd = RowCorner)

/** A row's ground: the rows' card's corners at its ends. */
private fun rowLook(ends: RowEnds, card: Color) = when (ends) {
    RowEnds.ONLY -> GroundLook(card, RowCorner)
    RowEnds.FIRST -> GroundLook(card, RowCorner, 0.dp)
    RowEnds.MIDDLE -> GroundLook(card, 0.dp)
    RowEnds.LAST -> GroundLook(card, 0.dp, RowCorner)
}

/** A row's part of the card, made once rather than per row per composition. */
private fun rowShape(ends: RowEnds): Shape = when (ends) {
    RowEnds.ONLY -> OnlyRow
    RowEnds.FIRST -> FirstRow
    RowEnds.MIDDLE -> RectangleShape
    RowEnds.LAST -> LastRow
}

/** The point of sale's mark, the store, this till and whether it's watching, the word in the dot's old colour; Settings. */
@Composable
// The engine's record taken apart: it isn't stable to Compose, and a tick must not redraw the top row.
private fun TopRow(place: String?, state: String, tone: Tone, said: String, actions: TillActions) = Row(
    Modifier.fillMaxWidth().height(64.dp).padding(start = 24.dp, end = 16.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    // The word carries what the dot did (the Logos board, B): paid green while watching, danger when disconnected,
    // else plain muted (the dot's grey outline is too faint for text). Light paid on ground is 4.50:1, right at the
    // text bar: a lighter paid or a darker ground in Theme.kt needs this word checked again.
    val shown = tone.takeUnless { it == Tone.MUTED || it == Tone.PLAIN }
    Row(
        Modifier.weight(1f).semantics(mergeDescendants = true) { contentDescription = said },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        PosBadge(LocalPointOfSale.current)
        // The state word is what the cashier reads this row for: the store and location give way first at large font sizes.
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (place != null) Text(
                place, fontSize = 13.sp, color = mk.muted, maxLines = 1,
                overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
            )
            if (place != null) Text(" · ", fontSize = 13.sp, color = mk.muted, maxLines = 1, softWrap = false)
            Text(
                state, fontSize = 13.sp, color = shown?.let { toneColor(it) } ?: mk.muted, fontWeight = shown?.let { FontWeight.Medium },
                maxLines = 1, softWrap = false,
            )
        }
    }
    CircleButton(MkIcons.Settings, "Settings", { actions.send(Action.OpenSettings) }, ground = mk.chip, tint = mk.ink, iconSize = 20.dp)
}

/** No sale yet, or not watching: the faded mark and what happens next. */
/**
 * The shift's header (the iPad round 3 designs, core.md decision 223): "This shift" with since when and a New shift button, the
 * takings and the paid count under them. New shift asks first, then opens a new window now.
 */
@Composable
private fun NewShiftHeader(label: String, header: ShiftHeader, actions: TillActions) {
    var asking by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f)) {
            Text(label, style = display(28, -0.4), modifier = Modifier.semantics { heading() })
            header.since?.let { Text(it, fontSize = 14.sp, color = mk.muted) }
        }
        OutlineButton(header.newShift.label, { asking = true }, height = 44.dp, enabled = header.newShift.enabled, size = 15)
    }
    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(header.total, style = display(34, -0.8))
        Text("· ${header.paid}", fontSize = 15.sp, color = mk.muted, modifier = Modifier.padding(bottom = 6.dp))
    }
    if (asking) {
        val confirm = header.confirm
        AlertDialog(
            onDismissRequest = { asking = false },
            containerColor = mk.card,
            titleContentColor = mk.ink,
            textContentColor = mk.muted,
            title = { Text(confirm.title) },
            text = { Text(confirm.message) },
            confirmButton = { LinkButton(confirm.start, { asking = false; actions.send(Action.NewShift) }) },
            dismissButton = { LinkButton(confirm.cancel, { asking = false }) },
        )
    }
}

@Composable
private fun Idle(idle: ShiftIdle, status: List<StatusChip>, actions: TillActions) = Column(
    Modifier.fillMaxWidth().heightIn(min = 320.dp).padding(24.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
    verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
) {
    TillWidget(80.dp)
    Text(idle.title, style = display(22, 0.0), color = mk.muted, textAlign = TextAlign.Center)
    Text(
        idle.hint, fontSize = 15.sp, lineHeight = 21.sp, color = mk.muted, textAlign = TextAlign.Center,
        modifier = Modifier.widthIn(max = 280.dp),
    )
    // The status chips (docs/decisions/core.md 127), drawn plainly as one line.
    if (status.isNotEmpty()) {
        Text(
            status.joinToString(" · ") { it.text }, fontSize = 14.sp, color = mk.muted, textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 280.dp),
        )
    }
    idle.start?.let { PrimaryButton(it.label, { actions.send(Action.StartShift) }, enabled = it.enabled) }
}

/** All, Needs action with its count, Paid: one selected, as tabs to TalkBack. */
@Composable
private fun Chips(chips: List<ViewChip>, onFilter: (ShiftFilter) -> Unit) = Row(
    // A large font can make the chips wider than the screen: they scroll rather than wrap a label into a blob.
    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, bottom = 6.dp).selectableGroup(),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    for (chip in chips) FilterChip(chip.label, chip.count?.toInt(), chip.selected) { onFilter(chip.filter) }
}

/** A filter chip's height at rest, and its label's inset from its ends. */
private val ChipHeight = 36.dp
private val ChipInset = 14.dp

/** The count badge's height at rest, and the inset that keeps it concentric with the chip's round end. */
private val BadgeHeight = 20.dp
private val BadgeInset = (ChipHeight - BadgeHeight) / 2

/**
 * One filter chip: a pill holding its [label] and, when there is one, its
 * [count] badge. The badge is a pill too, inset from the chip's end by as
 * much as from its top and bottom, so its curve follows the chip's at any
 * count and any font scale: it fills the chip's height inside that inset.
 */
@Composable
internal fun FilterChip(label: String, count: Int?, selected: Boolean, onClick: () -> Unit) {
    val shape = RoundedCornerShape(50)
    Row(
        Modifier.minimumInteractiveComponentSize().testTag(ChipTag)
            .heightIn(min = ChipHeight).height(IntrinsicSize.Min).clip(shape)
            .then(if (selected) Modifier.background(mk.ink) else Modifier.border(BorderStroke(1.5.dp, mk.edge), shape))
            .selectable(selected = selected, role = Role.Tab, onClick = onClick)
            .semantics { contentDescription = listOfNotNull(label, count?.toString()).joinToString(", ") }
            .then(if (count == null) Modifier.padding(horizontal = ChipInset) else Modifier.padding(start = ChipInset, end = BadgeInset, top = BadgeInset, bottom = BadgeInset)),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(label, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = if (selected) mk.ground else mk.ink, maxLines = 1, softWrap = false)
        if (count != null) {
            Box(
                // The tag sits where the ground is drawn, so a test measures the pill a cashier sees.
                Modifier.fillMaxHeight().heightIn(min = BadgeHeight).atLeastAsWideAsTall().testTag(BadgeTag)
                    .clip(shape).background(mk.warn).padding(horizontal = 6.dp),
                contentAlignment = Alignment.Center,
            ) { Text("$count", style = tabular, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = mk.ground, maxLines = 1, softWrap = false) }
        }
    }
}

internal const val ChipTag = "filter chip"
internal const val BadgeTag = "filter chip count"

/**
 * Holds what follows (the badge's ground included) at least as wide as the
 * height it is given, so a one-digit badge is a circle and a longer one a pill.
 */
private fun Modifier.atLeastAsWideAsTall() = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints.copy(minWidth = maxOf(constraints.minWidth, constraints.minHeight).coerceAtMost(constraints.maxWidth)))
    layout(placeable.width, placeable.height) { placeable.place(0, 0) }
}

/** The ground of a row's badge, by its glyph. */
private fun badgeGround(icon: StateIcon): Ground = when (icon) {
    StateIcon.CLOCK -> Ground.WARN
    StateIcon.WARNING -> Ground.DANGER
    StateIcon.TICK -> Ground.PAID
    StateIcon.RING, StateIcon.REFUND, StateIcon.CLOSE, StateIcon.CASH -> Ground.CHIP
}

/**
 * A row as the engine wrote it, in a stable class equal by value: the engine's records aren't stable to Compose, so a
 * view drawn again (a tick, the tray opening) would otherwise recompose every row on screen.
 */
@Immutable
private data class RowWords(
    val title: String,
    val sub: String,
    val tone: Tone,
    val quiet: Boolean,
    val icon: StateIcon,
    val opens: RowOpens,
    val step: Int,
    val confirmations: Int?,
    val required: Int?,
) {
    companion object {
        fun of(row: ShiftRow) = RowWords(
            row.title, listOfNotNull(row.sub, row.device).joinToString(" · "), row.tone, row.quiet, row.icon, row.opens,
            row.ring.step.toInt(), row.ring.confirmations?.toInt(), row.ring.required?.toInt(),
        )
    }
}

/** One sale: its state's icon, "Sale #m2Qe · $8.40", how it stands, and where a tap goes. */
@Composable
private fun SaleRow(id: String, row: RowWords, modifier: Modifier, onAsk: (() -> Unit)?, onOpen: () -> Unit) {
    // The swipe's way for TalkBack: the X's own words, on the row.
    val asks = onAsk?.let { remember(it) { listOf(CustomAccessibilityAction("Close sale") { it(); true }) } }
    Row(
        modifier.fillMaxWidth().heightIn(min = 64.dp)
            .clickable(onClickLabel = if (row.opens == RowOpens.CODE) "Show the code" else "Open the details", role = Role.Button, onClick = onOpen)
            .then(if (asks != null) Modifier.semantics { customActions = asks } else Modifier)
            .padding(start = 16.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        RowBadge(id, row.icon, row.step, row.confirmations, row.required)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(row.title, fontSize = 16.sp, color = if (row.quiet) mk.muted else mk.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            // The engine writes a live row's time left and draws it again each second (docs/decisions/core.md 37).
            Text(row.sub, fontSize = 13.sp, color = toneColor(row.tone), maxLines = 2, overflow = TextOverflow.Ellipsis, style = LocalTextStyle.current.merge(tabular))
        }
        if (row.opens == RowOpens.CODE) {
            Icon(MkIcons.Code, contentDescription = null, tint = mk.muted, modifier = Modifier.size(22.dp))
        } else {
            Icon(MkIcons.Chevron, contentDescription = null, tint = mk.muted, modifier = Modifier.size(16.dp))
        }
    }
}

/**
 * A row's badge: ring to tick, ring to clock, one face fading into the
 * next (the ring's own arc sweeps, M6). Its own skippable scope, so the
 * row's countdown ticking doesn't call into its transition: it runs only
 * when the glyph changes. Opening a sale that isn't live, the badge grows
 * into its page's hero; a paid sale stepping back, the hero shrinks into it.
 */
@Composable
private fun RowBadge(id: String, icon: StateIcon, step: Int, confirmations: Int?, required: Int?) = Box(Modifier.clearAndSetSemantics {}) {
    val fade = motion.fade.fraction
    AnimatedContent(targetState = icon, transitionSpec = { (fadeIn(fade) togetherWith fadeOut(fade)).using(null) }, label = "row badge") { face ->
        Box(Modifier.sharedSurface(badgeKey(id), Move.RowGrow, CircleShape, layer = Layers.OVER_GROUND)) {
            StateBadge(ringOf(step.toUByte(), confirmations?.toUInt(), required?.toUInt()), face, badgeGround(face), RingRowSize, 18.dp, quiet = true)
        }
    }
}
