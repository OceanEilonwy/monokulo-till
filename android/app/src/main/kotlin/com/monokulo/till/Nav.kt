@file:OptIn(ExperimentalSharedTransitionApi::class)

package com.monokulo.till

import uniffi.till_app.Screen
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.BoundsTransform
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.ExperimentalSharedTransitionApi
import androidx.compose.animation.SharedTransitionLayout
import androidx.compose.animation.SharedTransitionScope
import androidx.compose.animation.SharedTransitionScope.ResizeMode
import androidx.compose.animation.SharedTransitionScope.SharedContentConfig
import androidx.compose.animation.SharedTransitionScope.SharedContentState
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.animation.core.Transition
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.rememberTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import kotlin.math.min
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.abs
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onPlaced
import androidx.compose.ui.node.LayoutModifierNode
import androidx.compose.ui.node.LayoutAwareModifierNode
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.platform.InspectorInfo
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.invalidateDraw
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.unit.IntSize
import kotlin.math.roundToInt
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.graphics.GraphicsLayerScope
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.asAndroidPath
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.util.lerp
import androidx.compose.ui.util.packFloats
import androidx.compose.ui.util.unpackFloat1
import androidx.compose.ui.util.unpackFloat2
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.IntOffset
import android.util.Log
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.motion
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.collectLatest

/** How one screen reaches another (`docs/motion.md` M7), and back the same way reversed. */
sealed interface Move {
    /** A shift row grows into its sale; the sale shrinks back into its row. */
    data object RowGrow : Move

    /** A sale that came up by itself rises a little out of the shift: no row was tapped. */
    data object Rise : Move

    /** The sale's order card grows into the order page. */
    data object OrderGrow : Move

    /** The sale's code card grows into the enlarged code. */
    data object CodeGrow : Move

    /** Settings is a different place: it fades through. */
    data object FadeThrough : Move

    /** The next onboarding step slides in from the end ([forward]), the last from the start. */
    data class Slide(val forward: Boolean) : Move

    /** Anything else, and every pair under [Motion.Still]. */
    data object Crossfade : Move
}

/** Which [Move] takes the till from one screen to the next. */
object Nav {
    /**
     * How far down, of the height, a sale that came up by itself starts, rising into place: a slide, not a scale, so
     * its code is never drawn smaller than at rest and its words never through a changing scale (decision 198).
     */
    const val RISE_BY = 0.04f

    /** How small Settings starts as it fades in. */
    const val SETTINGS_FROM = 0.96f

    /** How far, of the width, the next onboarding step slides in from. */
    const val STEP_IN = 0.3f

    /** How far, of the width, the last onboarding step slides out. */
    const val STEP_OUT = 0.1f

    /**
     * Half way through a screen change: before it the screen being left is the picture, after it the one arriving,
     * and the code screen's bars turn with it.
     */
    const val HALF_WAY = 0.5f

    fun move(from: Screen, to: Screen, cameUpByItself: Boolean, still: Boolean): Move {
        val same = from.orderIdOrNull() != null && from.orderIdOrNull() == to.orderIdOrNull()
        return when {
            still -> Move.Crossfade
            from == Screen.Settings || to == Screen.Settings -> Move.FadeThrough
            from == Screen.Till && to is Screen.Sale -> if (cameUpByItself) Move.Rise else Move.RowGrow
            from is Screen.Sale && to == Screen.Till -> Move.RowGrow
            // Another sale came up in this one's place: it rises, as one coming up from the shift does.
            from is Screen.Sale && to is Screen.Sale -> Move.Rise
            same && (from is Screen.Order || to is Screen.Order) -> Move.OrderGrow
            same && (from is Screen.Code || to is Screen.Code) -> Move.CodeGrow
            else -> {
                val a = step(from)
                val b = step(to)
                if (a != null && b != null && a != b) Move.Slide(forward = b > a) else Move.Crossfade
            }
        }
    }

    /**
     * A screen's words at [toward] of a move (the stage's progress toward its target), fading through: [isArriving],
     * in over the last [arriving] of it; else out over the first [leaving]. With the two shares adding to less than
     * the whole move, the two screens' words are never both drawn (decision 188).
     */
    fun through(toward: Float, isArriving: Boolean, leaving: Float, arriving: Float): Float =
        if (isArriving) ((toward - 1f + arriving) / arriving).coerceIn(0f, 1f) else ((leaving - toward) / leaving).coerceIn(0f, 1f)

    /** Whether going [from] to [to] is going back: up from a sale to the shift, from a code to its sale, through setup backwards. */
    fun isBack(from: Screen, to: Screen): Boolean {
        val a = depth(from)
        val b = depth(to)
        return if (a != null && b != null) b < a else (step(to) ?: 0) < (step(from) ?: 0)
    }

    /** Where a sale's screens sit: the shift under its sales, a sale under its code and order. */
    private fun depth(screen: Screen): Int? = when (screen) {
        Screen.Till -> 0
        is Screen.Sale -> 1
        is Screen.Code, is Screen.Order -> 2
        Screen.Settings -> 3
        else -> null
    }

    /** Onboarding's order, the shift last. */
    private fun step(screen: Screen): Int? = when (screen) {
        Screen.Welcome -> 0
        Screen.Scan -> 1
        is Screen.Confirm -> 2
        is Screen.Connected -> 3
        Screen.PointOfSale -> 4
        is Screen.Locations -> 5
        Screen.Alerts -> 6
        Screen.Float -> 7
        Screen.Ready -> 8
        Screen.Till -> 9
        else -> null
    }
}

/**
 * [move] as the screens' enter and exit, with [motion]'s presets; going back puts the screen arriving underneath.
 * Everything spatial is on [Motion.track], a straight line: the stage's progress is what springs (decision 169).
 */
fun AnimatedContentTransitionScope<Screen>.transform(move: Move, motion: Motion, back: Boolean): ContentTransform {
    val fade = motion.fade.fraction
    val quick = motion.fadeQuick.fraction
    val transform = when (move) {
        // The grounds move between the two and each screen's words fade through over them, per draw. The screens
        // themselves don't fade: the one going stays as it is under the moving ground until it lands (it covers it by
        // then, or it is the one shown), so no frame composites a whole screen through an alpha layer (decisions 182, 185).
        // The order page draws no ground of its own while it grows: the sale's, the same colour, is under it throughout.
        Move.RowGrow, Move.CodeGrow, Move.OrderGrow -> EnterTransition.None togetherWith ExitTransition.KeepUntilTransitionsFinished
        // The sale slides up whole and opaque, its code included (decision 205: a customer's camera may already be on
        // it); its ground and words fade in per draw on the same progress ([sharedGround]'s fadesIn, [words]).
        Move.Rise -> slideInVertically(motion.track.spec(IntOffset.VisibilityThreshold)) { (it * Nav.RISE_BY).toInt() } togetherWith fadeOut(quick)
        Move.FadeThrough -> (fadeIn(fade) + scaleIn(motion.track.fraction, initialScale = Nav.SETTINGS_FROM)) togetherWith fadeOut(quick)
        is Move.Slide -> {
            val towards = if (move.forward) AnimatedContentTransitionScope.SlideDirection.Start else AnimatedContentTransitionScope.SlideDirection.End
            val slide = motion.track.spec(IntOffset.VisibilityThreshold)
            (slideIntoContainer(towards, slide) { (it * Nav.STEP_IN).toInt() } + fadeIn(fade)) togetherWith
                (slideOutOfContainer(towards, slide) { (it * Nav.STEP_OUT).toInt() } + fadeOut(quick))
        }
        // Under Still a change to or from a screen that may show a payment code is a cut: its code and its white are
        // whole on the first frame, never two codes of two sizes through each other (decision 205).
        Move.Crossfade -> if (mayShowCode(initialState) || mayShowCode(targetState)) EnterTransition.None togetherWith ExitTransition.None else fadeIn(quick) togetherWith fadeOut(quick)
    }
    // Screens fill the window: no size to animate.
    return transform.apply { targetContentZIndex = if (back) -1f else 0f }.using(null)
}

/** A sale's screen (its code while it's live) or its enlarged code. */
private fun mayShowCode(screen: Screen) = screen is Screen.Sale || screen is Screen.Code

