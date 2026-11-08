package com.monokulo.till

import uniffi.till_app.Screen
import android.graphics.Bitmap
import android.os.Environment
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import com.monokulo.till.ui.mk
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.activity.ComponentActivity
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.core.view.WindowCompat
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.HeroTag
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.LocalScreenReader
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.rememberShiftPlace
import java.io.File
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/**
 * The till's screens under the one driver ([ModelStage]: [FollowScreen],
 * the back handler and [TillStage]) on the test clock, a screen change run
 * frame by frame as the device runs it, every frame captured. A seek puts
 * the stage, the shared match and the overlay in one consistent state; only
 * a running move can desynchronise them, so this is what sees a frame where
 * they disagree (decisions 203 to 206).
 */
class RunningMoves(private val compose: AndroidComposeTestRule<*, ComponentActivity>, val sale: TillSale) {
    val sales = listOf(testSale("z0", expiresAt = 1_600), sale, testSale("b2", expiresAt = 1_600))
    val now = mutableLongStateOf(1_000)
    var shown by mutableStateOf(sale)

    /** Whether a screen reader explores by touch, as the shift asks it when a Back lands. */
    var screenReader = false

    /** The shift's sales, newest first, as the shift screen lists them. */
    var shift by mutableStateOf(listOf(sales[0], sale, sales[2]))

    /** Each screen's stage as it last composed: what it says is moving, read on every frame. */
    private val stages = mutableMapOf<Screen, Stage>()

    /** Every ground's track by its key, both sides: where the ground is drawn on a frame of its move. */
    private val tracks = mutableMapOf<String, MutableList<GroundTrack>>()
    lateinit var model: TestModel
        private set

    /** The till on [from], in the [dark] or light theme, with [motion]. */
    fun start(from: Screen, dark: Boolean, motion: Motion = Motion.Full) {
        model = TestModel(from)
        // The bars' icons as the app's edge to edge leaves them for the theme: light icons over the dark one.
        compose.runOnUiThread {
            val view = compose.activity.findViewById<ViewGroup>(android.R.id.content)
            WindowCompat.getInsetsController(compose.activity.window, view).isAppearanceLightStatusBars = !dark
        }
        compose.setContent {
            Till(motion, dark) {
                CompositionLocalProvider(
                    LocalGroundTracks provides { key -> GroundTrack(key).also { tracks.getOrPut(key) { mutableListOf() } += it } },
                    LocalScreenReader provides { screenReader },
                ) {
                // The activity's window is the theme's ground (decision 181); a test's is white.
                Box(Modifier.fillMaxSize().background(mk.ground)) {
                // As the activity's: kept over the stage, where the shift's list was when a row was tapped.
                val place = rememberShiftPlace()
                ModelStage(model) { s ->
                    val stage = LocalStage.current
                    if (stage != null) SideEffect { stages[s] = stage }
                    // As the app's screens do (MainActivity.TillScreens): a screen being left keeps the sale it showed.
                    val sale = keptWhileLeaving(s, model.screen) { shown }.value
                    Box(Modifier.fillMaxSize().testTag(tag(s))) {
                    when (s) {
                        is Screen.Code -> CodeScreen(rememberFace(sale), onBack = {})
                        is Screen.Order -> OrderPage(rememberFace(sale), "Back to the sale", onBack = {}, onShowCode = {})
                        is Screen.Sale -> SaleScreen(rememberFace(sale, now.longValue), noActions)
                        else -> EngineShift(shift, now.longValue, noActions, place)
                    }
                    }
                }
                }
                }
            }
        }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        repeat(2) { compose.mainClock.advanceTimeByFrame() }
    }

    /**
     * One frame of a move: the picture, what each side's stage said was under way on it, the arriving screen's codes
     * and words where they are laid out, where the moving ground is drawn (null: it has no shape this frame), and the
     * paid page's hero and heading (the one spoken as it changes: no live sale has it), how far each screen is shown,
     * and whether the status bar's icons are dark.
     */
    class Frame(
        val index: Int,
        val bitmap: Bitmap,
        val moves: List<Move?>,
        val codes: List<Rect>,
        val texts: List<Pair<String, Rect>>,
        val ground: Rect?,
        val hero: Rect?,
        val heading: Rect?,
        val shown: Map<Screen, Float>,
        val lightBars: Boolean,
        /** Where the moving code is drawn on this frame (its track), null when it is not moving. */
        val code: Rect?,
        /** Every screen's words on this frame where they are laid out, by screen. */
        val words: Map<Screen, List<Pair<String, Rect>>>,
        /** The codes of the screens other than the arriving one, where they are laid out. */
        val leavingCodes: List<Rect>,
        /** The stage's progress toward its target. */
        val p: Float,
        /** Where the moving ground is going on this frame (its course's arriving end, as laid out), null when it is not moving. */
        val groundEnd: Rect? = null,
    )

