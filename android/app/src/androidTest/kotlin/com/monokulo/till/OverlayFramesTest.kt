package com.monokulo.till

import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import androidx.compose.ui.test.swipeWithVelocity
import androidx.compose.ui.geometry.Rect
import com.monokulo.till.overlay.CODE_SLOT_TAG
import com.monokulo.till.overlay.HERO_TAG
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.overlay.CARD_TAG
import com.monokulo.till.overlay.CODE_TAG
import com.monokulo.till.overlay.FloatingPanel
import com.monokulo.till.overlay.Panel
import com.monokulo.till.overlay.PanelActions
import com.monokulo.till.overlay.PanelMotion
import com.monokulo.till.overlay.PanelPage
import com.monokulo.till.overlay.PhonePanel
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.mk
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.onNodeWithContentDescription
import com.monokulo.till.overlay.BUBBLE_MARK_TAG
import com.monokulo.till.overlay.BubbleFace
import com.monokulo.till.ui.LensLook
import com.monokulo.till.ui.TillGeometry
import androidx.compose.ui.graphics.asAndroidPath
import androidx.compose.ui.graphics.vector.PathParser
import kotlin.math.roundToInt
import org.junit.Rule
import org.junit.Test
import java.io.File
import kotlin.math.abs
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import uniffi.till_screens.BubbleLook

/**
 * The overlay's moves stopped at a quarter, a half and three quarters of
 * their spring's settling time, each frame saved for reviewers
 * (`docs/reports/motion/frames/`; the files land in the app's external
 * files dir, `frames/`) and the payment code checked on each: square and
 * not clipped, so it would scan whichever frame a camera caught. A
 * recording on the software emulator can't show these frames (decision
 * 171); the test clock can.
 */
class OverlayFramesTest {
    @get:Rule
    val compose = createComposeRule()

    private var lowered = 0
    private var opened = 0
    private val actions = PanelActions(
        onLower = { lowered++ }, onFlung = { velocity -> motion.raise.exit(velocity, landsAt = Panel.IN_BUBBLE) }, onOpenShift = { opened++ }, onOrder = { opened++ }, onBackToCode = {},
        onCopy = { opened++ }, onCopyXmr = { opened++ }, onFullScreen = { opened++ },
    )
    /** Fifteen minutes from now, so the frames show a countdown a cashier would see. */
    private val expires = System.currentTimeMillis() / 1000 + 15 * 60
    private var sale by mutableStateOf(testSale("f1", expiresAt = expires))
    private var page by mutableStateOf<PanelPage>(PanelPage.Code)
    private lateinit var motion: PanelMotion
    /** Where the free bubble is on screen. */
    private val freeBubble = mutableStateOf<Offset?>(null)

    /** `motion.surface` settles in about this long (`docs/motion.md` §2): the seek points are its quarters. */
    private val surfaceMs = 450L

    /** The panel over a backdrop: a strong colour by default, so the card's edges show; [white] for Square's own ground. */
    private fun panel(raised: Float, white: Boolean = false, tablet: Boolean = false, presets: Motion = Motion.Full) {
        compose.mainClock.autoAdvance = false
        freeBubble.value = Offset(1200f, 900f)
        motion = PanelMotion(raised, freeBubble)
        compose.setContent {
            Till(presets) {
                motion.raise.Attach()
                Box(Modifier.fillMaxSize().background(if (white) Fixed.White else mk.danger).testTag("window")) {
                    if (tablet) {
                        FloatingPanel(panelOf(sale), "Front Counter", page, actions, motion)
                    } else {
                        PhonePanel(panelOf(sale), "Front Counter", BubbleLook.LIT, page, actions, motion)
                    }
                }
            }
        }
        compose.mainClock.advanceTimeBy(100)
    }