/** The screen transition in progress and the scopes a shared surface needs, for the screens under [TillStage]. */
@Stable
class Stage internal constructor(
    val shared: SharedTransitionScope,
    val content: AnimatedVisibilityScope,
    private val transition: Transition<Screen>,
    private val held: HeldMove,
    private val scrub: BackScrub,
    private val shownState: State<Float>,
    internal val screen: Screen,
    private val progress: () -> Float,
    private val root: StageRoot,
    internal val geometry: StageGeometry,
) {
    /**
     * The move under way between the two screens of the transition, read when a surface asks, not in composition;
     * null at rest, so nothing done "during" a move outlives it (a code or order screen is the same order on both
     * sides of a still stage, which [Nav.move] would otherwise call its grow: decision 187).
     */
    fun moving(): Move? = held.during(transition.currentState, transition.targetState)

    /** Whether a back gesture holds the stage: it may yet spring back, so the screen it would leave is still the one up. */
    val scrubbing: Boolean get() = scrub.active

    /**
     * How far this screen is through arriving (0 to 1) or leaving (1 to 0),
     * on [Motion.track]: the line the surfaces' bounds move on, so a part
     * driven by it stays in step with them. Read in draw or layout.
     */
    fun shown(): Float = shownState.value

    /** [shown] while the screens change by [move]; 1 otherwise, as at rest. */
    fun shownDuring(move: Move): Float = if (moving() == move) shownState.value else 1f

    /**
     * This screen's face of a surface while the screens change by [move]:
     * the arriving face fades in over the first [Motion.crossfadeOnTrack]
     * of the move, the leaving one out over the last, so the surface always
     * shows one or the other. A function of the stage's progress toward
     * this screen, so a cancelled gesture fades back the way it came; 1 at
     * rest.
     */
    fun faceDuring(move: Move, share: Float): Float {
        if (moving() != move) return 1f
        // The stage's own progress, toward its target: the same number for both screens, whichever way it runs.
        val toward = progress()
        val mine = if (transition.targetState == screen) toward else 1f - toward
        return (mine / share).coerceIn(0f, 1f)
    }

    /**
     * This screen's own words and parts while the screens change by
     * [move]: they fade through, never across (decision 188). Leaving,
     * they are gone by [Motion.fadeOutOnTrack] of the move; arriving, they
     * come in over its last [Motion.crossfadeOnTrack]; so two screens'
     * words are never drawn at once, and each is drawn at the size it was
     * laid out at, never scaled (decision 185). A function of the stage's
     * progress toward this screen, so a cancelled gesture fades back the
     * way it came; 1 at rest.
     */
    fun throughDuring(move: Move, leaving: Float, arriving: Float): Float =
        if (moving() != move) 1f else Nav.through(progress(), transition.targetState == screen, leaving, arriving)

    /**
     * Whether this screen has been left and the stage is at rest on another: the frame on which the change lands
     * still composes it (its exit holds until the transition finishes), and it would draw whole, over the screen
     * arriving underneath on the way back, for that one frame. Read in draw.
     */
    fun gone(): Boolean = transition.currentState == transition.targetState && transition.targetState != screen

    /** Whether this screen is the one the stage is going to. */
    fun arriving(): Boolean = transition.targetState == screen

    /**
     * The sale a change to this screen is leaving (a Back, or a paid page stepping back, onto the shift), null at
     * rest or from anything else. Read in composition, so it recomposes its reader twice a change, as the change
     * starts and as it lands, never a frame.
     */
    val leavingSale: String?
        get() = if (transition.targetState == screen) (transition.currentState as? Screen.Sale)?.orderId else null

    /** Whether the stage rests on this screen: the change onto it has landed. */
    fun landed(): Boolean = transition.currentState == screen && transition.targetState == screen

    /** The stage's own progress toward the screen it is going to: the one number every part of a move reads. */
    internal fun toward(): Float = progress()

    /** Whether [move] is the one under way: read in draw, where it's needed. */
    fun isMoving(move: Move): Boolean = moving() == move

    /** Where [coordinates] sit in the stage's overlay, where the grounds move: read in draw. */
    fun inOverlay(coordinates: LayoutCoordinates): Offset = root.coordinates?.localPositionOf(coordinates, Offset.Zero) ?: Offset.Zero
}

/**
 * The move of the screen change under way, decided once per change
 * (decision 206): the first to ask, the transform or a surface drawing,
 * reads [cameUpByItself] for the pair, and everything after gets the same
 * answer until the stage is at rest, so a flag the till sets mid-move
 * can't turn a row's grow into a rise half way through it.
 */
internal class HeldMove(private val cameUpByItself: () -> Boolean, private val still: Boolean) {
    private var from: Screen? = null
    private var to: Screen? = null
    private var move: Move = Move.Crossfade

    /** Which change this is, counted up as each starts: a surface's course starts again on a new one. */
    var generation = 0L
        private set

    /** Whether this change took over another mid-move (a Back during a grow), rather than starting from rest. */
    var interrupted = false
        private set

    /**
     * The move from [from] to [to]; null at rest ([from] is [to]), which also forgets the last change. A change is its
     * target: the same target asked again with another [from] mid-change (the stage's current screen read on another
     * frame) is the same change, its move and generation kept, so nothing it reads (the till's came-up-by-itself above
     * all) can turn it into another half way; only a new target, a takeover, starts a new generation (decisions 245,
     * 249, 284).
     */
    fun during(from: Screen, to: Screen): Move? {
        if (from == to) {
            this.from = null
            this.to = null
            return null
        }
        if (to != this.to || this.from == null) {
            interrupted = this.from != null
            generation++
            this.from = from
            this.to = to
            move = Nav.move(from, to, cameUpByItself(), still)
        }
        return move
    }
}

/** The stage's own layout, where the moving grounds are drawn. */
class StageRoot {
    var coordinates: LayoutCoordinates? = null
        internal set
}

/** A back gesture over the stage, from its start until it has committed or sprung back. */
@Stable
class BackScrub {
    var active by mutableStateOf(false)
        internal set
}

/**
 * The one thing that moves [state] to the model's [screen]: every change
 * the till or a tap makes runs on [Motion.stage], and none while a back
 * gesture holds the stage. When a gesture lets go, committed or cancelled,
 * the stage goes to wherever the model is by then, so a sale coming up or
 * a payment landing under the finger is followed rather than lost
 * (decision 186): at rest the stage and the model always agree.
 */
@Composable
fun FollowScreen(state: SeekableTransitionState<Screen>, scrub: BackScrub, screen: State<Screen>, onLanded: (Screen) -> Unit = {}) {
    val m = motion
    // The one landing report (decision 269): the stage at rest on the model's screen, once per landing, so the model
    // knows a sale arriving now finds the shift at rest (it rises) and one arriving before found a move (it waits in
    // the tray). Read off the frame, as the stage's state changes; nothing recomposes for it.
    val landed by rememberUpdatedState(onLanded)
    LaunchedEffect(state) {
        snapshotFlow { state.currentState.takeIf { it == state.targetState && it == screen.value } }.collect { at -> at?.let { landed(it) } }
    }
    LaunchedEffect(state, scrub, m) {
        snapshotFlow { if (scrub.active) null else screen.value }.collectLatest { s ->
            if (s == null || (state.currentState == s && state.targetState == s)) return@collectLatest
            try {
                // From rest, the screen arriving is composed and drawn once at 0 before the spring's clock starts, as
                // a back gesture's is: its first frame is the change's heaviest (a whole screen composed), and counted
                // on the clock it put the stage half way there on the first frame drawn (decision 235). Mid-move the
                // new target is just retargeted, from where the move got to, a move still at 0 too: a seek there
                // would hold the stage at 0 until the move it took over had run its course (decision 249).
                if (m.stage != null && state.currentState == state.targetState) {
                    state.seekTo(0f, s)
                    // The next frame composes it; the spring's first frame, after it, starts the clock.
                    withFrameNanos {}
                }
                state.animateTo(s, m.stage)
            } catch (e: CancellationException) {
                // A gesture's seek took the stage (its mutex cancels this run, not this driver): the gesture's
                // end emits again and the stage is put back on the model's screen then.
                currentCoroutineContext().ensureActive()
            }
        }
    }
}

/**
 * Android's back over the stage. What it goes back from is the model's
 * screen as the gesture starts ([current]), and where to is asked of
 * [target] then too (the model as it is, not as the last composition saw
 * it); never the stage's own current state, which mid-move is still the
 * screen before. On a settled stage the gesture seeks [state] toward the
 * target, so each part of the move is as far along as the finger (decision
 * 169); during a move, or with no target, or under Still, there's no
 * preview. Released, [onCommit] is told the screen it left and the stage
 * follows the model from where the finger left it ([FollowScreen]);
 * cancelled, the stage springs back to the model's screen. [scrub] is held
 * from the gesture's start until it has committed or been cancelled.
 */
@Composable
fun StageBack(
    enabled: Boolean,
    state: SeekableTransitionState<Screen>,
    scrub: BackScrub,
    current: () -> Screen,
    target: () -> Screen?,
    onCommit: (from: Screen) -> Unit,
) {
    val m = motion
    PredictiveBackHandler(enabled) { progress ->
        val from = current()
        val to = target()
        val settled = state.currentState == from && state.targetState == from
        if (to == null || m.still || !settled) {
            // No preview: a move is running (Back retargets it, from wherever it is), or there's nowhere to scrub to.
            progress.collect {}
            onCommit(from)
            return@PredictiveBackHandler
        }
        scrub.active = true
        try {
            // The screen back goes to is composed and laid out at 0 before the finger moves anything, so every part's
            // track starts at the same 0 and a progress of p is p of the way for all of them.
            state.seekTo(0f, to)
            withFrameNanos {}
            withFrameNanos {}
            progress.collect { state.seekTo(it.progress, to) }
            onCommit(from)
        } finally {
            // Committed or cancelled, the stage is the driver's again: it goes on to the model's screen from here.
            scrub.active = false
        }
    }
}