    /** The model sent to [to] (as the till does, or the cashier), and every frame until the stage has landed and settled. */
    fun run(to: Screen, byItself: Boolean = false, groundKey: String? = null, during: (frame: Int) -> Unit = {}, change: () -> Unit = {}): List<Frame> {
        val frames = mutableListOf(capture(0, to, groundKey))
        // What the model does to the sale in the same update as it moves the screen (closed from the sheet, paid on the code).
        change()
        model.go(to, byItself)
        var landed = 0
        var i = 1
        while (i < MAX_FRAMES && landed < SETTLE_FRAMES) {
            // What happens to the till while the move runs (a sale arriving mid-Back), before the frame [i] draws.
            during(i)
            compose.mainClock.advanceTimeByFrame()
            frames += capture(i++, to, groundKey)
            if (model.stage.currentState == to && model.stage.targetState == to) landed++
        }
        return frames
    }

    /** Every frame until the clock has moved [ms] on, with nothing sent: a change the screen makes in itself (the paid fold). */
    fun runFor(ms: Long, change: () -> Unit): List<Frame> {
        val on = model.screen.value
        val frames = mutableListOf(capture(0, on, null))
        change()
        val until = compose.mainClock.currentTime + ms
        var i = 1
        while (compose.mainClock.currentTime < until) {
            compose.mainClock.advanceTimeByFrame()
            frames += capture(i++, on, null)
        }
        return frames
    }

    private fun capture(index: Int, arriving: Screen, groundKey: String?): Frame {
        // Drawn now, on the main thread, from the state the frame left (a copy of the window's last buffer may be a
        // frame behind what the stage says, and on another run not), at half size: thirty frames of a full-size
        // window would not fit the test's heap, and a module is still 8 px.
        val view = compose.activity.findViewById<ViewGroup>(android.R.id.content).getChildAt(0)
        val bitmap = Bitmap.createBitmap(view.width / SCALE, view.height / SCALE, Bitmap.Config.ARGB_8888)
        var lightBars = false
        compose.runOnUiThread {
            lightBars = WindowCompat.getInsetsController(compose.activity.window, view).isAppearanceLightStatusBars
            val canvas = Canvas(bitmap)
            canvas.scale(1f / SCALE, 1f / SCALE)
            view.draw(canvas)
        }
        val moves = stages.values.map { it.moving() }
        val mine = hasAnyAncestor(hasTestTag(tag(arriving)))
        fun drawnOf(matcher: SemanticsMatcher) = compose.onAllNodes(matcher, useUnmergedTree = true).fetchSemanticsNodes().map { it.drawn().half() }
        val codes = drawnOf(mine and SemanticsMatcher("a payment code") { n -> n.config.getOrNull(SemanticsProperties.ContentDescription)?.any { it.startsWith("Payment code") } == true })
        val texts = compose.onAllNodes(mine and SemanticsMatcher.keyIsDefined(SemanticsProperties.Text), useUnmergedTree = true).fetchSemanticsNodes()
            .map { (it.config.getOrNull(SemanticsProperties.Text)?.joinToString().orEmpty()) to it.drawn().half() }
        val ground = groundKey?.let { k -> tracks[k]?.firstNotNullOfOrNull { it.drawn() } }?.half()
        val groundEnd = groundKey?.let { k -> tracks[k]?.firstNotNullOfOrNull { it.end() } }?.half()
        val code = tracks["code/${sale.orderId}"]?.firstNotNullOfOrNull { it.drawn() }?.half()
        val words = stages.keys.associateWith { s ->
            compose.onAllNodes(hasAnyAncestor(hasTestTag(tag(s))) and SemanticsMatcher.keyIsDefined(SemanticsProperties.Text), useUnmergedTree = true).fetchSemanticsNodes()
                .map { (it.config.getOrNull(SemanticsProperties.Text)?.joinToString().orEmpty()) to it.drawn().half() }
        }
        val payment = SemanticsMatcher("a payment code") { n -> n.config.getOrNull(SemanticsProperties.ContentDescription)?.any { it.startsWith("Payment code") } == true }
        val leavingCodes = stages.keys.filter { it != arriving }.flatMap { s -> drawnOf(hasAnyAncestor(hasTestTag(tag(s))) and payment) }
        val hero = drawnOf(SemanticsMatcher.expectValue(SemanticsProperties.TestTag, HeroTag)).firstOrNull()
        val heading = drawnOf(mine and SemanticsMatcher.keyIsDefined(SemanticsProperties.Heading) and SemanticsMatcher.keyIsDefined(SemanticsProperties.LiveRegion)).firstOrNull()
        // A moving code is drawn where its track says, not where either screen laid it out.
        val drawn = if (code != null) listOf(code) else codes
        return Frame(index, bitmap, moves, drawn, texts, ground, hero, heading, stages.mapValues { it.value.shown() }, lightBars, code, words, leavingCodes, model.stage.fraction, groundEnd)
    }

