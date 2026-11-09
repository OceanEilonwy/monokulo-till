package com.monokulo.till

import uniffi.till_app.Screen
import android.graphics.Bitmap
import android.os.Environment
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.ui.CodeScreen
import androidx.compose.runtime.CompositionLocalProvider
import com.monokulo.till.ui.HeroSize
import com.monokulo.till.ui.HeroTag
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ShiftScreen
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import org.junit.Rule
import org.junit.Test
import java.io.File
import kotlin.math.abs
import androidx.compose.ui.util.lerp
import kotlin.math.roundToInt
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Every screen [Move] seeked to a quarter, a half and three quarters, each
 * frame written out for review (`docs/reports/motion/frames/`), and checked
 * on every one: the code whole and on its white, no words drawn through a
 * scale (decision 185); and every move's landed frame checked against the
 * same screen composed under Still (decision 187).
 */
class MotionFramesTest {
    @get:Rule
    val compose = createComposeRule()

    private val sale = testSale("a1", expiresAt = 1_600).copy(receipt = sampleReceipt())
    private val sales = listOf(testSale("z0", expiresAt = 1_600), sale, testSale("b2", expiresAt = 1_600))
    private val now = mutableLongStateOf(1_000)
    private lateinit var scope: CoroutineScope

    /** Under Still once set: the same screens composed with no motion, for a landed move to be compared with. */
    private var still by mutableStateOf(false)
    private var shown by mutableStateOf(sale)

    /** Every ground's tracks by key, in the order their screens composed them. */
    private val tracks = mutableMapOf<String, MutableList<GroundTrack>>()

    private fun stage(from: Screen, byItself: Boolean = false): SeekableTransitionState<Screen> {
        val state = SeekableTransitionState(from)
        compose.setContent {
            scope = rememberCoroutineScope()
            Till(if (still) Motion.Still else Motion.Full) {
                CompositionLocalProvider(LocalGroundTracks provides { key -> GroundTrack(key).also { tracks.getOrPut(key) { mutableListOf() } += it } }) {
                TillStage(state, cameUpByItself = { byItself }) { s ->
                    when (s) {
                        is Screen.Code -> CodeScreen(rememberFace(shown), onBack = {})
                        is Screen.Order -> OrderPage(rememberFace(shown), onBack = {}, onShowCode = {})
                        is Screen.Sale -> SaleScreen(rememberFace(shown, now.longValue), noActions)
                        else -> EngineShift(sales, now.longValue, noActions)
                    }
                }
                }
            }
        }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        return state
    }

    /**
     * [state] at [p] toward [to] as a running move has it on its frame there: a shared element is placed with the
     * bounds of the update before, so a single seek leaves it a whole step behind while a move only ever trails a
     * frame. A hair short of [p] first, then [p], so every part is where that frame of the move puts it.
     */
    private fun seekOnto(state: SeekableTransitionState<Screen>, p: Float, to: Screen) {
        scope.launch { state.seekTo(p - HAIR, to) }
        repeat(2) { compose.mainClock.advanceTimeByFrame() }
        scope.launch { state.seekTo(p, to) }
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
    }

    /**
     * [state] seeked toward [to] at each of [Points], [check] run and the frame saved as `<name>-<percent>.png`, and no
     * text on screen drawn at any but its own size on any of them.
     */
    private fun seek(
        state: SeekableTransitionState<Screen>,
        to: Screen,
        name: String,
        check: (Float, ImageBitmap) -> Unit = { _, _ -> },
    ) {
        scope.launch { state.seekTo(0f, to) }
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
        val failures = mutableListOf<String>()
        for (p in Points) {
            seekOnto(state, p, to)
            val frame = compose.onRoot().captureToImage()
            save(frame, "$name-${(p * 100).roundToInt()}")
            runCatching { check(p, frame) }.exceptionOrNull()?.let { failures += it.message.orEmpty() }
            runCatching { assertWordsAtRestSize("at $p") }.exceptionOrNull()?.let { failures += it.message.orEmpty() }
        }
        assertTrue(failures.isEmpty(), failures.joinToString("\n"))
    }

    private fun bounds(description: String): Rect =
        compose.onNodeWithContentDescription(description, substring = true, useUnmergedTree = true).fetchSemanticsNode().drawn()