val LocalStage = staticCompositionLocalOf<Stage?> { null }

/**
 * The till's screens, one at a time, each reaching the next by its [Move]
 * (M7). [state] is seekable, so a back gesture can scrub the transition
 * before it commits; run it with [Motion.stage] (`state.animateTo(screen,
 * motion.stage)`), so a seek and a spring move every part along the same
 * line. [cameUpByItself] says the screen was changed by the
 * till (a new sale coming up), not the cashier.
 */
@Composable
fun TillStage(
    state: SeekableTransitionState<Screen>,
    cameUpByItself: () -> Boolean,
    scrub: BackScrub = remember { BackScrub() },
    content: @Composable (Screen) -> Unit,
) {
    val m = motion
    val transition = rememberTransition(state, label = "screen")
    val root = remember { StageRoot() }
    val held = remember(m) { HeldMove(cameUpByItself, m.still) }
    val geometry = remember(transition, held) { StageGeometry(transition, held, { state.fraction }, root, m.fadeOutOnTrack) }
    // One collector on whether a change runs (nothing recomposes for it; the flow runs off the frame). While one does,
    // the frame being drawn, so each surface is placed once a frame however many parts ask. At rest, the held move is
    // forgotten, so the next change between the same two screens is read afresh with no surface needing to draw at
    // rest for it; only if the stage still rests when the collector gets there, so a rest reported late, after the
    // next change has begun, never makes that change read its move again half way (decision 284).
    LaunchedEffect(transition, geometry, held) {
        snapshotFlow { transition.currentState != transition.targetState }.collect { changing ->
            if (!changing) {
                val at = transition.currentState
                if (at == transition.targetState) held.during(at, at)
                return@collect
            }
            try {
                while (transition.currentState != transition.targetState) withFrameNanos { geometry.frame = it }
            } finally {
                geometry.frame = 0L
            }
        }
    }
    // The debug build's stage trace: compiled out of a release build (BuildConfig.DEBUG is a constant), and off
    // until switched on (decision 237).
    if (BuildConfig.DEBUG) StageTrace(state, transition, held, geometry, scrub)
    SharedTransitionLayout(Modifier.onPlaced { root.coordinates = it }) {
        val shared = this
        Box {
        transition.AnimatedContent(
            transitionSpec = {
                transform(held.during(initialState, targetState) ?: Move.Crossfade, m, Nav.isBack(initialState, targetState))
            },
        ) { screen ->
            val visibility = (this as AnimatedVisibilityScope).transition
            val shown = visibility.animateFloat({ m.track.fraction }, label = "shown") { if (it == EnterExitState.Visible) 1f else 0f }
            val stage = remember(m) { Stage(shared, this, transition, held, scrub, shown, screen, { state.fraction }, root, geometry) }
            // A screen left draws nothing on the frame its change lands: the one arriving is the picture by then.
            Box(Modifier.drawWithContent { if (!stage.gone()) drawContent() }) {
                CompositionLocalProvider(LocalStage provides stage) { content(screen) }
            }
        }
        // The moving grounds and codes, drawn by the stage itself over both screens while a change runs (decision 232):
        // a window-sized layer each, so a surface larger than either of its ends' nodes is never cut to one of them.
        // At rest they draw nothing and are drawn in place, empty.
        val grounds = remember(geometry) { GroundsPainter(geometry) }
        Box(Modifier.matchParentSize().renderInSharedTransitionScopeOverlay(Layers.GROUND) { geometry.moving() }.drawBehind { with(grounds) { paint() } })
        Box(Modifier.matchParentSize().renderInSharedTransitionScopeOverlay(Layers.CODE) { geometry.moving() }.drawBehind { geometry.paintCodes(this) })
        }
    }
}

/**
 * Every shared surface's two ends by key, and where each surface is on
 * the frame being drawn: the one read of a move (decision 232). A surface's
 * bounds on a frame are the straight line between its two ends' places,
 * as laid out on that frame, at the stage's own progress (the number the
 * words read, a back gesture seeks and [Motion.stage] springs), so a
 * ground, the code on it, the words clipped to it and a seeked frame all
 * agree; nothing is placed an update behind and reached past.
 */
internal class StageGeometry(
    private val transition: Transition<Screen>,
    private val held: HeldMove,
    private val progress: () -> Float,
    private val root: StageRoot,
    private val fadeOut: Float,
) {
    private val courses = Courses()

    /**
     * The frame being drawn while a change runs (its frame time, from [TillStage]'s frame loop), 0 when no loop is
     * ticking: a course placed on this frame is not placed again by the next reader of the same frame.
     */
    internal var frame = 0L

    /** [key]'s course while one of its ends is on a screen; null otherwise. A find, never a make: read in draw. */
    fun find(key: String): Course? = courses.find(key)

    /** Whether a change is under way: the stage's overlay layers draw only then. */
    fun moving(): Boolean = held.during(transition.currentState, transition.targetState) != null

    /**
     * Every moving code's bitmap, scaled as one image to its course on this frame (principle 3: never re-laid out).
     * A code whose sale is no longer live where the stage is going (paid or ended on the enlarged code) has no
     * partner: it stays where it was, fading out by [fadeOut] inside its white as that closes into the page's hero,
     * never outside it (decision 241).
     */
    fun paintCodes(scope: DrawScope) {
        for (i in 0 until courses.size()) {
            val c = courses.at(i)
            val during = c.during ?: continue
            if (locate(c, during)) {
                val image = c.to?.bitmap ?: continue
                val l = c.left.roundToInt()
                val t = c.top.roundToInt()
                scope.drawImage(
                    image, dstOffset = IntOffset(l, t), dstSize = IntSize(c.right.roundToInt() - l, c.bottom.roundToInt() - t),
                    filterQuality = FilterQuality.None,
                )
            } else {
                fading(scope, c, during)
            }
        }
    }

    /** [code]'s leaving end with no partner, drawn at its own place clipped to its [End.white]'s course on this frame. */
    private fun fading(scope: DrawScope, code: Course, during: Move) {
        val leaving = code.endOn(transition.currentState) ?: return
        val image = leaving.bitmap ?: return
        val white = find(leaving.white ?: return) ?: return
        if (!locate(white, during)) return
        val alpha = Nav.through(progress(), isArriving = false, leaving = fadeOut, arriving = 1f)
        if (alpha <= 0f) return
        val r = root.coordinates ?: return
        val c = leaving.coordinates ?: return
        val at = r.localPositionOf(c, Offset.Zero)
        scope.clipRect(white.left, white.top, white.right, white.bottom) {
            drawImage(
                image, dstOffset = IntOffset(at.x.roundToInt(), at.y.roundToInt()), dstSize = c.size, alpha = alpha,
                filterQuality = FilterQuality.None,
            )
        }
    }

    internal fun size(): Int = courses.size()

    internal fun at(i: Int): Course = courses.at(i)

    fun add(end: End) = courses.add(end)

    fun remove(end: End) = courses.remove(end)

    /**
     * Puts [course]'s place on this frame of [during] in it: false at rest,
     * under another move, or with either end missing (a row scrolled off the
     * shift has no place to go to).
     */
    fun locate(course: Course, during: Move): Boolean {
        val from = transition.currentState
        val to = transition.targetState
        if (held.during(from, to) != during) return course.lost()
        // Placed already on this frame (an end node, the words on it, the stage's painter all ask): the same place.
        val p = progress()
        if (course.placedOn(frame, held.generation, p)) return true
        val r = root.coordinates ?: return course.lost()
        val a = course.endOn(from) ?: return course.lost()
        val b = course.endOn(to) ?: return course.lost()
        val ca = a.coordinates!!
        val cb = b.coordinates!!
        val o = r.localPositionOf(ca, Offset.Zero)
        val e = r.localPositionOf(cb, Offset.Zero)
        course.place(
            a, b, o.x, o.y, o.x + ca.size.width, o.y + ca.size.height, e.x, e.y, e.x + cb.size.width, e.y + cb.size.height,
            p, held.generation, held.interrupted, frame,
        )
        return true
    }
}

/**
 * The courses of the surfaces on screen, by key and in a list walked by
 * index as the stage draws (so a frame makes no iterator). A course is
 * made with its first end (on attach, never in draw) and goes with its
 * last, so the stage keeps one per surface on screen, not one per sale
 * ever opened, and a frame walks only those.
 */
internal class Courses {
    private val byKey = HashMap<String, Course>()
    private val list = ArrayList<Course>()

    fun find(key: String): Course? = byKey[key]

    fun size(): Int = list.size

    fun at(i: Int): Course = list[i]

    fun add(end: End) {
        byKey.getOrPut(end.key) { Course(end.key).also { list += it } }.add(end)
    }

    fun remove(end: End) {
        val c = byKey[end.key] ?: return
        c.remove(end)
        if (c.ends.isEmpty()) {
            byKey.remove(end.key)
            list.remove(c)
        }
    }
}