    private fun tag(s: Screen) = "screen:$s"

    private fun Rect.half() = Rect(left / SCALE, top / SCALE, right / SCALE, bottom / SCALE)

    companion object {
        /** More than any move takes: 450 ms of track and the spring's tail, at 16 ms a frame. */
        const val MAX_FRAMES = 90

        /** How much smaller than the window a frame is kept. */
        const val SCALE = 2

        /** Frames kept after the stage has landed, for anything that lands a frame late. */
        const val SETTLE_FRAMES = 4

        /** Mean luminance of [bitmap], 0 to 255, every fourth pixel. */
        fun luminance(bitmap: Bitmap, region: Rect? = null): Float {
            val l = (region?.left ?: 0f).toInt().coerceIn(0, bitmap.width - 1)
            val t = (region?.top ?: 0f).toInt().coerceIn(0, bitmap.height - 1)
            val r = (region?.right ?: bitmap.width.toFloat()).toInt().coerceIn(l + 1, bitmap.width)
            val b = (region?.bottom ?: bitmap.height.toFloat()).toInt().coerceIn(t + 1, bitmap.height)
            var sum = 0.0
            var n = 0
            for (y in t until b step 4) for (x in l until r step 4) {
                sum += lum(bitmap.getPixel(x, y))
                n++
            }
            return if (n == 0) 0f else (sum / n).toFloat()
        }

        fun lum(c: Int): Float = 0.2126f * (c shr 16 and 0xff) + 0.7152f * (c shr 8 and 0xff) + 0.0722f * (c and 0xff)

        /** The darkest and lightest luminance inside [region] of [bitmap], every second pixel. */
        fun range(bitmap: Bitmap, region: Rect): Pair<Float, Float> {
            var lo = 255f
            var hi = 0f
            val l = region.left.toInt().coerceIn(0, bitmap.width)
            val t = region.top.toInt().coerceIn(0, bitmap.height)
            val r = region.right.toInt().coerceIn(0, bitmap.width)
            val b = region.bottom.toInt().coerceIn(0, bitmap.height)
            for (y in t until b step 2) for (x in l until r step 2) {
                val v = lum(bitmap.getPixel(x, y))
                lo = min(lo, v)
                hi = max(hi, v)
            }
            return lo to hi
        }

        /**
         * The flash detector (decision 203's review): over a screen change in the dark theme, no frame's mean luminance
         * is lighter than both of its ends by more than [margin] (a white frame between two dark ones), and none steps
         * from the frame before by more than the whole move's end to end difference (a jump to a third picture). A
         * frame darker than both is a card closing before the page's words come in, not a flash. The hard cuts of
         * f274ad7 fail the second rule on their one frame: that is the jump the moves replace. Empty if nothing failed.
         */
        fun flashes(frames: List<Frame>, margin: Float = FLASH_MARGIN): List<String> {
            val l = frames.map { luminance(it.bitmap) }
            val first = l.first()
            val last = l.last()
            val hi = max(first, last) + margin
            val whole = abs(last - first)
            val failures = mutableListOf<String>()
            for (i in 1 until l.size) {
                if (l[i] > hi) failures += "f${frames[i].index} at ${"%.1f".format(l[i])} is lighter than both its ends ${"%.1f".format(first)} and ${"%.1f".format(last)}"
                if (abs(l[i] - l[i - 1]) > whole + margin) failures += "f${frames[i].index} steps ${"%.1f".format(l[i] - l[i - 1])} in one frame, more than the move's whole ${"%.1f".format(whole)}"
            }
            return failures
        }

        /** How far a frame's mean luminance may wander from the line between its ends: a word fading, not a screen. */
        const val FLASH_MARGIN = 4f

        fun save(frame: Frame, name: String) {
            val dir = InstrumentationRegistry.getArguments().getString("additionalTestOutputDir")?.let(::File)
                ?: File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(Environment.DIRECTORY_PICTURES), "running")
            dir.mkdirs()
            File(dir, "$name-f${frame.index}.png").outputStream().use { frame.bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}

/** Where [GroundTrack]'s surface is drawn on this frame of its move, in the stage's overlay; null at rest or with an end missing. */
internal fun GroundTrack.drawn(): Rect? = if (locate()) Rect(left, top, right, bottom) else null

/** Where [GroundTrack]'s surface is going on this frame of its move (its arriving end), in the stage's overlay; null as [drawn]. */
internal fun GroundTrack.end(): Rect? = if (locate()) located.let { Rect(it.el, it.et, it.er, it.eb) } else null