    /** Every text on screen is drawn at the size it was laid out at: none through a changing scale, so no glyph is rasterised again per frame. */
    private fun assertWordsAtRestSize(at: String) {
        val scaled = compose.onAllNodes(SemanticsMatcher.keyIsDefined(SemanticsProperties.Text), useUnmergedTree = true).fetchSemanticsNodes()
            .filter { it.layoutInfo.coordinates.size.width > 0 && abs(it.drawn().width - it.layoutInfo.coordinates.size.width) > 1f }
            .map { "${it.config.getOrNull(SemanticsProperties.Text)?.joinToString()} ${it.layoutInfo.coordinates.size.width} -> ${it.drawn().width}" }
        assertTrue(scaled.isEmpty(), "$at no words are drawn scaled: $scaled")
    }

    @Test
    fun theEnlargingCodeIsOneWholeSquareCodeOnItsWhiteOnEveryFrame() {
        val state = stage(Screen.Sale("a1"))
        val rest = bounds("Payment code for")
        val cardMargin = margin(compose.onRoot().captureToImage(), rest)
        val margins = mutableListOf<Pair<Float, Int>>()
        val drawn = mutableListOf<Pair<Float, Rect>>()
        seek(state, Screen.Code("a1"), "code-grow") { p, frame ->
            val code = codeDrawn()
            assertTrue(abs(code.width - code.height) <= 1f, "at $p the code is square: $code")
            assertTrue(code.width >= rest.width - 1f, "at $p it is no smaller than at rest: ${code.width} < ${rest.width}")
            margins += p to margin(frame, code)
            drawn += p to code
        }
        seekOnto(state, 1f, Screen.Code("a1"))
        val big = bounds("Payment code, tap to shrink")
        val screenMargin = margin(compose.onRoot().captureToImage(), big)
        assertMargins(margins, minOf(cardMargin, screenMargin))
        assertOnTheLine(drawn, rest, big)
    }

    @Test
    fun theShrinkingCodeIsOneWholeSquareCodeOnItsWhiteOnEveryFrame() {
        val state = stage(Screen.Code("a1"))
        val big = bounds("Payment code, tap to shrink")
        val screenMargin = margin(compose.onRoot().captureToImage(), big)
        val margins = mutableListOf<Pair<Float, Int>>()
        val drawn = mutableListOf<Pair<Float, Rect>>()
        seek(state, Screen.Sale("a1"), "code-shrink") { p, frame ->
            val code = codeDrawn()
            assertTrue(abs(code.width - code.height) <= 1f, "at $p the code is square: $code")
            margins += p to margin(frame, code)
            drawn += p to code
        }
        seekOnto(state, 1f, Screen.Sale("a1"))
        val small = bounds("Payment code for")
        val cardMargin = margin(compose.onRoot().captureToImage(), small)
        assertMargins(margins, minOf(cardMargin, screenMargin))
        assertOnTheLine(drawn, big, small)
    }

    /** Where the moving code is drawn on this frame: its track, not where either screen laid it out. */
    private fun codeDrawn(): Rect = tracks["code/a1"]?.firstNotNullOfOrNull { it.drawn() } ?: bounds("Payment code")

    /**
     * A seek moves the code as it moves the words (decision 223): at p of the way, the code is p of the way along
     * the straight line between its two rest places, to two pixels.
     */
    private fun assertOnTheLine(drawn: List<Pair<Float, Rect>>, from: Rect, to: Rect) {
        val off = drawn.filter { (p, r) ->
            val want = Rect(lerp(from.left, to.left, p), lerp(from.top, to.top, p), lerp(from.right, to.right, p), lerp(from.bottom, to.bottom, p))
            abs(r.left - want.left) > 2f || abs(r.top - want.top) > 2f || abs(r.right - want.right) > 2f || abs(r.bottom - want.bottom) > 2f
        }
        assertTrue(off.isEmpty(), "the code is where the seek puts it, between $from and $to: $off")
    }

    /** The white round the code on every frame is at least the smaller of its two rest margins (decision 189). */
    private fun assertMargins(margins: List<Pair<Float, Int>>, least: Int) {
        assertTrue(least > 20, "the rest margins were measured: $least")
        val thin = margins.filter { it.second < least - 2 }
        assertTrue(thin.isEmpty(), "the code keeps at least its smaller rest margin ($least px) of white on every frame: $margins")
    }

    /**
     * The row's ground, on the shift being left, is the side that draws a row's grow: three quarters through, it is
     * still in the overlay with a shape (its exit holds alpha at 1; with ExitTransition.None it would be gone).
     */
    @Test
    fun theLeavingRowsGroundIsStillDrawnAtThreeQuartersOfItsGrow() {
        val state = stage(Screen.Till)
        scope.launch { state.seekTo(0f, Screen.Sale("a1")) }
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
        // Only a ground still in the overlay is told its shape as the move goes on: half way, then three quarters.
        val row = tracks["sale/a1"]?.firstOrNull()
        seekOnto(state, 0.5f, Screen.Sale("a1"))
        val half = row?.drawn()
        seekOnto(state, 0.75f, Screen.Sale("a1"))
        val threeQuarters = row?.drawn()
        assertTrue(threeQuarters != null && threeQuarters != half, "the row's ground is drawn through its clip at 75 % of its grow: $half, then $threeQuarters")
    }