/**
 * One end of a shared surface: where its screen lays it out, the move it
 * is shared by, and how it looks there: a ground's [look], or a code's
 * [bitmap].
 */
internal class End(val key: String, val screen: Screen, val during: Move) {
    var coordinates: LayoutCoordinates? = null
    var look: GroundLook? = null
    var bitmap: ImageBitmap? = null

    /** A code's white: the ground key it fades out inside when it has no partner ([StageGeometry.paintCodes]). */
    var white: String? = null
}

/**
 * One shared surface's line over a move, and where it is on the frame
 * drawn: from [sl]..[sb] (the leaving end, or where the surface was when a
 * new change took it over mid-move) to [el]..[eb] (the arriving end), at
 * [p], is [left]..[bottom]. Plain floats, so a frame allocates nothing.
 */
internal class Course(val key: String) {
    val ends = ArrayList<End>(2)

    /** The move its ends are shared by: what the stage draws it during, whatever was placed last. */
    val during: Move? get() = if (ends.isEmpty()) null else ends[0].during
    var from: End? = null
        private set
    var to: End? = null
        private set
    var p = 0f
        private set
    var sl = 0f
        private set
    var st = 0f
        private set
    var sr = 0f
        private set
    var sb = 0f
        private set
    var el = 0f
        private set
    var et = 0f
        private set
    var er = 0f
        private set
    var eb = 0f
        private set
    var left = 0f
        private set
    var top = 0f
        private set
    var right = 0f
        private set
    var bottom = 0f
        private set

    /** Whether [left]..[bottom] is where the surface was drawn on the last frame of a move. */
    private var valid = false
    private var generation = -1L
    private var snapped = false
    private var frame = 0L
    private var kl = 0f
    private var kt = 0f
    private var kr = 0f
    private var kb = 0f

    /** The change this course was last drawn in, -1 if it was not drawn on the last frame of one. */
    val drawnIn: Long get() = if (valid) generation else -1L

    fun add(end: End) {
        ends += end
    }

    /** [end] has left its screen; with none left the course goes from the stage ([Courses]), its code bitmap with it. */
    fun remove(end: End) {
        ends.remove(end)
    }

    fun endOn(screen: Screen): End? {
        for (i in ends.indices) {
            val e = ends[i]
            if (e.screen == screen && e.coordinates?.isAttached == true) return e
        }
        return null
    }

    fun lost(): Boolean {
        valid = false
        return false
    }

    /** Whether it was placed on [frame] (not 0: a frame loop is ticking) of [generation]'s change at [p] already. */
    fun placedOn(frame: Long, generation: Long, p: Float): Boolean =
        valid && frame != 0L && frame == this.frame && generation == this.generation && p == this.p

    /**
     * Where the surface is at [p] of [generation]'s change from end [a], laid
     * out at [ol]..[ob] in the stage, to end [b] at [el]..[eb].
     */
    fun place(
        a: End, b: End, ol: Float, ot: Float, or: Float, ob: Float, el: Float, et: Float, er: Float, eb: Float,
        p: Float, generation: Long, interrupted: Boolean, frame: Long = 0L,
    ) {
        if (generation != this.generation) {
            // A change that took over another mid-move starts from where this surface was drawn last, not from the
            // leaving end's rest place, so nothing jumps (the progress starts again from 0 toward the new screen).
            snapped = interrupted && valid && this.generation == generation - 1
            if (snapped) {
                kl = left
                kt = top
                kr = right
                kb = bottom
            }
            this.generation = generation
        }
        if (snapped) line(kl, kt, kr, kb, el, et, er, eb, p) else line(ol, ot, or, ob, el, et, er, eb, p)
        from = a
        to = b
        valid = true
        this.frame = frame
    }

    /** The straight line from one box to the other, and the point [p] of the way along it: where the surface is now. */
    fun line(sl: Float, st: Float, sr: Float, sb: Float, el: Float, et: Float, er: Float, eb: Float, p: Float) {
        this.sl = sl
        this.st = st
        this.sr = sr
        this.sb = sb
        this.el = el
        this.et = et
        this.er = er
        this.eb = eb
        this.p = p
        left = lerp(sl, el, p)
        top = lerp(st, et, p)
        right = lerp(sr, er, p)
        bottom = lerp(sb, eb, p)
    }

    /** Whether the surface on this frame holds the box [l]..[b] (in the overlay), to half a pixel. */
    fun holds(l: Float, t: Float, r: Float, b: Float): Boolean =
        left <= l + EDGE && top <= t + EDGE && right >= r - EDGE && bottom >= b - EDGE

    /**
     * The part of the move over which the surface holds the box [l]..[b],
     * packed as two floats ([packFloats]): from its first ([heldFrom]) to
     * its second ([heldUntil]), the first above the second when it never
     * does. Its edges move on straight lines, so this is exact. Nothing is
     * kept: every word run on one ground asks for its own box.
     */
    fun holding(l: Float, t: Float, r: Float, b: Float): Long {
        var lo = 0f
        var hi = 1f
        // Each edge narrows lo..hi to where s + (e - s) p ≤ c.
        for (edge in 0 until 4) {
            val s: Float
            val e: Float
            val c: Float
            when (edge) {
                0 -> { s = sl; e = el; c = l + EDGE }
                1 -> { s = st; e = et; c = t + EDGE }
                2 -> { s = -sr; e = -er; c = -(r - EDGE) }
                else -> { s = -sb; e = -eb; c = -(b - EDGE) }
            }
            val d = e - s
            if (abs(d) < EDGE) {
                if (s > c) return packFloats(2f, -1f)
                continue
            }
            val x = (c - s) / d
            if (d > 0f) hi = min(hi, x) else lo = maxOf(lo, x)
        }
        return packFloats(lo, hi)
    }

    /**
     * The first point of the move from which the surface no longer meets
     * the box [l]..[b] to its end (2 if it still meets it there): where
     * words arriving under a shrinking ground may start to show.
     */
    fun clearOf(l: Float, t: Float, r: Float, b: Float): Float =
        min(min(atLeastFrom(sl, el, r), atLeastFrom(st, et, b)), min(atLeastFrom(-sr, -er, -l), atLeastFrom(-sb, -eb, -t)))

    /** The least p from which s + (e - s) p ≥ c holds to 1, or 2 if it doesn't hold at 1. */
    private fun atLeastFrom(s: Float, e: Float, c: Float): Float {
        if (e < c) return 2f
        if (s >= c) return 0f
        return ((c - s) / (e - s)).coerceIn(0f, 1f)
    }

    companion object {
        /** Half a pixel: a box laid out on a pixel edge is held by a surface on the same edge. */
        private const val EDGE = 0.5f

        /** Where a [holding] range starts. */
        fun heldFrom(range: Long): Float = unpackFloat1(range)

        /** Where a [holding] range ends. */
        fun heldUntil(range: Long): Float = unpackFloat2(range)
    }
}

/**
 * A shared surface's node: it is its key's end on this screen while
 * attached, and where it is placed is where the end is laid out.
 */
private abstract class EndNode(var geometry: StageGeometry, var end: End) : Modifier.Node(), LayoutAwareModifierNode {
    protected var placed: LayoutCoordinates? = null

    override fun onAttach() = geometry.add(end)

    override fun onDetach() {
        geometry.remove(end)
        end.coordinates = null
        placed = null
    }

    override fun onPlaced(coordinates: LayoutCoordinates) {
        placed = coordinates
        end.coordinates = coordinates
    }

    /** A new stage, key or screen for this node: a new end, with the look the old one had. */
    fun rebind(geometry: StageGeometry, key: String, screen: Screen, during: Move) {
        if (geometry === this.geometry && key == end.key && screen == end.screen && during == end.during) return
        val look = end.look
        val bitmap = end.bitmap
        if (isAttached) this.geometry.remove(end)
        this.geometry = geometry
        end = End(key, screen, during).also {
            it.look = look
            it.bitmap = bitmap
        }
        if (isAttached) {
            geometry.add(end)
            end.coordinates = placed
        }
    }
}

/**
 * Where a shared surface is on this frame of its move, as one screen sees
 * it: for the surface's own drawing, the words clipped to it, what waits
 * for it to pass, and a device test or the stage trace reading it.
 */
@Stable
class GroundTrack(val key: String) {
    internal var stage: Stage? = null
        private set
    internal var during: Move = Move.Crossfade
        private set
    private var course: Course? = null

    internal fun bind(stage: Stage, during: Move) {
        this.stage = stage
        this.during = during
    }

    /**
     * Whether the surface is moving on this frame (both its ends are there), its place then in [left]..[bottom]. Its
     * course is looked up by key each time, never held from a bind: a row reused by the shift's list, or a course that
     * went with its last end, can't leave a track pointing at one no longer drawn.
     */
    fun locate(): Boolean {
        val s = stage ?: return false
        val c = s.geometry.find(key) ?: return false
        course = c
        return s.geometry.locate(c, during)
    }

    /** The course [locate] last put this frame's place in. */
    internal val located: Course get() = course!!