    private fun save(move: String, quarter: Int, image: Bitmap = compose.onNodeWithTag("window").captureToImage().asAndroidBitmap()) {
        val dir = File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null), "frames").apply { mkdirs() }
        val small = Bitmap.createScaledBitmap(image, image.width / 4, image.height / 4, true)
        File(dir, (if (move.startsWith("tablet")) "" else "phone-") + "$move-q$quarter.png").outputStream().use { small.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    /** The code, wherever it shows on this frame, is square and drawn whole. */
    private fun assertCodeWhole(move: String, quarter: Int) {
        compose.onAllNodesWithTag(CODE_TAG, useUnmergedTree = true).fetchSemanticsNodes().forEach { code ->
            val drawn = code.boundsInRoot
            if (drawn.width <= 0f) return@forEach
            assertTrue(abs(drawn.width - drawn.height) <= 2f, "$move q$quarter: the code is ${drawn.width} x ${drawn.height}, not square")
        }
    }

    /**
     * The code is drawn at full strength on this frame: its darkest modules
     * are near black. A code faded in shows them grey, and a camera already
     * on the panel would see a faint code (§10 rule 6).
     */
    private fun assertCodeOpaque(move: String, quarter: Int) {
        val image = compose.onNodeWithTag("window").captureToImage().asAndroidBitmap()
        val code = compose.onNodeWithTag(CODE_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        var darkest = 1f
        for (y in code.top.toInt().coerceAtLeast(0) until code.bottom.toInt().coerceAtMost(image.height) step 2) {
            for (x in code.left.toInt().coerceAtLeast(0) until code.right.toInt().coerceAtMost(image.width) step 2) {
                val p = image.getPixel(x, y)
                darkest = minOf(darkest, (android.graphics.Color.red(p) + android.graphics.Color.green(p) + android.graphics.Color.blue(p)) / (3f * 255f))
            }
        }
        assertTrue(darkest < 0.15f, "$move q$quarter: the code's darkest module is $darkest light, so it is faded")
    }

    /** Seeks [move] to its quarters; from quarter [opaqueFrom] on, the code must be drawn at full strength. */
    private fun seek(move: String, opaqueFrom: Int = 4, start: () -> Unit) {
        start()
        var at = 0L
        for (quarter in 1..3) {
            val to = surfaceMs * quarter / 4
            compose.mainClock.advanceTimeBy(to - at)
            at = to
            save(move, quarter)
            assertCodeWhole(move, quarter)
            if (quarter >= opaqueFrom) assertCodeOpaque(move, quarter)
        }
        compose.mainClock.advanceTimeBy(1_500)
        save(move, 4)
    }

    @Test
    fun thePanelsRaise() {
        panel(raised = 0f)
        seek("raise") { motion.raise.enter() }
    }

    @Test
    fun thePanelsLower() {
        panel(raised = 1f)
        seek("lower") { motion.raise.exit() }
    }

    /**
     * How dark the band just under the card is on a white backdrop, 0 to 1:
     * the card's shadow, which nothing else draws there.
     */
    private fun shadowUnderCard(image: Bitmap): Float {
        val card = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val top = card.bottom.toInt() + 2
        val bottom = (card.bottom + card.height * 0.04f).toInt().coerceAtMost(image.height - 1)
        var darkest = 0f
        for (y in top..bottom) {
            for (x in (card.left + card.width / 4).toInt()..(card.right - card.width / 4).toInt()) {
                val p = image.getPixel(x, y)
                val light = (android.graphics.Color.red(p) + android.graphics.Color.green(p) + android.graphics.Color.blue(p)) / (3f * 255f)
                darkest = maxOf(darkest, 1f - light)
            }
        }
        return darkest
    }

    /** Lowered over Square's white, the shadow goes with the card: never darker than the card is opaque (decision 191). */
    @Test
    fun theLowersShadowGoesWithTheCard() {
        panel(raised = 1f, white = true)
        val atRest = shadowUnderCard(compose.onNodeWithTag("window").captureToImage().asAndroidBitmap())
        assertTrue(atRest > 0.03f, "the card casts a shadow at rest ($atRest)")
        motion.raise.exit()
        var at = 0L
        for (quarter in 1..3) {
            val to = surfaceMs * quarter / 4
            compose.mainClock.advanceTimeBy(to - at)
            at = to
            val image = compose.onNodeWithTag("window").captureToImage().asAndroidBitmap()
            save("lower-light", quarter, image)
            val shown = Panel.shown(motion.raise.value)
            val shadow = shadowUnderCard(image)
            assertTrue(shadow <= atRest * shown + 0.02f, "q$quarter: the shadow is $shadow dark under a card $shown opaque (at rest $atRest)")
        }
    }

    @Test
    fun thePaidFold() {
        panel(raised = 1f)
        seek("paid-fold") { sale = testSale("f1", SalePhase.PAID, expiresAt = expires) }
    }

    @Test
    fun theNextSaleGrowingOutOfThePaidTick() {
        sale = testSale("f1", SalePhase.PAID, expiresAt = expires)
        panel(raised = 1f)
        seek("next-sale", opaqueFrom = 1) { sale = testSale("f2", expiresAt = expires) }
    }

    @Test
    fun theTabletsCodeComingBackFromTheOrder() {
        sale = sale.copy(receipt = true)
        page = PanelPage.Order
        panel(raised = 1f, tablet = true)
        compose.mainClock.advanceTimeBy(1_000)
        seek("tablet-back-to-code", opaqueFrom = 1) { page = PanelPage.Code }
    }

    @Test
    fun thePhonesCodeComingBackFromTheOrderUnderStill() {
        sale = sale.copy(receipt = true)
        page = PanelPage.Order
        panel(raised = 1f, presets = Motion.Still)
        compose.mainClock.advanceTimeBy(1_000)
        // Nothing grows under Still: the order crossfades off the code (fadeQuick), which is whole under it from then on.
        seek("still-back-to-code", opaqueFrom = 2) { page = PanelPage.Code }
    }

    @Test
    fun theOrderLineArrivingAfterTheSale() {
        panel(raised = 1f)
        seek("late-order", opaqueFrom = 1) { sale = sale.copy(receipt = true) }
    }

    @Test
    fun theOrderGrowingOutOfTheCard() {
        panel(raised = 1f)
        seek("order-grow") { page = PanelPage.Order }
    }

    /** How far [a] and [b] are apart, 0 to 3. */
    private fun apart(a: Int, b: Int): Float =
        (abs(android.graphics.Color.red(a) - android.graphics.Color.red(b)) + abs(android.graphics.Color.green(a) - android.graphics.Color.green(b)) +
            abs(android.graphics.Color.blue(a) - android.graphics.Color.blue(b))) / 255f

    private fun window(): Bitmap = compose.onNodeWithTag("window").captureToImage().asAndroidBitmap()

    /**
     * Lowered, nothing of the card is left once the docked bubble is back
     * where the free one will take over (decision 227): on every frame from
     * the one the bubble is within 4 dp of its place (to the eye, landed), every pixel of the
     * card's place at rest, but for the bubble's own corner, is the
     * backdrop; and the collapse lands (the window may go) no later than
     * two frames after that.
     */
    @Test
    fun theLowerLeavesNoCardOnceTheBubbleIsHome() {
        panel(raised = 1f)
        val card = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val backdrop = window().getPixel(4, 4)
        val home = Offset(1200f, 900f)
        var landedAt = -1
        var frame = 0
        motion.raise.exit(landsAt = Panel.IN_BUBBLE) { landedAt = frame }
        var homeAt = -1
        val history = mutableListOf<Float>()
        val centres = mutableListOf<Offset>()
        while (frame < 120 && (homeAt < 0 || frame < homeAt + 4)) {
            frame++
            compose.mainClock.advanceTimeByFrame()
            history += motion.raise.value
            val bubble = compose.onAllNodesWithContentDescription("Lower to the bubble", useUnmergedTree = true).fetchSemanticsNodes()
                .filter { abs(it.size.width - it.size.height) <= 2 }
                .minByOrNull { abs(it.size.width - 80 * compose.density.density) } ?: continue
            val centre = bubble.positionOnScreen + Offset(bubble.size.width / 2f, bubble.size.height / 2f)
            centres += centre
            if (homeAt < 0 && (centre - home).getDistance() <= 4 * compose.density.density) homeAt = frame
            // The capture is the frame last drawn, one behind the layout the bubble's place is read from.
            if (homeAt < 0 || frame == homeAt) continue
            val image = window()
            var left = 0
            var firstLeft = ""
            var lx = 9999; var rx = 0; var ty = 9999; var by = 0
            for (y in card.top.toInt() + 4 until card.bottom.toInt().coerceAtMost(image.height) - 4 step 6) {
                for (x in card.left.toInt() + 4 until (card.right - card.width / 4).toInt() step 6) {
                    if (apart(image.getPixel(x, y), backdrop) > 0.03f) {
                        if (left == 0) firstLeft = "($x,$y) ${Integer.toHexString(image.getPixel(x, y))} vs ${Integer.toHexString(backdrop)} card $card"
                        lx = minOf(lx, x); rx = maxOf(rx, x); ty = minOf(ty, y); by = maxOf(by, y)
                        left++
                    }
                }
            }
            assertEquals(0, left, "frame $frame (raise ${motion.raise.value}), the bubble at $centre home since $homeAt: $left pixels of the card are left, first $firstLeft, in [$lx,$ty]-[$rx,$by], raise by frame $history")
        }
        assertTrue(homeAt > 0, "the docked bubble gets home")
        assertTrue(landedAt in 0..homeAt + 2, "the collapse lands on frame $landedAt, the bubble is home on $homeAt")
    }

    /** The folding surface's width through the slot's middle: everything there that isn't the card's own ground. */
    private fun surfaceWidth(image: Bitmap): Int {
        val slot = compose.onNodeWithTag(CODE_SLOT_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val card = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val y = slot.center.y.toInt()
        val ground = image.getPixel(card.left.toInt() + 6, y)
        val middle = slot.center.x.toInt()
        var from = middle
        while (from > card.left + 6 && apart(image.getPixel(from - 1, y), ground) > 0.04f) from--
        var to = middle
        while (to < card.right - 6 && apart(image.getPixel(to + 1, y), ground) > 0.04f) to++
        return to - from + 1
    }

    /**
     * Paid, one surface shrinks from the code's card into the hero's circle
     * (decision 228): on every frame there is a ground at least the hero's
     * size round the tick, it never grows on the way, and it passes through
     * at least three sizes between the card and the circle.
     */
    @Test
    fun thePaidFoldShrinksOneSurfaceIntoTheHero() {
        panel(raised = 1f)
        val density = compose.density.density
        val card = surfaceWidth(window())
        val hero = (88 * density).toInt()
        assertTrue(abs(card - 264 * density) < 6 * density, "at rest the surface is the code's card: $card px")
        sale = testSale("f1", SalePhase.PAID, expiresAt = expires)
        val widths = mutableListOf<Int>()
        repeat(45) {
            compose.mainClock.advanceTimeByFrame()
            widths += surfaceWidth(window())
        }
        widths.forEachIndexed { frame, w -> assertTrue(w >= hero - 4 * density, "frame $frame: the surface is $w px, under the hero's $hero") }
        widths.zipWithNext().forEachIndexed { frame, (a, b) -> assertTrue(b <= a + 2 * density, "frame $frame: the surface grew from $a to $b px") }
        val between = widths.filter { it < card - 8 * density && it > hero + 8 * density }.map { it / (4 * density).toInt() }.toSet()
        assertTrue(between.size >= 3, "the surface passes through ${between.size} sizes between the card and the hero: $widths")
        assertTrue(abs(widths.last() - hero) < 4 * density, "it lands at the hero's size: ${widths.last()} px")
        compose.onNodeWithTag(HERO_TAG, useUnmergedTree = true).assertExists()
    }

    /** How strongly [bounds] is inked over the card's ground, 0 to 3: the darkest (or, in dark, lightest) pixel's distance from it. */
    private fun ink(image: Bitmap, bounds: Rect, ground: Int): Float {
        var most = 0f
        for (y in bounds.top.toInt().coerceAtLeast(0) until bounds.bottom.toInt().coerceAtMost(image.height)) {
            for (x in bounds.left.toInt().coerceAtLeast(0) until bounds.right.toInt().coerceAtMost(image.width)) {
                most = maxOf(most, apart(image.getPixel(x, y), ground))
            }
        }
        return most
    }

    private fun boundsOf(text: String): Rect? =
        compose.onAllNodesWithText(text, useUnmergedTree = true).fetchSemanticsNodes().firstOrNull()?.boundsInRoot

    /**
     * The panel moving on from a paid sale to the next one waiting: the old
     * sale's "… received" and the new sale's header are never both above
     * half their strength on one frame (decision 226).
     */
    @Test
    fun theNextSalesHeaderNeverMeetsTheLastSalesAmount() {
        val paid = testSale("f1", SalePhase.PAID, expiresAt = expires)
        // A long reference, so the next header reaches past where the last one's words end and its ink is its own there.
        val next = testSale("f2", expiresAt = expires).copy(amount = "9.80", xmr = "0.0392", reference = "f2 Back counter tab")
        sale = paid
        panel(raised = 1f)
        compose.mainClock.advanceTimeBy(1_500)
        val old = "${paid.xmr} XMR received"
        val header = panelOf(next).title
        val card = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val groundAt = { image: Bitmap -> image.getPixel(card.left.toInt() + 6, card.top.toInt() + (card.height / 2).toInt()) }
        val before = window()
        val oldFull = ink(before, boundsOf(old)!!, groundAt(before))
        // Only the part of the header's line past the last header's words: what is drawn there is the next sale's.
        val lastTitle = boundsOf(panelOf(paid).title)!!
        var lastEnds = lastTitle.left.toInt()
        for (x in lastTitle.left.toInt() until lastTitle.right.toInt()) {
            for (y in lastTitle.top.toInt() until lastTitle.bottom.toInt()) {
                if (apart(before.getPixel(x, y), groundAt(before)) > 0.1f) lastEnds = x
            }
        }
        val headerPast = { r: Rect -> Rect(maxOf(r.left, lastEnds + 8f), r.top, r.right, r.bottom) }
        sale = next
        val frames = mutableListOf<Pair<Float, Float>>()
        repeat(40) {
            compose.mainClock.advanceTimeByFrame()
            val image = window()
            val ground = groundAt(image)
            val o = boundsOf(old)?.let { ink(image, it, ground) } ?: 0f
            val h = boundsOf(header)?.let { ink(image, headerPast(it), ground) } ?: 0f
            frames += o to h
        }
        compose.mainClock.advanceTimeBy(1_500)
        val after = window()
        val headerFull = ink(after, headerPast(boundsOf(header)!!), groundAt(after))
        assertTrue(headerFull > 0.3f, "the next header's words reach past the last one's: $headerFull")
        frames.forEachIndexed { frame, (o, h) ->
            assertTrue(
                o < oldFull / 2 || h < headerFull / 2,
                "frame $frame: the last sale's amount (${o / oldFull}) and the next sale's header (${h / headerFull}) are both drawn",
            )
        }
    }

    /**
     * A tap on the card's place while it grows out of the bubble lowers it
     * again and opens nothing (M1), wherever the scaled card's buttons
     * happen to be on that frame; at rest the same tap reaches the card.
     */
    @Test
    fun aTapDuringTheRaiseRetargetsItAndOpensNothing() {
        panel(raised = 1f)
        val card = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        // Where the header ("Open the shift") and the grabber are at rest, and the middle of the card.
        val dp = compose.density.density
        val spots = listOf(Offset(card.center.x, card.top + 24 * dp), Offset(card.left + card.width / 3, card.top + 70 * dp), card.center)
        spots.forEach { at ->
            motion.raise.snap(0f)
            compose.mainClock.advanceTimeByFrame()
            motion.raise.enter()
            compose.mainClock.advanceTimeBy(80)
            assertTrue(motion.raise.value in 0.05f..0.9f, "mid-raise: ${motion.raise.value}")
            compose.onNodeWithTag("window").performTouchInput { click(at) }
            compose.mainClock.advanceTimeByFrame()
        }
        assertEquals(0, opened, "a tap mid-raise opened something")
        assertEquals(spots.size, lowered, "each tap mid-raise lowers the panel")
        compose.mainClock.advanceTimeBy(1_500)
        compose.onNodeWithTag("window").performTouchInput { click(spots[1]) }
        compose.mainClock.advanceTimeByFrame()
        assertEquals(1, opened, "at rest the header's tap opens the shift")
    }

    /** The docked bubble's circle on this frame, in the root: its centre and radius. */
    private fun dockedCircle(): Pair<Offset, Float>? {
        val dp = compose.density.density
        val bubble = compose.onAllNodesWithContentDescription("Lower to the bubble", useUnmergedTree = true).fetchSemanticsNodes()
            .filter { abs(it.size.width - it.size.height) <= 2 && it.size.width > 0 }
            .minByOrNull { abs(it.size.width - 80 * dp) } ?: return null
        return bubble.boundsInRoot.center to 28 * dp
    }

    /** How far [circle] is from [rect]'s nearest edge, past its radius; 0 or less where they touch or overlap. */
    private fun gap(rect: Rect, circle: Pair<Offset, Float>): Float {
        val (c, r) = circle
        val dx = maxOf(rect.left - c.x, 0f, c.x - rect.right)
        val dy = maxOf(rect.top - c.y, 0f, c.y - rect.bottom)
        return kotlin.math.hypot(dx, dy) - r
    }

    /**
     * The card grows out of the docked bubble wherever the free bubble was,
     * and folds back into it on its way home (M1, decision 246): on every
     * frame of a raise and a lower, from a free bubble at the far edge (the
     * left, half way down) and one beside the dock, the card's scaled
     * bounds hold or touch the docked bubble's circle, as near as the
     * design's 12 dp between them at rest. Before 189 the card scaled about
     * the dock, so from the far edge it came out of the empty top corner
     * with the bubble 1000 px away.
     */
    @Test
    fun theCardGrowsOutOfTheBubbleOnEveryFrameWhereverTheBubbleIs() {
        val metrics = InstrumentationRegistry.getInstrumentation().targetContext.resources.displayMetrics
        val dp = metrics.density
        val places = mapOf(
            "far" to Offset(44 * dp, metrics.heightPixels / 2f),
            "near" to Offset(metrics.widthPixels - 44 * dp, 140 * dp),
        )
        val touching = 12 * dp + 2f
        panel(raised = 1f)
        places.forEach { (name, at) ->
            listOf("raise" to 0f, "lower" to 1f).forEach { (move, from) ->
                freeBubble.value = at
                motion.raise.snap(from)
                compose.mainClock.advanceTimeBy(100)
                if (from == 0f) motion.raise.enter() else motion.raise.exit(landsAt = Panel.IN_BUBBLE)
                var frames = 0
                var worst = -1f
                var worstAt = ""
                while (frames < 90) {
                    compose.mainClock.advanceTimeByFrame()
                    frames++
                    val raise = motion.raise.value
                    // Lowered, the window goes at IN_BUBBLE; what is past it is never drawn.
                    if (from == 1f && raise <= Panel.IN_BUBBLE) break
                    val card = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
                    val circle = dockedCircle() ?: continue
                    val g = gap(card, circle)
                    if (g > worst) { worst = g; worstAt = "frame $frames raise $raise card $card bubble ${circle.first}" }
                    assertTrue(g <= touching, "$name $move, $worstAt: the card is ${g / dp} dp from the docked bubble's circle")
                    if (from == 0f && raise >= 0.999f) break
                }
                assertTrue(frames > 4, "$name $move ran $frames frames")
            }
        }
    }

    /**
     * A grabber flung down keeps going down after the finger lets go (M2,
     * decision 247): the drag runs on at the release speed while the raise
     * collapses, so the card's top is lower on each of the first two frames
     * after the release than on the one before, then the collapse takes it
     * up into the bubble. Before 190 the top turned up on the first frame.
     * The speed is the review's fling (about 6000 px a second, 98 px a
     * frame on the recording).
     */
    @Test
    fun aFlungPanelKeepsTheFingersWayAfterTheRelease() {
        panel(raised = 1f)
        // The free bubble at the top of the end edge, beside the dock, where the panel is mostly raised from.
        val metrics = InstrumentationRegistry.getInstrumentation().targetContext.resources.displayMetrics
        freeBubble.value = Offset(metrics.widthPixels - 44 * metrics.density, 140 * metrics.density)
        compose.mainClock.advanceTimeBy(100)
        val grabber = compose.onAllNodesWithContentDescription("Lower to the bubble", useUnmergedTree = true).fetchSemanticsNodes()
            .first { it.size.width > it.size.height }
        val start = grabber.boundsInRoot.center
        compose.onNodeWithTag("window").performTouchInput {
            swipeWithVelocity(start, start + Offset(0f, 420f), endVelocity = 6000f, durationMillis = 120)
        }
        val tops = mutableListOf<Float>()
        val raises = mutableListOf<Float>()
        repeat(14) {
            tops += compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.top
            raises += motion.raise.value
            compose.mainClock.advanceTimeByFrame()
        }
        // The release is the last frame drawn at rest, the finger's last place; the two after it carry on down.
        val release = raises.indexOfFirst { it < 1f } - 1
        assertTrue(release in 0..2, "the collapse starts with the release: raises $raises")
        assertTrue(tops[release + 1] > tops[release] && tops[release + 2] > tops[release + 1], "flung down, the card's top goes on down: tops $tops raises $raises")
        assertTrue(tops.last() < tops.max(), "then the collapse takes it into the bubble: tops $tops")
    }

    /**
     * Thrown and raised again before it was gone (the docked bubble tapped
     * mid-collapse), the card comes back to its own place, not a throw's
     * length below it (decision 247).
     */
    @Test
    fun aThrownPanelRaisedAgainComesBackToItsPlace() {
        panel(raised = 1f)
        val rest = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val grabber = compose.onAllNodesWithContentDescription("Lower to the bubble", useUnmergedTree = true).fetchSemanticsNodes()
            .first { it.size.width > it.size.height }
        val start = grabber.boundsInRoot.center
        compose.onNodeWithTag("window").performTouchInput {
            swipeWithVelocity(start, start + Offset(0f, 420f), endVelocity = 6000f, durationMillis = 120)
        }
        repeat(5) { compose.mainClock.advanceTimeByFrame() }
        assertTrue(motion.raise.value in 0.2f..0.95f, "mid-collapse: ${motion.raise.value}")
        motion.raise.enter()
        compose.mainClock.advanceTimeBy(1_500)
        val back = compose.onNodeWithTag(CARD_TAG, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        assertEquals(rest.top, back.top, 1f, "raised again, the card is at its place")
    }

    // The bubble as the terminal (`docs/motion.md` §12.5, Monokulo's "Keys count" round form): the running segment, the border's fill per step, the paid spread and tick, Still, its label.

    private var look by mutableStateOf(BubbleLook.IDLE)
    private var step by mutableStateOf(1)

    /** The free bubble over Square's white, in [look] from its first frame, on the test clock; light unless [dark]. */
    private fun bubble(from: BubbleLook, presets: Motion = Motion.Full, label: String? = null, dark: Boolean = false, at: Int = 1) {
        look = from
        step = at
        compose.mainClock.autoAdvance = false
        compose.setContent {
            Till(presets, dark = dark) {
                Box(Modifier.background(Fixed.White)) { BubbleFace(look, label = label, step = step) }
            }
        }
        compose.mainClock.advanceTimeByFrame()
    }

    private fun mark(): Bitmap = compose.onNodeWithTag(BUBBLE_MARK_TAG, useUnmergedTree = true).captureToImage().asAndroidBitmap()

    /** The pixel at ([u], [v]) of the canvas's 120 grid, as `till()` draws it: the screen scaled 1.22 about (48, 51) in the disc's 100-unit box. */
    private fun Bitmap.atTill(u: Float, v: Float): Int {
        val unit = width / TillGeometry.BOX
        val x = (unit * (TillGeometry.BOX / 2 + TillGeometry.SCALE * (u - TillGeometry.SCREEN_X))).roundToInt().coerceIn(0, width - 1)
        val y = (unit * (TillGeometry.BOX / 2 + TillGeometry.SCALE * (v - TillGeometry.SCREEN_Y))).roundToInt().coerceIn(0, height - 1)
        return getPixel(x, y)
    }

    private val borderPath = android.graphics.PathMeasure(PathParser().parsePathString(TillGeometry.BORDER).toPath().asAndroidPath(), false)

    /** The middle of the border's stroke [along] (0..1, wrapping) of the way round from 12 o'clock, clockwise. */
    private fun Bitmap.onBorder(along: Float): Int {
        val at = floatArrayOf(0f, 0f)
        borderPath.getPosTan((((along % 1f) + 1f) % 1f) * borderPath.length, at, null)
        return atTill(at[0], at[1])
    }

    private fun near(pixel: Int, colour: androidx.compose.ui.graphics.Color, within: Float = 0.12f): Boolean =
        apart(pixel, colour.toArgb()) <= within

    /** Which of [a] and [b] [pixel] is nearer. */
    private fun nearer(pixel: Int, a: androidx.compose.ui.graphics.Color, b: androidx.compose.ui.graphics.Color): Boolean =
        apart(pixel, a.toArgb()) < apart(pixel, b.toArgb())

    /** Every 20th of the border, from [from] to [to] of the way round, is one colour, [colour]. */
    private fun assertBorder(image: Bitmap, from: Float, to: Float, colour: androidx.compose.ui.graphics.Color, what: String) {
        var along = from + 0.01f
        while (along < to - 0.01f) {
            val p = image.onBorder(along)
            assertTrue(near(p, colour), "$what: the border at $along is ${Integer.toHexString(p)}, not ${Integer.toHexString(colour.toArgb())}")
            along += 0.05f
        }
    }

    /**
     * Waiting for the scan, an orange segment runs clockwise round the dim
     * track from 12 o'clock, a turn every [Motion.CHASE_MS]: at three points
     * of the turn the segment's middle is orange and the border across from
     * it and just behind it is the track.
     */
    @Test
    fun theWaitingBubblesSegmentRunsClockwise() {
        bubble(BubbleLook.LIT)
        val turn = Motion.CHASE_MS.toFloat()
        var at = 0L
        for (third in 0..2) {
            val to = (turn * third / 3).toLong()
            compose.mainClock.advanceTimeBy(to - at)
            at = to
            val image = mark()
            val start = to / turn
            val middle = start + TillGeometry.RUN_SHARE / 2
            save("bubble-run", third, image)
            assertTrue(near(image.onBorder(middle), Fixed.Accent), "${to} ms: the segment's middle at $middle is ${Integer.toHexString(image.onBorder(middle))}, not orange")
            assertTrue(near(image.onBorder(middle + 0.5f), Fixed.TerminalDim), "${to} ms: across from the segment is the dim track")
            assertTrue(near(image.onBorder(start - 0.08f), Fixed.TerminalDim), "${to} ms: behind the segment is the track: it runs clockwise")
        }
    }

    /** The border fills a third when the payment is seen and two thirds confirming, clockwise from 12 o'clock, in orange; nothing runs. */
    @Test
    fun theBorderFillsAThirdAStep() {
        bubble(BubbleLook.LIT, at = 2)
        compose.mainClock.advanceTimeBy(1_500)
        val seen = mark()
        save("bubble-step", 2, seen)
        assertBorder(seen, 0f, 1f / 3, Fixed.Accent, "seen")
        assertBorder(seen, 1f / 3 + 0.04f, 1f, Fixed.TerminalDim, "seen")
        step = 3
        compose.mainClock.advanceTimeBy(1_500)
        val confirming = mark()
        save("bubble-step", 3, confirming)
        assertBorder(confirming, 0f, 2f / 3, Fixed.Accent, "confirming")
        assertBorder(confirming, 2f / 3 + 0.04f, 1f, Fixed.TerminalDim, "confirming")
    }

    /** In night mode the disc is the dark icon's charcoal and the unfilled border is ink; paid, the border is whole green. */
    @Test
    fun inDarkModeTheDiscIsCharcoalAndTheTrackInk() {
        bubble(BubbleLook.LIT, dark = true)
        compose.mainClock.advanceTimeBy(Motion.CHASE_MS / 3L)
        val image = mark()
        save("bubble-dark", 0, image)
        val start = 1f / 3
        assertTrue(near(image.onBorder(start + 0.5f), Fixed.Ink), "across from the segment is the ink track, not ${Integer.toHexString(image.onBorder(start + 0.5f))}")
        // Up and left of the screen, past the border's edge and inside the disc.
        val corner = (image.width * 0.17f).roundToInt()
        assertTrue(near(image.getPixel(corner, corner), Fixed.TerminalCharcoal), "between the border and the disc's edge is the charcoal disc, not white")
        look = BubbleLook.PAID
        compose.mainClock.advanceTimeBy(1_500)
        val paid = mark()
        save("bubble-dark", 1, paid)
        assertBorder(paid, 0f, 1f, Fixed.PaidLight, "dark, paid")
        assertTrue(near(paid.atTill(58.5f, 43.5f), Fixed.White, 0.15f), "the tick is still white")
    }

    @Test
    fun theIdleBubbleHasAGreyScreenAndAWholeDimBorder() {
        bubble(BubbleLook.IDLE)
        repeat(3) {
            compose.mainClock.advanceTimeBy(Motion.CHASE_MS / 3L)
            // The whole bubble at 55 % over white: its dim border, faded, the same all round, and no fill or segment on it.
            val image = mark()
            assertBorder(image, 0f, 1f, androidx.compose.ui.graphics.lerp(Fixed.White, Fixed.TerminalDim, 0.55f), "idle")
        }
    }

    /**
     * Paid: the screen turns green from the centre out, the centre square
     * first and the triangles a step behind, then a white tick draws along
     * its path over the green; the border fills round to whole green.
     */
    @Test
    fun paidTurnsTheScreenGreenFromTheCentreOutThenDrawsTheTick() {
        bubble(BubbleLook.LIT)
        compose.mainClock.advanceTimeBy(Motion.CHASE_MS / 3L)
        look = BubbleLook.PAID
        compose.mainClock.advanceTimeByFrame()
        val start = compose.mainClock.currentTime
        // A point of the centre square clear of the tick, one of the top triangle, and three along the tick.
        fun centre(image: Bitmap) = image.atTill(50f, 48f)
        fun triangle(image: Bitmap) = image.atTill(48f, 33f)
        val tickPoints = listOf(40.5f to 54.5f, 44.5f to 58.5f, 54.5f to 47.8f)
        fun anyTick(image: Bitmap) = tickPoints.any { (u, v) -> near(image.atTill(u, v), Fixed.White, 0.15f) }
        fun wholeTick(image: Bitmap) = tickPoints.all { (u, v) -> near(image.atTill(u, v), Fixed.White, 0.15f) }
        val triangleOrange = LensLook.Orange[1]
        val triangleGreen = LensLook.Green[1]
        var centreGreenAt = -1L
        var triangleGreenAt = -1L
        var tickAt = -1L
        var wholeAt = -1L
        var i = 0
        while (compose.mainClock.currentTime - start < 1_200) {
            val t = compose.mainClock.currentTime - start
            val image = mark()
            if (t in 0..400 && i % 4 == 0) save("bubble-paid", (t / 16).toInt(), image)
            i++
            if (centreGreenAt < 0 && near(centre(image), LensLook.Green[0], 0.06f)) centreGreenAt = t
            if (triangleGreenAt < 0 && near(triangle(image), triangleGreen, 0.06f)) triangleGreenAt = t
            if (tickAt < 0 && anyTick(image)) {
                tickAt = t
                assertTrue(near(centre(image), LensLook.Green[0], 0.06f), "$t ms: the tick is drawn on a centre still turning green")
            }
            if (wholeAt < 0 && wholeTick(image)) wholeAt = t
            if (t in 120..130) assertTrue(nearer(triangle(image), triangleOrange, triangleGreen), "$t ms: the centre is mostly green while the triangles are still orange")
            compose.mainClock.advanceTimeByFrame()
        }
        assertTrue(centreGreenAt in 150..240, "the centre is green as its 200 ms fade ends: $centreGreenAt ms")
        assertTrue(triangleGreenAt in centreGreenAt..320, "the triangles, 60 ms behind it: $triangleGreenAt ms")
        assertTrue(tickAt >= 200, "the tick waits for the centre: first drawn at $tickAt ms")
        assertTrue(wholeAt in tickAt..800, "and is drawn whole by 800 ms: $wholeAt ms")
        val done = mark()
        save("bubble-paid", 99, done)
        assertBorder(done, 0f, 1f, Fixed.PaidLight, "paid")
    }

    /** How light [pixel] is, 0 to 1. */
    private fun lightness(pixel: Int): Float =
        (0.2126f * android.graphics.Color.red(pixel) + 0.7152f * android.graphics.Color.green(pixel) + 0.0722f * android.graphics.Color.blue(pixel)) / 255f

    /**
     * Paid to Idle, the tick leaves by fading whole (decision 293): on every
     * frame its three points, start, corner and end, are each as far from
     * white as the others, lighter than the screen beside them while it is
     * still most of the way shown, and lighter frame by frame less; no point
     * goes to the screen while another is still white, as an un-drawn tick's
     * end would. Gone with the quick fade.
     */
    @Test
    fun paidToIdleFadesTheTickWholeNeverUndrawsIt() {
        bubble(BubbleLook.PAID)
        compose.mainClock.advanceTimeBy(1_000)
        // Along the tick, and beside each, about 4 units off its line inside the V, the screen with no tick on it.
        val along = listOf(40.5f to 54.5f, 44.5f to 58.5f, 54.5f to 47.8f)
        val beside = listOf(43.3f to 51.7f, 44.5f to 53.5f, 51.6f to 45.1f)
        fun shown(image: Bitmap): List<Float> = along.indices.map { i ->
            val on = lightness(image.atTill(along[i].first, along[i].second))
            val off = lightness(image.atTill(beside[i].first, beside[i].second))
            ((on - off) / (1f - off).coerceAtLeast(0.01f)).coerceIn(0f, 1f)
        }
        val whole = shown(mark())
        assertTrue(whole.all { it > 0.8f }, "paid at rest, the tick is whole and white: $whole")
        look = BubbleLook.IDLE
        compose.mainClock.advanceTimeByFrame()
        val start = compose.mainClock.currentTime
        var last = 1f
        var i = 0
        while (compose.mainClock.currentTime - start <= 200) {
            val t = compose.mainClock.currentTime - start
            val image = mark()
            save("bubble-untick", i++, image)
            val points = shown(image)
            assertTrue(points.max() - points.min() < 0.25f, "$t ms: the tick fades as one, its points $points")
            // Over its first 50 ms the 120 ms fade has the tick at least 0.58 shown.
            if (t <= 50) assertTrue(points.min() > 0.4f, "$t ms: still mostly shown, every point of it is: $points")
            assertTrue(points.average().toFloat() <= last + 0.05f, "$t ms: it only fades, ${points.average()} after $last")
            last = points.average().toFloat()
            compose.mainClock.advanceTimeByFrame()
        }
        assertTrue(last < 0.1f, "and is gone after the quick fade: $last")
    }

    /** A payment landing mid-turn holds the segment where it was as it fades: no jump to 12 o'clock. */
    @Test
    fun paidMidTurnFadesTheSegmentWhereItIs() {
        bubble(BubbleLook.LIT)
        val third = Motion.CHASE_MS / 3L
        compose.mainClock.advanceTimeBy(third)
        val middle = 1f / 3 + TillGeometry.RUN_SHARE / 2
        look = BubbleLook.PAID
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
        val image = mark()
        assertTrue(nearer(image.onBorder(middle), Fixed.Accent, Fixed.TerminalDim), "the segment is still at $middle as it fades")
        assertTrue(nearer(image.onBorder(middle + 0.5f), Fixed.TerminalDim, Fixed.Accent), "and the track across from it is still the track")
    }

    /** Under Still the waiting bubble has no segment and no loop, its screen orange on the dim track, and paid is green with the tick and the border whole after one quick fade. */
    @Test
    fun underStillNothingRunsAndTheColoursSayTheState() {
        val recompositions = Recompositions()
        try {
            bubble(BubbleLook.LIT, Motion.Still)
            compose.mainClock.advanceTimeBy(200)
            val waiting = mark()
            save("bubble-still", 0, waiting)
            assertBorder(waiting, 0f, 1f, Fixed.TerminalDim, "still, waiting")
            assertTrue(near(waiting.atTill(50f, 48f), LensLook.Orange[0], 0.06f), "the screen is orange")
            assertEquals(0, recompositions.count(INFINITE), "no infinite transition is made under Still")
            step = 2
            compose.mainClock.advanceTimeBy(200)
            val seen = mark()
            assertBorder(seen, 0f, 1f / 3, Fixed.Accent, "still, seen")
            look = BubbleLook.PAID
            compose.mainClock.advanceTimeByFrame()
            compose.mainClock.advanceTimeBy(140)
            val paid = mark()
            save("bubble-still", 1, paid)
            assertTrue(near(paid.atTill(50f, 48f), LensLook.Green[0], 0.06f) && near(paid.atTill(48f, 33f), LensLook.Green[1], 0.06f), "one quick fade turns the whole screen")
            assertTrue(near(paid.atTill(58.5f, 43.5f), Fixed.White, 0.15f), "the tick is whole")
            assertBorder(paid, 0f, 1f, Fixed.PaidLight, "still, paid")
        } finally {
            recompositions.close()
        }
    }

    /** The loops run only while they are seen: lit, the one infinite transition; idle or paid, none. */
    @Test
    fun onlyTheLitBubbleRunsALoop() {
        val recompositions = Recompositions()
        try {
            bubble(BubbleLook.IDLE)
            compose.mainClock.advanceTimeBy(500)
            assertEquals(0, recompositions.count(INFINITE), "idle runs no loop")
            look = BubbleLook.LIT
            compose.mainClock.advanceTimeBy(500)
            assertTrue(recompositions.count(INFINITE) > 0, "lit, the chase's loop is made")
        } finally {
            recompositions.close()
        }
    }

    @Test
    fun aPaidBubbleRunsNoLoop() {
        val recompositions = Recompositions()
        try {
            bubble(BubbleLook.PAID)
            compose.mainClock.advanceTimeBy(500)
            assertEquals(0, recompositions.count(INFINITE), "paid runs no loop")
        } finally {
            recompositions.close()
        }
    }

    /** TalkBack hears the step the bubble no longer shows (§12.5). */
    @Test
    fun theBubblesLabelSaysTheAmountAndTheStep() {
        var label by mutableStateOf<String?>(null)
        compose.mainClock.autoAdvance = false
        compose.setContent { Till { BubbleFace(look, label = label) } }
        // The engine's `BubbleView.label` (`till_screens::bubble::label`, whose words its own tests pin).
        val cases = listOf(
            BubbleLook.LIT to "Till, \$8.40 waiting for payment",
            BubbleLook.LIT to "Till, \$8.40 seen on the network",
            BubbleLook.LIT to "Till, \$8.40 confirming",
            BubbleLook.PAID to "Till, \$8.40 paid",
            BubbleLook.IDLE to "Till, nothing waiting",
        )
        for ((shown, says) in cases) {
            look = shown
            label = says
            compose.mainClock.advanceTimeByFrame()
            compose.onNodeWithContentDescription(says).assertExists()
        }
        label = null
        compose.mainClock.advanceTimeByFrame()
        assertEquals(0, compose.onAllNodesWithContentDescription("Till", substring = true).fetchSemanticsNodes().size, "a bubble with no label (onboarding's) says nothing")
    }

    private companion object {
        /** The trace name of the bubble's one infinite transition, as [Recompositions] counts it. */
        const val INFINITE = "androidx.compose.animation.core.rememberInfiniteTransition"
    }
}