    @Test
    fun aRowGrowsIntoItsSale() = seek(stage(Screen.Till), Screen.Sale("a1"), "row-grow")

    @Test
    fun aSaleShrinksIntoItsRow() = seek(stage(Screen.Sale("a1")), Screen.Till, "row-shrink")

    @Test
    fun aSaleThatCameUpByItselfRises() {
        val state = stage(Screen.Till, byItself = true)
        val rest = with(compose.density) { InterruptTest.RestCode.toPx() }
        seek(state, Screen.Sale("a1"), "rise") { p, _ ->
            val code = bounds("Payment code for")
            assertTrue(code.width >= rest - 1f, "at $p the rising sale's code is no smaller than at rest: ${code.width} < $rest")
        }
    }

    @Test
    fun theOrderCardGrowsIntoTheOrderPage() = seek(stage(Screen.Sale("a1")), Screen.Order("a1"), "order-grow")

    @Test
    fun theOrderPageShrinksIntoTheOrderCard() = seek(stage(Screen.Order("a1")), Screen.Sale("a1"), "order-shrink")

    @Test
    fun theOrderPageStartsAsTheSaleItself() {
        val state = stage(Screen.Sale("a1"))
        val sale = compose.onRoot().captureToImage().asAndroidBitmap()
        scope.launch { state.seekTo(0f, Screen.Order("a1")) }
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
        val start = compose.onRoot().captureToImage().asAndroidBitmap()
        var differ = 0
        // A pixel differs when a channel is off by more than a few levels: drawn through the stage's layer at 0, the
        // sale's own edges are antialiased a level or two differently (at most 6 of 255 on an API 37 emulator),
        // which no one sees; a word or bar of the page showing through is far more.
        fun near(a: Int, b: Int) = (0..24 step 8).all { kotlin.math.abs((a shr it and 0xFF) - (b shr it and 0xFF)) <= 8 }
        for (y in 0 until sale.height step 4) for (x in 0 until sale.width step 4) if (!near(sale.getPixel(x, y), start.getPixel(x, y))) differ++
        val total = (sale.height / 4) * (sale.width / 4)
        assertTrue(differ * 200 < total, "at 0 the page's bar, heading and foot are unseen and the card is the sale's own: $differ of $total differ")
    }