    internal val left: Float get() = located.left
    internal val top: Float get() = located.top
    internal val right: Float get() = located.right
    internal val bottom: Float get() = located.bottom
}

/**
 * Where a screen's grounds get their tracks: one per surface, made once. A
 * test seam: a device test provides its own maker, to read where each
 * surface is drawn on a frame of a running move; the app's is the plain
 * constructor.
 */
val LocalGroundTracks = staticCompositionLocalOf<(key: String) -> GroundTrack> { { GroundTrack(it) } }

/** [key]'s track, for the surface and the words on it. */
@Composable
fun rememberGroundTrack(key: String): GroundTrack {
    val make = LocalGroundTracks.current
    return remember(key, make) { make(key) }
}

/** How a ground looks at rest on one side of a move: its colour, and its top and bottom corners. */
@Immutable
class GroundLook(val color: Color, val top: Dp, val bottom: Dp = top)

/**
 * [key]'s ground: an [own] coloured, rounded rectangle with nothing in it,
 * which moves between the two screens' nodes while they change by
 * [during]. On every frame of the move the stage draws it once, in its
 * overlay under everything on it, at the place [StageGeometry] puts it
 * (the straight line between the two ends at the stage's progress), its
 * colour and corners going from the leaving end's look to the arriving
 * one's on the same progress; neither end draws itself. What sits on
 * a ground is not inside it: its words fade through at their own size
 * ([words]), the code is its own surface over it ([sharedCode]), so nothing
 * on a ground is drawn through a changing scale (decision 185). With no
 * partner (a sale whose row is off the shift), a ground fades with its
 * screen. [track] is the screen's view of it, for the words on it.
 */
@Composable
fun Modifier.sharedGround(key: String, during: Move, own: GroundLook, track: GroundTrack? = null, fadesIn: Move? = null): Modifier {
    val stage = LocalStage.current
    val painter = remember { GroundPainter() }
    if (stage == null || motion.still) return drawBehind { with(painter) { paint(own.color, own.top.toPx(), own.bottom.toPx()) } }
    val ground = track ?: remember(key) { GroundTrack(key) }
    ground.bind(stage, during)
    return this.then(GroundElement(stage, ground, own, fadesIn, paints = true))
}

/**
 * Where [track]'s ground lands on this screen, looking like [look] as it
 * gets there, with nothing drawn of it here at rest: what is laid out on
 * this node draws itself (a paid page's hero, which the enlarged code's
 * white closes into when the payment lands on it: decision 241).
 */
@Composable
fun Modifier.groundEnd(track: GroundTrack, during: Move, look: GroundLook): Modifier {
    val stage = LocalStage.current ?: return this
    if (motion.still) return this
    track.bind(stage, during)
    return this.then(GroundElement(stage, track, look, fadesIn = null, paints = false))
}

private class GroundElement(val stage: Stage, val track: GroundTrack, val own: GroundLook, val fadesIn: Move?, val paints: Boolean) :
    ModifierNodeElement<GroundNode>() {
    override fun create() = GroundNode(stage.geometry, End(track.key, stage.screen, track.during).also { it.look = own }, stage, track, own, fadesIn, paints)

    override fun update(node: GroundNode) {
        node.rebind(stage.geometry, track.key, stage.screen, track.during)
        node.end.look = own
        node.stage = stage
        node.track = track
        node.own = own
        node.fadesIn = fadesIn
        node.paints = paints
        node.invalidateDraw()
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "sharedGround"
        properties["key"] = track.key
        properties["during"] = track.during
        properties["own"] = own
        properties["fadesIn"] = fadesIn
        properties["paints"] = paints
    }

    override fun equals(other: Any?): Boolean = other is GroundElement && other.stage === stage && other.track === track &&
        other.own === own && other.fadesIn == fadesIn && other.paints == paints

    override fun hashCode(): Int = System.identityHashCode(track) * 31 + System.identityHashCode(own)
}

private class GroundNode(
    geometry: StageGeometry,
    end: End,
    var stage: Stage,
    var track: GroundTrack,
    var own: GroundLook,
    var fadesIn: Move?,
    var paints: Boolean,
) : EndNode(geometry, end), DrawModifierNode {
    private val painter = GroundPainter()

    override fun ContentDrawScope.draw() {
        if (!paints) return drawContent()
        val stage = stage
        val track = track
        val own = own
        when {
            !stage.isMoving(track.during) -> {
                // Arriving by [fadesIn] (a sale rising by itself), it fades in on the stage's line; leaving by it, out.
                val f = fadesIn
                val seen = if (f != null && stage.isMoving(f)) stage.shown() else 1f
                with(painter) { paint(own.color.copy(alpha = own.color.alpha * seen), own.top.toPx(), own.bottom.toPx()) }
            }
            // Moving: the stage draws the one ground where its line puts it on this frame, over both screens; neither
            // end draws itself (decision 232).
            track.locate() -> Unit
            // No partner (a sale whose row is off the shift): it fades with its screen, the one under it showing through.
            else -> with(painter) { paint(own.color.copy(alpha = own.color.alpha * stage.shown()), own.top.toPx(), own.bottom.toPx()) }
        }
        drawContent()
    }
}

/** Every moving ground on this frame, drawn by the stage in its overlay under everything on them. */
private class GroundsPainter(private val geometry: StageGeometry) {
    private val painter = GroundPainter()

    fun DrawScope.paint() {
        for (i in 0 until geometry.size()) {
            val c = geometry.at(i)
            val during = c.during ?: continue
            if (!geometry.locate(c, during) || c.to?.look == null) continue
            with(painter) { paintCourse(c) }
        }
    }
}

/** A rounded rectangle with its own top and bottom corners; its path made once and reused. */
private class GroundPainter {
    private val path = Path()
    private val radii = FloatArray(8)

    fun DrawScope.paint(color: Color, top: Float, bottom: Float) {
        if (top == bottom) {
            drawRoundRect(color, cornerRadius = CornerRadius(top))
            return
        }
        rounded(0f, 0f, size.width, size.height, top, bottom)
        drawPath(path, color)
    }

    /** [course]'s ground on this frame, in the stage's overlay: its colour and corners [Course.p] of the way between its ends' looks. */
    fun DrawScope.paintCourse(course: Course) {
        val a = course.from?.look
        val b = course.to?.look ?: return
        val from = a ?: b
        val p = course.p
        val top = lerp(from.top.toPx(), b.top.toPx(), p)
        val bottom = lerp(from.bottom.toPx(), b.bottom.toPx(), p)
        rounded(course.left, course.top, course.right, course.bottom, top, bottom)
        drawPath(path, lerp(from.color, b.color, p))
    }

    private fun rounded(l: Float, t: Float, r: Float, b: Float, top: Float, bottom: Float) {
        radii.fill(top, 0, 4)
        radii.fill(bottom, 4, 8)
        path.rewind()
        path.asAndroidPath().addRoundRect(l, t, r, b, radii, android.graphics.Path.Direction.CW)
    }
}

/**
 * The code's [bitmap] as [key]'s surface between a sale and its enlarged
 * code ([during]): while they change, the stage draws it once, over
 * everything (its overlay's [Layers.CODE]), as one image scaled to the
 * straight line between the two codes' places at the stage's progress:
 * the same read that places the white it sits on ([sharedGround]), so the
 * quiet zone runs from one rest margin to the other and is never below
 * the smaller (decision 232); neither end draws itself. Never re-laid out,
 * re-encoded or faded (principle 3, decision 205). A code with no partner
 * on a frame of its move is drawn only by the screen that is the picture
 * then (decision 203).
 */
@Composable
fun Modifier.sharedCode(key: String, during: Move, track: GroundTrack, bitmap: ImageBitmap, white: String): Modifier {
    val stage = LocalStage.current ?: return this
    if (motion.still) return this
    track.bind(stage, during)
    return this.then(CodeElement(stage, track, key, bitmap, white))
}

private class CodeElement(val stage: Stage, val track: GroundTrack, val key: String, val bitmap: ImageBitmap, val white: String) :
    ModifierNodeElement<CodeNode>() {
    override fun create() = CodeNode(stage.geometry, newEnd(), stage, track)

    private fun newEnd() = End(key, stage.screen, track.during).also {
        it.bitmap = bitmap
        it.white = white
    }

    override fun update(node: CodeNode) {
        node.rebind(stage.geometry, key, stage.screen, track.during)
        node.end.bitmap = bitmap
        node.end.white = white
        node.stage = stage
        node.track = track
        node.invalidateDraw()
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "sharedCode"
        properties["key"] = key
        properties["during"] = track.during
        properties["white"] = white
    }

    override fun equals(other: Any?): Boolean = other is CodeElement && other.stage === stage && other.track === track && other.key == key &&
        other.bitmap === bitmap && other.white == white

    override fun hashCode(): Int = key.hashCode() * 31 + System.identityHashCode(track)
}

private class CodeNode(geometry: StageGeometry, end: End, var stage: Stage, var track: GroundTrack) : EndNode(geometry, end), DrawModifierNode {
    override fun ContentDrawScope.draw() {
        val stage = stage
        val track = track
        when {
            !stage.isMoving(track.during) -> drawContent()
            // Moving: the stage draws the code over everything, on its white's line (decision 232).
            track.locate() -> Unit
            // No partner but its white is moving (paid on the enlarged code): the stage fades it inside that white.
            whiteMoving(track.during) -> Unit
            stage.shown() >= Nav.HALF_WAY -> drawContent()
        }
    }

    private fun whiteMoving(during: Move): Boolean {
        val white = geometry.find(end.white ?: return false) ?: return false
        return geometry.locate(white, during)
    }
}

/**
 * This node is [key]'s surface on whichever screen shows it: while the
 * screens change by [during], its bounds move from one screen's node to
 * the other's on [Motion.track], in step with the stage's progress, and
 * its content, scaled as a layer, crossfades. Only for a glyph with no
 * words in it (a badge into a hero): a glyph drawn through a changing
 * scale is rasterised again at every scale (decision 185), so words ride a
 * [sharedGround] instead; the code is a [sharedCode]. Any other move, or
 * Still, leaves it alone, so a sale that came up by itself doesn't grow
 * out of a row the cashier didn't tap. A surface with a higher [layer]
 * draws over one with a lower while both move.
 */
@Composable
fun Modifier.sharedSurface(key: String, during: Move, clip: Shape = RectangleShape, layer: Float = 0f): Modifier {
    val stage = LocalStage.current ?: return this
    val m = motion
    if (m.still) return this
    val shared = stage.shared
    val config = remember(stage, during) {
        object : SharedContentConfig {
            override val SharedContentState.isEnabled: Boolean get() = stage.moving() == during
        }
    }
    val state = shared.rememberSharedContentState(key, config)
    val bounds = remember(m) { BoundsTransform { _, _ -> m.track.spec(Rect.VisibilityThreshold) } }
    val overlayClip = remember(shared, clip) { shared.OverlayClip(clip) }
    val surface = with(shared) {
        sharedBounds(
            // The face going stays drawn, opaque, for the whole move (an exit that holds alpha at 1, so no layer of its
            // own), and the crossfade below shows it fading ([keptOpaque]).
            state, stage.content, enter = EnterTransition.None, exit = keptOpaque(m.track.fraction),
            boundsTransform = bounds, resizeMode = ScaledToWidth, zIndexInOverlay = layer, clipInOverlayDuringTransition = overlayClip,
        )
    }
    // The crossfade inside the bounds, per draw (ModulateAlpha): an enter or exit fade would composite each face through
    // an offscreen buffer of its own, a full-screen texture a frame for a sale, which the GPU's cache then keeps.
    val share = m.crossfadeOnTrack
    return surface.graphicsLayer {
        alpha = stage.faceDuring(during, share)
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }
}

/**
 * One logcat line per frame of a running change (tag [StageTraceTag]),
 * switched on with `adb shell setprop log.tag.TillStage DEBUG` and
 * read as each change starts: the move, its two screens, the stage's
 * progress and whether a back gesture is seeking it, the words' shares
 * out and in at that progress, and every moving surface's drawn bounds
 * with the progress it was drawn at (the last frame drawn, as the stage
 * left it). So a desynchronised part, or a dropped frame in framestats,
 * can be matched to what the stage drew (N-R4-2). Debug builds only; with
 * the tag off a change costs one property read and nothing per frame.
 */
@Composable
private fun StageTrace(state: SeekableTransitionState<Screen>, transition: Transition<Screen>, held: HeldMove, geometry: StageGeometry, scrub: BackScrub) {
    val m = motion
    LaunchedEffect(transition) {
        snapshotFlow { transition.currentState != transition.targetState }.collect { changing ->
            if (!changing || !Log.isLoggable(StageTraceTag, Log.DEBUG)) return@collect
            var frame = 0
            while (transition.currentState != transition.targetState) {
                withFrameNanos { nanos ->
                    val from = transition.currentState
                    val to = transition.targetState
                    val p = state.fraction
                    val line = StringBuilder()
                    line.append("f").append(frame++).append(" t=").append(nanos / 1_000_000).append(" ").append(held.during(from, to))
                        .append(" ").append(from).append(" -> ").append(to).append(" p=").append("%.3f".format(p))
                        .append(" seek=").append(scrub.active)
                        .append(" out=").append("%.2f".format(Nav.through(p, false, m.fadeOutOnTrack, m.crossfadeOnTrack)))
                        .append(" in=").append("%.2f".format(Nav.through(p, true, m.fadeOutOnTrack, m.crossfadeOnTrack)))
                    for (i in 0 until geometry.size()) {
                        val c = geometry.at(i)
                        if (c.drawnIn != held.generation) continue
                        line.append(" | ").append(c.key).append(" @").append("%.3f".format(c.p)).append(" [")
                            .append(c.left.roundToInt()).append(',').append(c.top.roundToInt()).append(',')
                            .append(c.right.roundToInt()).append(',').append(c.bottom.roundToInt()).append(']')
                    }
                    Log.d(StageTraceTag, line.toString())
                }
            }
        }
    }
}

/** The stage trace's logcat tag. */
const val StageTraceTag = "TillStage"

/** Laid out once at its own size and scaled as one layer while the bounds move: nothing inside is measured per frame. */
private val ScaledToWidth = ResizeMode.scaleToBounds(ContentScale.FillWidth, Alignment.TopCenter)

/**
 * A payment code on a screen whose [ground] is moving by [during] is drawn
 * whole or not at all: only on the frames whose ground holds all of it, so
 * a moving edge never cuts a code (decision 234). Its white goes with it:
 * the code is never drawn without the ground it needs to scan. Leaving with
 * no ground to go with (its row not laid out on the screen arriving), its
 * card fades out on the stage's own progress, gone as the arriving screen's
 * words begin (the move's last [Motion.crossfadeOnTrack]): never left whole
 * over the arriving screen until the change lands and cut, and never a
 * frame of bare ground between the two screens (decision 296). A ground
 * that would let go of it before those words begin (shrinking into a row
 * low on the screen) carries it instead, whole and inside it, for as long
 * as it fits (decision 299), fading out as the card does with no ground.
 * Leaving on a ground that holds it, it dims with the screen's words, gone
 * by the same [Motion.fadeOutOnTrack], so a Back (and a back gesture's
 * scrub) never leaves a bright code over a page that has faded round it
 * (decision 313).
 */
@Composable
fun Modifier.wholeOn(ground: GroundTrack, during: Move): Modifier {
    val stage = LocalStage.current ?: return this
    val m = motion
    if (m.still) return this
    return this.then(WholeElement(stage, ground, during, 1f - m.crossfadeOnTrack, m.fadeOutOnTrack))
}

private class WholeElement(val stage: Stage, val ground: GroundTrack, val during: Move, val leaving: Float, val dims: Float) :
    ModifierNodeElement<WholeNode>() {
    override fun create() = WholeNode(stage, ground, during, leaving, dims)

    override fun update(node: WholeNode) {
        node.stage = stage
        node.ground = ground
        node.during = during
        node.leaving = leaving
        node.dims = dims
        node.invalidateDraw()
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "wholeOn"
        properties["ground"] = ground.key
        properties["during"] = during
    }

    override fun equals(other: Any?): Boolean =
        other is WholeElement && other.stage === stage && other.ground === ground && other.during == during && other.leaving == leaving && other.dims == dims

    override fun hashCode(): Int = System.identityHashCode(ground) * 31 + during.hashCode()
}

/**
 * [Modifier.wholeOn]'s node: drawn or not per frame by where its ground is, and placed with a layer only while its
 * move is under way (the stage's states are read in placement, so it places again as a move starts and lands, nothing
 * per frame), whose alpha, read per frame, fades a code with no ground to go with, and whose translation keeps a
 * code inside a ground that would let go of it early.
 */