    @Test
    fun aSalePaidOnScreenClosesItsCodeIntoTheHero() {
        stage(Screen.Sale("a1"))
        val card = bounds("Payment code for")
        shown = sale.copy(phase = SalePhase.Paid, paidAt = 1_100)
        compose.mainClock.advanceTimeByFrame()
        val widths = mutableListOf<Float>()
        val failures = mutableListOf<String>()
        var at = 0L
        for (ms in listOf(60L, 120L, 180L, 240L, 300L, 360L)) {
            compose.mainClock.advanceTimeBy(ms - at)
            at = ms
            val frame = compose.onRoot().captureToImage()
            save(frame, "paid-fold-$ms")
            if (ms <= 120) widths += bounds("Payment code for").width
            runCatching { assertHeroWhole(frame, ms) }.exceptionOrNull()?.let { failures += it.message.orEmpty() }
            runCatching { assertWordsAtRestSize("at $ms ms") }.exceptionOrNull()?.let { failures += it.message.orEmpty() }
        }
        assertTrue(widths[0] < card.width && widths[1] < widths[0], "the code closes with its card toward the hero: ${card.width} -> $widths")
        assertTrue(failures.isEmpty(), failures.joinToString("\n"))
        compose.mainClock.advanceTimeBy(1_000)
        save(compose.onRoot().captureToImage(), "paid-fold-rest")
        compose.onAllNodesWithContentDescription("Payment code for", substring = true, useUnmergedTree = true).assertCountEquals(0)
        assertTrue(compose.onAllNodesWithText("Paid", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty(), "the page says Paid")
    }

    /**
     * A sale that ends while on screen (refunded from the close sheet, run out, or ended by its other till) folds the
     * same way into its ended hero on the warn ground (N4). "Close it for now" leaves for the shift instead (decision 200).
     */
    @Test
    fun aSaleEndedOnScreenClosesItsCodeIntoTheEndedHero() {
        stage(Screen.Sale("a1"))
        shown = sale.copy(phase = SalePhase.Cancelled)
        compose.mainClock.advanceTimeByFrame()
        val failures = mutableListOf<String>()
        var at = 0L
        for (ms in listOf(60L, 120L, 240L, 300L)) {
            compose.mainClock.advanceTimeBy(ms - at)
            at = ms
            val frame = compose.onRoot().captureToImage()
            save(frame, "ended-fold-$ms")
            runCatching { assertHeroWhole(frame, ms) }.exceptionOrNull()?.let { failures += it.message.orEmpty() }
            runCatching { assertWordsAtRestSize("at $ms ms") }.exceptionOrNull()?.let { failures += it.message.orEmpty() }
        }
        assertTrue(failures.isEmpty(), failures.joinToString("\n"))
        compose.mainClock.advanceTimeBy(1_000)
        save(compose.onRoot().captureToImage(), "ended-fold-rest")
        compose.onAllNodesWithContentDescription("Payment code for", substring = true, useUnmergedTree = true).assertCountEquals(0)
    }

    /**
     * Once the card has closed to about the hero's size, the hero is a whole circle: the ground just above it, and its
     * top an arc, not cut straight by the page's scroll (the review's flat top at 240 ms).
     */
    private fun assertHeroWhole(frame: ImageBitmap, ms: Long) {
        val hero = compose.onNodeWithTag(HeroTag, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        // The hero's own size: its node is scaled while the card closes into it.
        val side = with(compose.density) { HeroSize.toPx() }
        val bitmap = frame.asAndroidBitmap().copy(Bitmap.Config.ARGB_8888, false)
        val cx = hero.center.x.roundToInt()
        val ground = bitmap.getPixel(4, hero.top.roundToInt())
        // From well above the hero's resting top down the centre line: where its shape starts.
        var top = (hero.top - side).roundToInt().coerceAtLeast(2)
        while (top < hero.bottom && close(bitmap.getPixel(cx, top), ground)) top++
        if (top >= hero.bottom - side / 2) return
        var bottom = top
        while (bottom < bitmap.height && !close(bitmap.getPixel(cx, bottom), ground)) bottom++
        val height = bottom - top
        val above = bitmap.getPixel(cx, top - 2)
        assertTrue(close(above, ground), "at $ms ms the page's ground is just above the hero: ${Integer.toHexString(above)}")
        // Only once the shape is about the hero's own size is it meant to be a circle (before, it is the card closing).
        if (height > side * 1.3f || height < side * 0.7f) return
        fun chord(y: Int): Int {
            var l = cx
            var r = cx
            while (l > 0 && !close(bitmap.getPixel(l, y), ground)) l--
            while (r < bitmap.width - 1 && !close(bitmap.getPixel(r, y), ground)) r++
            return r - l
        }
        val width = chord(top + height / 2)
        if (width > side * 1.3f) return
        val nearTop = chord(top + 3)
        assertTrue(nearTop * 10 < width * 4, "at $ms ms the hero's top is an arc, not cut flat: $nearTop px across 3 px down, $width wide")
        assertTrue(abs(width - height) <= 4, "at $ms ms the hero is as wide as it is tall: $width x $height")
    }

    private fun close(a: Int, b: Int): Boolean =
        abs((a shr 16 and 0xff) - (b shr 16 and 0xff)) + abs((a shr 8 and 0xff) - (b shr 8 and 0xff)) + abs((a and 0xff) - (b and 0xff)) < 12

    @Test
    fun aRowGrowLandsOnTheSaleAsItIsUnderStill() = landsAsStill(Screen.Till, Screen.Sale("a1"))

    @Test
    fun aRowShrinkLandsOnTheShiftAsItIsUnderStill() = landsAsStill(Screen.Sale("a1"), Screen.Till)

    @Test
    fun theCodeGrowLandsOnTheCodeAsItIsUnderStill() = landsAsStill(Screen.Sale("a1"), Screen.Code("a1"))

    @Test
    fun theCodeShrinkLandsOnTheSaleAsItIsUnderStill() = landsAsStill(Screen.Code("a1"), Screen.Sale("a1"))

    @Test
    fun theOrderGrowLandsOnTheOrderPageAsItIsUnderStill() = landsAsStill(Screen.Sale("a1"), Screen.Order("a1"))

    @Test
    fun theOrderShrinkLandsOnTheSaleAsItIsUnderStill() = landsAsStill(Screen.Order("a1"), Screen.Sale("a1"))

    @Test
    fun aRiseLandsOnTheSaleAsItIsUnderStill() = landsAsStill(Screen.Till, Screen.Sale("a1"), byItself = true)

    @Test
    fun thePaidFoldLandsOnThePageAsItIsUnderStill() {
        stage(Screen.Sale("a1"))
        shown = sale.copy(phase = SalePhase.Paid, paidAt = 1_100)
        compose.mainClock.advanceTimeBy(1_500)
        assertSameAsStill("paid")
    }

    /**
     * [from] moved to [to] with motion and landed: the frame and the semantics are the screen's own, the same as [to]
     * composed under Still, so nothing a move leaves behind (a face at zero, a surface still in the overlay) survives it.
     */
    private fun landsAsStill(from: Screen, to: Screen, byItself: Boolean = false) {
        val state = stage(from, byItself)
        scope.launch { state.animateTo(to, Motion.Full.stage) }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(to, state.currentState)
        assertSameAsStill("$from to $to")
    }

    private fun assertSameAsStill(what: String) {
        val landedTree = tree()
        val landedFrame = compose.onRoot().captureToImage()
        val landed = landedFrame.asAndroidBitmap()
        still = true
        compose.mainClock.advanceTimeBy(500)
        val stillTree = tree()
        val stillFrame = compose.onRoot().captureToImage()
        val rest = stillFrame.asAndroidBitmap()
        val name = what.replace(Regex("[^A-Za-z0-9]+"), "-").trim('-').lowercase()
        save(landedFrame, "landed-$name")
        assertEquals(stillTree, landedTree, "$what: the landed screen has the semantics it has under Still")
        var differ = 0
        for (y in 0 until rest.height step 2) for (x in 0 until rest.width step 2) if (!close(rest.getPixel(x, y), landed.getPixel(x, y))) differ++
        val total = (rest.height / 2) * (rest.width / 2)
        assertTrue(differ * 1000 < total, "$what: the landed frame is the Still frame: $differ of $total pixels differ")
    }

    /** What the screen says and where: every node's words and description with its bounds, in order. */
    private fun tree(): List<String> = compose.onAllNodes(SemanticsMatcher("any") { true }, useUnmergedTree = true).fetchSemanticsNodes()
        .mapNotNull(::describe).sorted()

    private fun describe(node: SemanticsNode): String? {
        val words = node.config.getOrNull(SemanticsProperties.Text)?.joinToString()
        val description = node.config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString()
        if (words == null && description == null) return null
        val b = node.boundsInRoot
        return "${words.orEmpty()}|${description.orEmpty()}@${b.left.roundToInt()},${b.top.roundToInt()},${b.right.roundToInt()},${b.bottom.roundToInt()}"
    }

    /** The white round [code] in [frame]: the shortest run of pure white outward from its edges, on each side at three points. */
    private fun margin(frame: ImageBitmap, code: Rect): Int {
        val bitmap = frame.asAndroidBitmap().copy(Bitmap.Config.ARGB_8888, false)
        fun run(x0: Int, y0: Int, dx: Int, dy: Int): Int {
            var n = 0
            var x = x0 + dx
            var y = y0 + dy
            while (x in 0 until bitmap.width && y in 0 until bitmap.height && bitmap.getPixel(x, y) == WHITE && n < CAP) {
                n++
                x += dx
                y += dy
            }
            // The screen's edge counts as the end of the white, not a shortfall: the enlarged code's white is the screen.
            return if (x !in 0 until bitmap.width || y !in 0 until bitmap.height) CAP else n
        }
        val l = code.left.roundToInt() - 1
        val r = code.right.roundToInt()
        val t = code.top.roundToInt() - 1
        val b = code.bottom.roundToInt()
        val runs = mutableListOf<Int>()
        for (f in listOf(0.25f, 0.5f, 0.75f)) {
            val y = (code.top + code.height * f).roundToInt()
            val x = (code.left + code.width * f).roundToInt()
            runs += run(l, y, -1, 0)
            runs += run(r, y, 1, 0)
            runs += run(x, t, 0, -1)
            runs += run(x, b, 0, 1)
        }
        return runs.min()
    }

    private fun save(frame: ImageBitmap, name: String) {
        val dir = InstrumentationRegistry.getArguments().getString("additionalTestOutputDir")?.let(::File)
            ?: File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(Environment.DIRECTORY_PICTURES), "frames")
        dir.mkdirs()
        File(dir, "$name.png").outputStream().use { frame.asAndroidBitmap().compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private companion object {
        val Points = listOf(0.25f, 0.5f, 0.75f)
        const val WHITE = -0x1

        /** A frame's worth of a move's progress, at most: 16.7 ms of 450. */
        const val HAIR = 0.002f

        /** Longer than any rest margin: a run this long is plenty. */
        const val CAP = 400
    }
}