private class WholeNode(var stage: Stage, var ground: GroundTrack, var during: Move, var leaving: Float, var dims: Float) :
    Modifier.Node(), LayoutModifierNode, LayoutAwareModifierNode, DrawModifierNode {
    private var placed: LayoutCoordinates? = null

    /** Where [carry] last put the code: how far it is moved inside its ground on this frame, in whole pixels. */
    private var dx = 0f
    private var dy = 0f

    private val layer: GraphicsLayerScope.() -> Unit = {
        val located = ground.locate()
        val carried = located && carry()
        // Arriving it is whole from its first frame (§10 rule 6); leaving on its ground it dims as the words do, except
        // carried, where it is all there is until the arriving words begin, so it fades out to them (299).
        alpha = when {
            stage.arriving() -> 1f
            located && !carried -> stage.throughDuring(during, leaving = dims, arriving = 1f)
            else -> stage.throughDuring(during, leaving = leaving, arriving = 1f)
        }
        if (carried) {
            translationX = dx
            translationY = dy
        } else {
            translationX = 0f
            translationY = 0f
        }
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }

    /**
     * Leaving, on a ground (located this frame) that lets go of the card before the arriving screen's words begin
     * (shrinking into a row low on the screen, its edge passes the card early; the designer's back5 and back6): the
     * card carries its code, kept whole inside it by the least move, in whole pixels so its modules stay sharp, for as
     * long as it fits; with no words on screen yet, cutting it there left frames of bare ground (decision 299). Its
     * course is straight lines, so whether it lets go early is known exactly from the move's start and is the same on
     * every frame of it. False, and nothing moved, otherwise.
     */
    private fun carry(): Boolean {
        dx = 0f
        dy = 0f
        val mine = placed ?: return false
        if (stage.arriving()) return false
        val at = stage.inOverlay(mine)
        val r = at.x + mine.size.width
        val b = at.y + mine.size.height
        val c = ground.located
        val held = c.holding(at.x, at.y, r, b)
        if (Course.heldFrom(held) <= Course.heldUntil(held) && Course.heldUntil(held) >= leaving) return false
        dx = into(at.x, r, c.left, c.right)
        dy = into(at.y, b, c.top, c.bottom)
        return true
    }

    /** The least whole-pixel move putting [lo]..[hi] inside [from]..[to]: 0 if it is already in. */
    private fun into(lo: Float, hi: Float, from: Float, to: Float): Float = when {
        lo < from -> ceil(from - lo)
        hi > to -> floor(to - hi)
        else -> 0f
    }

    override fun onPlaced(coordinates: LayoutCoordinates) {
        placed = coordinates
    }

    override fun MeasureScope.measure(measurable: Measurable, constraints: Constraints): MeasureResult {
        val placeable = measurable.measure(constraints)
        return layout(placeable.width, placeable.height) {
            if (stage.isMoving(during)) placeable.placeWithLayer(0, 0, layerBlock = layer) else placeable.place(0, 0)
        }
    }

    override fun ContentDrawScope.draw() {
        val mine = placed
        if (mine == null || !stage.isMoving(during) || !ground.locate()) return drawContent()
        val at = stage.inOverlay(mine)
        val x = if (carry()) dx else 0f
        val y = dy
        if (ground.located.holds(at.x + x, at.y + y, at.x + x + size.width, at.y + y + size.height)) drawContent()
    }
}

/** Where the surfaces' parts draw while they move: grounds under what's on them, the code over everything. */
object Layers {
    const val GROUND = 0f

    /** A paid or ended sale's hero, over the code card's white closing into it. */
    const val HERO = 0.5f
    const val OVER_GROUND = 1f
    const val CODE = 2f
}

/**
 * A leaving shared surface's exit: a fade that changes nothing (alpha 1 to
 * 1, so no layer of its own), there to tie the leaving side's life in the
 * overlay to an animation of the transition rather than to the screen's
 * `KeepUntilTransitionsFinished` alone; the crossfade is the surface's own
 * per-draw alpha. On Compose animation 1.12.1 under this stage,
 * `ExitTransition.None` changed no frame RunningMovesTest or
 * MotionFramesTest reads (decision 218);
 * `MotionFramesTest.theLeavingRowsGroundIsStillDrawnAtThreeQuartersOfItsGrow`
 * fails if a leaving ground ever stops being drawn mid-grow, whichever way
 * it is kept.
 */
private fun keptOpaque(spec: FiniteAnimationSpec<Float>): ExitTransition = fadeOut(spec, targetAlpha = 1f)

/** Where a node was last placed, for a draw that needs it. */
private class Placed {
    var coordinates: LayoutCoordinates? = null
}

/**
 * A screen's own words and parts (everything but its grounds, its code and
 * its badge, which are surfaces of their own) while the screens change:
 * faded through at their own size ([Stage.throughDuring]) while the move
 * is [over] or one of [through]; drawn over the moving grounds (they draw
 * over both screens) while it is [over] (clipped to [on], the ground they
 * sit on, where it is on that frame) or, arriving, one of [lift] (words
 * arriving where a ground still moves, so its edge never cuts them:
 * decision 204; leaving, they stay under the ground growing over them).
 * Words on a moving ground [on] show only while it holds them: arriving,
 * their fade starts once it covers them; leaving, it ends before it
 * uncovers them; and arriving words wait for [clear]'s shrinking ground to
 * have passed them (decision 233). With [fades] false they are lifted and
 * clipped but not faded: a container whose words fade one by one around a
 * code that never fades (decision 205). Alpha is applied per draw, through
 * a layer made only while one of their moves is under way, never an
 * offscreen buffer and nothing at rest. [through] and [lift] are compared
 * on every recomposition, so a call site passes top-level sets, never a
 * `setOf` made in place.
 */
@Composable
fun Modifier.words(
    over: Move? = null,
    through: Set<Move> = emptySet(),
    lift: Set<Move> = emptySet(),
    on: GroundTrack? = null,
    fades: Boolean = true,
    clear: GroundTrack? = null,
): Modifier {
    val stage = LocalStage.current ?: return this
    val m = motion
    if (m.still) return this
    val lifted = if (over == null && lift.isEmpty()) this else with(stage.shared) {
        // Lifted for as long as the stage's own move runs, which the shared bounds' matches (isTransitionActive) don't
        // span: the stage draws the grounds over both screens from the change's first frame to its last (decision 232).
        renderInSharedTransitionScopeOverlay(Layers.OVER_GROUND) {
            val move = stage.moving()
            (move != null && move == over && (on == null || on.locate())) || (move in lift && stage.arriving())
        }
    }
    val clipped = if (over == null || on == null) lifted else {
        val placed = remember { Placed() }
        // Clipped to the ground they sit on, where it is on this frame: nothing shows outside it, over the other screen.
        lifted.onPlaced { placed.coordinates = it }.drawWithContent {
            val mine = placed.coordinates
            if (mine == null || !stage.isMoving(over) || !on.locate()) {
                drawContent()
            } else {
                val at = stage.inOverlay(mine)
                clipRect(on.left - at.x, on.top - at.y, on.right - at.x, on.bottom - at.y) { this@drawWithContent.drawContent() }
            }
        }
    }
    if (!fades || (over == null && through.isEmpty())) return clipped
    return clipped.then(FadeThroughElement(stage, over, through, on, clear, m.fadeOutOnTrack, m.crossfadeOnTrack))
}

private class FadeThroughElement(
    val stage: Stage,
    val over: Move?,
    val through: Set<Move>,
    val on: GroundTrack?,
    val clear: GroundTrack?,
    val leaving: Float,
    val arriving: Float,
) : ModifierNodeElement<FadeThroughNode>() {
    override fun create() = FadeThroughNode(stage, over, through, on, clear, leaving, arriving)

    override fun update(node: FadeThroughNode) {
        node.stage = stage
        node.over = over
        node.through = through
        node.on = on
        node.clear = clear
        node.leaving = leaving
        node.arriving = arriving
    }

    override fun InspectorInfo.inspectableProperties() {
        name = "words"
        properties["over"] = over
        properties["through"] = through
        properties["on"] = on?.key
        properties["clear"] = clear?.key
    }

    override fun equals(other: Any?): Boolean = other is FadeThroughElement && other.stage === stage && other.over == over &&
        other.through == through && other.on === on && other.clear === clear && other.leaving == leaving && other.arriving == arriving

    override fun hashCode(): Int = ((stage.hashCode() * 31 + over.hashCode()) * 31 + through.hashCode()) * 31 + on.hashCode()
}

/**
 * [Modifier.words]' fade: placed with a layer only while one of its moves
 * is under way (the stage's states are read in placement, so this places
 * again as a move starts and lands, and nothing per frame), its alpha read
 * in that layer's block per frame. At rest a shift's rows carry no layer
 * each (R3-O5).
 */
private class FadeThroughNode(
    var stage: Stage,
    var over: Move?,
    var through: Set<Move>,
    var on: GroundTrack?,
    var clear: GroundTrack?,
    var leaving: Float,
    var arriving: Float,
) : Modifier.Node(), LayoutModifierNode, LayoutAwareModifierNode {
    private var placed: LayoutCoordinates? = null

    override fun onPlaced(coordinates: LayoutCoordinates) {
        placed = coordinates
    }

    private val layer: GraphicsLayerScope.() -> Unit = {
        alpha = alpha()
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }

    /** One of its moves. Every screen's words fade through a move that is theirs, a row's the cashier didn't tap too (221). */
    private fun fades(move: Move?): Boolean = move != null && (move == over || move in through)

    private fun alpha(): Float {
        val move = stage.moving()
        if (!fades(move)) return 1f
        val arrivingHere = stage.arriving()
        // The plain shares of the move: out over its first [leaving], in over its last [arriving] (decision 188).
        var start = 1f - arriving
        var end = leaving
        val mine = placed
        if (mine != null && mine.isAttached) {
            val at = stage.inOverlay(mine)
            val r = at.x + mine.size.width
            val b = at.y + mine.size.height
            val ground = on
            // On a moving ground: arriving, they start once it holds them; leaving, they are gone before it lets go.
            if (ground != null && ground.during == move && ground.locate()) {
                val held = ground.located.holding(at.x, at.y, r, b)
                if (arrivingHere) start = maxOf(start, Course.heldFrom(held)) else end = min(end, Course.heldUntil(held))
            }
            // Arriving where [clear]'s ground still is, they wait until it has passed: no edge sweeps through them.
            val c = clear
            if (arrivingHere && c != null && c.locate()) start = maxOf(start, c.located.clearOf(at.x, at.y, r, b))
        }
        val p = stage.toward()
        return if (arrivingHere) {
            if (start >= 1f) 0f else ((p - start) / (1f - start)).coerceIn(0f, 1f)
        } else {
            if (end <= 0f) 0f else ((end - p) / end).coerceIn(0f, 1f)
        }
    }

    override fun MeasureScope.measure(measurable: Measurable, constraints: Constraints): MeasureResult {
        val placeable = measurable.measure(constraints)
        return layout(placeable.width, placeable.height) {
            if (fades(stage.moving())) placeable.placeWithLayer(0, 0, layerBlock = layer) else placeable.place(0, 0)
        }
    }
}

/**
 * A screen's [color] ground, except while the screens change by [unless]:
 * the order page over the sale, whose ground is the same colour and stays
 * under it, so the page's words can fade in over the sale's going without
 * a whole page fading (decision 185).
 */
@Composable
fun Modifier.groundExcept(unless: Move, color: Color): Modifier {
    val stage = LocalStage.current ?: return drawBehind { drawRect(color) }
    return drawBehind { if (!stage.isMoving(unless)) drawRect(color) }
}

/**
 * [value] as [screen] showed it while it was the model's ([current]): from
 * the moment the model moves on, a screen being left keeps the picture it
 * had as its change began until the move lands and it goes, whatever the
 * model does to what it shows meanwhile (decision 242). A sale closed from
 * the sheet, expired or refunded as its screen is left still leaves with
 * its code on its card; the stage carries away what was on screen, never
 * the model's next state of it. Written in composition while the screen is
 * the model's, as `rememberUpdatedState` is, and left alone once it is not:
 * held, it reads [value] no more, so what the model does to the sale
 * meanwhile recomposes nothing of the leaving screen.
 */
@Composable
fun <T> keptWhileLeaving(screen: Screen, current: State<Screen>, value: () -> T): State<T> {
    val kept = remember(screen) { mutableStateOf(value()) }
    if (current.value == screen) kept.value = value()
    return kept
}

/** A sale's face (its live code, or its page) for the surfaces that morph between the two (M10). */
val LocalFace = staticCompositionLocalOf<AnimatedVisibilityScope?> { null }

/**
 * [key]'s surface between a sale's two faces ([LocalFace]): while one
 * turns into the other in place, its bounds move between the two faces'
 * nodes on [Motion.surface], its content (a code or a glyph, no words)
 * scaled as a layer and crossfading per draw (the going face out under
 * [Motion.fadeQuick], the coming one in with [Motion.fade]), each face
 * clipped in its own coordinates to a rounded rectangle whose corners go
 * from [other] (its partner's, as a share of the side: a circle is 0.5)
 * to [corner] on the same spring: the code card closing into the hero's
 * circle. The clip is the content's own, so it is where the content is on
 * every frame (decision 196). The [code]'s face (the live sale's card) is
 * the one white surface of the paid fold (decision 225): leaving, its
 * white stays whole under its code, which is gone by the fold's first
 * [Motion.fadeOutOnTrack] (decision 290), until the hero, drawn over it,
 * has come in over the last [Motion.crossfadeOnTrack]; arriving ("Show the code again") it is opaque from its
 * first frame, the hero fading off it (decision 205). A face whose lower
 * corners are [bottom] at rest (a footed code card's square ones, joined
 * to the foot under it) has those go from [bottom] to [other] instead, so
 * the code and its foot stay joined as the fold begins and as it lands.
 */
@Composable
fun Modifier.faceSurface(key: String, corner: Dp, other: Float, code: Boolean = false, bottom: Dp = corner): Modifier {
    val stage = LocalStage.current ?: return this
    val face = LocalFace.current ?: return this
    val m = motion
    if (m.still) return this
    val shared = stage.shared
    val state = shared.rememberSharedContentState(key)
    val bounds = remember(m) { BoundsTransform { _, _ -> m.surface.spec(Rect.VisibilityThreshold) } }
    val shown = face.transition.animateFloat({ m.surface.fraction }, label = "face") { if (it == EnterExitState.Visible) 1f else 0f }
    val seen = face.transition.animateFloat(
        { if (targetState == EnterExitState.Visible) m.fade.fraction else m.fadeQuick.fraction }, label = "face seen",
    ) { if (it == EnterExitState.Visible || (code && face.transition.targetState == EnterExitState.Visible)) 1f else 0f }
    val clip = remember { RoundClip() }
    val share = m.crossfadeOnTrack
    val leave = m.fadeOutOnTrack
    val layer = if (code) Layers.GROUND else Layers.HERO
    return with(shared) {
        // Over every clip for as long as its face is changing: a page's hero sits in a scroll, whose clip would cut
        // the circle on the frames the shared bounds hand drawing back before the change has landed.
        renderInSharedTransitionScopeOverlay(layer) { face.transition.isRunning }.sharedBounds(
            // The going face stays drawn for the whole change (an exit holding alpha at 1, so no layer of its own);
            // the crossfade is the per-draw alpha below ([keptOpaque]).
            state, face, enter = EnterTransition.None, exit = keptOpaque(m.surface.fraction),
            boundsTransform = bounds, resizeMode = ScaledToFit, zIndexInOverlay = layer,
        )
    }.drawBehind {
        // The code's white, under the fading code and over nothing else, for as long as the fold runs: the hero comes in
        // over a white surface closing into it, never over the screen's ground.
        if (code && face.transition.isRunning) {
            val side = size.minDimension
            val r = lerp(other * side, corner.toPx(), shown.value).coerceAtMost(side / 2f)
            val rb = lerp(other * side, bottom.toPx(), shown.value).coerceAtMost(side / 2f)
            drawPath(clip.around(size, r, rb), Fixed.White)
        }
    }.graphicsLayer {
        // The glyph coming in (the paid page's tick) only over the fold's last share, as the code screen's hero comes in
        // over its move's (decision 252): one glyph in the card at a time, never the tick over the fading code while the
        // card is still near its size (260 b). Read per draw from the fold's own progress; no state of its own.
        // The code going (the live card folding into the hero) on the same progress, gone by its first share, not on
        // a fade of its own that a late start or a slow frame leaves behind the card (260 b, decision 290).
        val running = face.transition.isRunning
        val arriving = face.transition.targetState == EnterExitState.Visible
        alpha = when {
            running && arriving && !code -> min(seen.value, Nav.through(shown.value, isArriving = true, leaving = 1f, arriving = share))
            running && !arriving && code -> Nav.through(1f - shown.value, isArriving = false, leaving = leave, arriving = share)
            else -> seen.value
        }
        compositingStrategy = CompositingStrategy.ModulateAlpha
    }.drawWithContent {
        if (!face.transition.isRunning) {
            drawContent()
        } else {
            val side = size.minDimension
            val r = lerp(other * side, corner.toPx(), shown.value).coerceAtMost(side / 2f)
            val rb = lerp(other * side, bottom.toPx(), shown.value).coerceAtMost(side / 2f)
            clipPath(clip.around(size, r, rb)) { this@drawWithContent.drawContent() }
        }
    }
}

/** Scaled as one layer to fit the bounds, centred: a square card into a circle. */
private val ScaledToFit = ResizeMode.scaleToBounds(ContentScale.Fit, Alignment.Center)

/** A rounded rectangle the size of what it clips, one path rewound each frame, so a frame allocates nothing. */
private class RoundClip {
    private val path = Path()
    private val radii = FloatArray(8)

    /** [r] at the top corners and [bottom] at the lower ones. */
    fun around(size: Size, r: Float, bottom: Float = r): Path {
        path.rewind()
        if (bottom == r) {
            path.asAndroidPath().addRoundRect(0f, 0f, size.width, size.height, r, r, android.graphics.Path.Direction.CW)
        } else {
            radii.fill(r, 0, 4)
            radii.fill(bottom, 4, 8)
            path.asAndroidPath().addRoundRect(0f, 0f, size.width, size.height, radii, android.graphics.Path.Direction.CW)
        }
        return path
    }
}

/**
 * Whether the screen this is on is the one up: true at once with no stage,
 * else once its arrival has landed, and through a back gesture that may yet
 * spring back to it; false from the moment it is being left for good (the
 * gesture committed, or the till moved on). Read where it's needed, so only
 * that recomposes, at those moments.
 */
@Composable
fun up(): Boolean {
    val stage = LocalStage.current ?: return true
    val transition = stage.content.transition
    return remember(transition) {
        derivedStateOf {
            transition.currentState == EnterExitState.Visible && (transition.targetState == EnterExitState.Visible || stage.scrubbing)
        }
    }.value
}
