package com.monokulo.till

import uniffi.till_app.Screen
import android.graphics.Bitmap
import androidx.compose.ui.geometry.Rect
import androidx.activity.BackEventCompat
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.hasScrollToIndexAction
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToKey
import com.monokulo.till.RunningMoves.Companion.flashes
import com.monokulo.till.RunningMoves.Companion.lum
import com.monokulo.till.RunningMoves.Companion.range
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.codeGround
import org.junit.Rule
import org.junit.Test
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.math.hypot
import kotlin.math.min
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Every screen change run as the device runs it (frame by frame under the
 * one driver, [RunningMoves]), in light and dark, both ways, and checked on
 * every frame (decisions 203 to 206): one move from the first frame to the
 * last on both screens; the code at full alpha on its white; no glyph cut by
 * a ground's edge; and in the dark theme no flash. The seeked frames
 * ([MotionFramesTest]) hold the stage, the match and the overlay in one
 * state, which a running move doesn't.
 */
class RunningMovesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val moves = RunningMoves(compose, testSale("a1", expiresAt = 1_600).copy(receipt = sampleReceipt()))

    @Test fun aRowGrowsIntoItsSaleLight() = rowGrow(dark = false)

    @Test fun aRowGrowsIntoItsSaleDark() = rowGrow(dark = true)

    @Test fun aSaleShrinksIntoItsRowLight() = rowShrink(dark = false)

    @Test fun aSaleShrinksIntoItsRowDark() = rowShrink(dark = true)

    @Test fun theCodeGrowsLight() = codeGrow(dark = false)

    @Test fun theCodeGrowsDark() = codeGrow(dark = true)

    @Test fun theCodeShrinksLight() = codeShrink(dark = false)

    @Test fun theCodeShrinksDark() = codeShrink(dark = true)

    @Test fun theOrderGrowsLight() = orderGrow(dark = false)

    @Test fun theOrderGrowsDark() = orderGrow(dark = true)

    @Test fun theOrderShrinksLight() = orderShrink(dark = false)

    @Test fun theOrderShrinksDark() = orderShrink(dark = true)

    @Test fun aSaleRisesLight() = rise(dark = false)

    @Test fun aSaleRisesDark() = rise(dark = true)

    @Test fun thePaidFoldLight() = paidFold(dark = false)

    @Test fun thePaidFoldDark() = paidFold(dark = true)

    @Test fun backFromASaleThatRoseGoesIntoItsRowLight() = backAfterRise(dark = false)

    @Test fun backFromASaleThatRoseGoesIntoItsRowDark() = backAfterRise(dark = true)

    @Test fun theCodeGrowsUnderStillLight() = stillEnlarge(dark = false)

    @Test fun theCodeGrowsUnderStillDark() = stillEnlarge(dark = true)

    @Test fun closeForNowLeavesWithItsCodeLight() = leavesWithItsCode("close-for-now", dark = false) { it.copy(closedAt = 1_100) }

    @Test fun closeForNowLeavesWithItsCodeDark() = leavesWithItsCode("close-for-now", dark = true) { it.copy(closedAt = 1_100) }

    /** Back from a live sale (260 a): on every frame the row's ground holds its card, the code dims as the words do (313). */
    @Test fun backFromALiveSaleLeavesWithItsCodeLight() = leavesWithItsCode("back-live", dark = false) { it }

    @Test fun backFromALiveSaleLeavesWithItsCodeDark() = leavesWithItsCode("back-live", dark = true) { it }

    @Test fun aSaleExpiringAsItIsLeftLeavesWithItsCode() = leavesWithItsCode("expired-left", dark = false) { it.copy(phase = SalePhase.Expired, endedAt = 1_100) }

    @Test fun aSaleRefundedAsItIsLeftLeavesWithItsCode() = leavesWithItsCode("refunded-left", dark = true) { it.copy(refundedAt = 1_100, closedAt = 1_100) }

    @Test fun paidOnTheEnlargedCodeFoldsIntoTheHeroLight() = paidOnCode(dark = false)

    @Test fun paidOnTheEnlargedCodeFoldsIntoTheHeroDark() = paidOnCode(dark = true)

    @Test fun aRiseTakingOverABackAtItsStartIsComposedBeforeItMoves() = riseOverBack()

    /**
     * A sale that stops being live in the update that leaves its screen (closed from the sheet, expired, refunded)
     * leaves as an untouched one does (decision 242): the change's first frame is the frame before it, the leaving
     * screen keeps its code card on the live layout, and on every frame the row's ground still holds the card's place
     * the code is drawn there, dimming with the screen's words (decision 313); then the row's shrink takes it.
     */
    private fun leavesWithItsCode(name: String, dark: Boolean, change: (TillSale) -> TillSale) {
        moves.start(Screen.Sale("a1"), dark)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1", change = { moves.shown = change(moves.sale) })
        val failures = mutableListOf<String>()
        val card = frames.first().leavingCodes.firstOrNull { it.width > 20f }
        if (card == null) {
            failures += "the sale shows no code before the change"
        } else {
            val whole = Rect(0f, 0f, frames.first().bitmap.width.toFloat(), frames.first().bitmap.height.toFloat())
            if (changed(frames[1].bitmap, frames.first().bitmap, whole) || changed(frames[1].bitmap, frames.first().bitmap, card)) {
                failures += "f1: the change's first frame is not the frame before it"
            }
            val foot = Rect(card.left, card.bottom + 6f, card.right, card.bottom + 30f)
            if (dark && inkAndWhite(frames.first().bitmap, foot).second <= FOOT_SHARE) failures += "f0: the card's foot is not under the code at $foot"
            for (f in frames.drop(1).dropLast(RunningMoves.SETTLE_FRAMES)) {
                if (f.p >= 1f) continue
                if (f.leavingCodes.none { it.width > 20f }) failures += "f${f.index}: the leaving sale's code card is gone before the move landed (p ${f.p})"
                val g = f.ground ?: continue
                val holds = g.left <= card.left + 1f && g.top <= card.top + 1f && g.right >= card.right - 1f && g.bottom >= card.bottom - 1f
                val (ink, _) = inkAndWhite(f.bitmap, card)
                if (holds) dimsWithTheWords(f, frames, card, g)?.let { failures += it }
                // On a phone the card's foot is under the code: its white never shows on a frame the code is not drawn (on the
                // dark theme's ground, where it would be seen).
                if (dark && ink < CODE_SHARE && inkAndWhite(f.bitmap, foot).second > FOOT_SHARE) failures += "f${f.index}: the card's foot at $foot is drawn with no code above it (p ${f.p})"
            }
        }
        check(name, dark, frames, Move.RowGrow, glyphs = false, grows = true, bars = failures)
    }

    /**
     * A payment landing on the enlarged code (decision 241): one white surface from the whole screen to the paid
     * page's hero on every frame of the move, never smaller than the hero; the code never drawn outside its white;
     * the code screen's words ("Scan with any Monero wallet") and the page's ("Paid") never both readable.
     */
    private fun paidOnCode(dark: Boolean) {
        moves.start(Screen.Code("a1"), dark)
        val frames = moves.run(Screen.Sale("a1"), groundKey = codeGround("a1"), change = { moves.shown = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100) })
        val failures = mutableListOf<String>()
        val moving = frames.drop(1).dropLast(RunningMoves.SETTLE_FRAMES).filter { it.p < 1f }
        for (f in moving) {
            val g = f.ground
            if (g == null) {
                failures += "f${f.index}: no white surface (p ${f.p})"
                continue
            }
            val b = f.bitmap
            val inset = 3f
            // White just inside each edge, unless the hero (drawn over it as it lands) is there.
            for ((x, y) in listOf(g.left + inset to g.center.y, g.right - inset to g.center.y, g.center.x to g.top + inset, g.center.x to g.bottom - inset)) {
                if (x.toInt() !in 0 until b.width || y.toInt() !in 0 until b.height) continue
                if (f.hero?.contains(androidx.compose.ui.geometry.Offset(x, y)) == true && g.width < (f.hero?.width ?: 0f) * 1.5f) continue
                if (lum(b.getPixel(x.toInt(), y.toInt())) < WHITE) failures += "f${f.index}: the white at $g is not drawn at ($x, $y)"
            }
            val h = f.hero
            if (h != null && (g.width < h.width - 1f || g.height < h.height - 1f)) failures += "f${f.index}: the white at $g is smaller than the hero at $h"
            // Outside the white is the page's ground: in the dark theme no module and white of a code are drawn there.
            if (dark) for (c in f.leavingCodes.filter { it.width > 20f }) {
                for (r in listOf(Rect(c.left, c.top, c.right, min(c.bottom, g.top - EDGE)), Rect(c.left, maxOf(c.top, g.bottom + EDGE), c.right, c.bottom))) {
                    if (r.height <= 2f) continue
                    val (ink, white) = inkAndWhite(b, r)
                    if (ink > CODE_SHARE && white > CODE_SHARE) failures += "f${f.index}: the code is drawn outside its white at $r"
                }
            }
        }
        if (moving.size < 3) failures += "the fold drew ${moving.size} frames"
        failures += wordsTogether(frames)
        failures += groundTravels(frames)
        failures += startsAtOnce(moving)
        if (dark) failures += flashes(frames)
        report("paid-on-code-${theme(dark)}", frames, failures)
    }

    /**
     * A sale rising while a committed Back is still at 0 (decision 245): the takeover composes its first frame at 0
     * before its spring starts, as a change from rest does, then moves at once, from 0 and not a quarter of the way.
     */
    private fun riseOverBack() {
        moves.start(Screen.Sale("a1"), dark = false)
        moves.model.go(Screen.Till)
        compose.mainClock.advanceTimeByFrame()
        val frames = moves.run(Screen.Sale("b2"), byItself = true)
        val failures = mutableListOf<String>()
        val atZero = frames.drop(1).takeWhile { it.p == 0f }.size
        if (atZero > PRIMED + 1) failures += "the rise holds at 0 for $atZero frames: " + frames.joinToString { "%.2f".format(it.p) }
        val first = frames.drop(1).firstOrNull { it.p > 0f }
        if (first == null) failures += "the rise never moves" else if (first.p > FIRST_STEP) failures += "f${first.index}: the rise's first frame is at ${first.p}, past its first step"
        report("rise-over-back", frames, failures)
    }

    private fun rowGrow(dark: Boolean) {
        moves.start(Screen.Till, dark)
        check("row-grow", dark, moves.run(Screen.Sale("a1"), groundKey = "sale/a1"), Move.RowGrow, glyphs = true, grows = true)
    }

    private fun rowShrink(dark: Boolean) {
        moves.start(Screen.Sale("a1"), dark)
        // The shift's other rows stand still under the shrinking ground, opaque: a card going over them, not a fade.
        check("row-shrink", dark, moves.run(Screen.Till, groundKey = "sale/a1"), Move.RowGrow, glyphs = false, grows = true)
    }

    private fun codeGrow(dark: Boolean) {
        moves.start(Screen.Sale("a1"), dark)
        val frames = moves.run(Screen.Code("a1"), groundKey = codeGround("a1"))
        check("code-grow", dark, frames, Move.CodeGrow, glyphs = true, grows = true, bars = (if (dark) barsTurnAtHalf(frames) else emptyList()) + margins(frames))
    }

    private fun codeShrink(dark: Boolean) {
        moves.start(Screen.Code("a1"), dark)
        val frames = moves.run(Screen.Sale("a1"), groundKey = codeGround("a1"))
        check("code-shrink", dark, frames, Move.CodeGrow, glyphs = true, grows = true, bars = (if (dark) barsTurnAtHalf(frames) else emptyList()) + margins(frames))
    }

    /**
     * The code and its white are one surface (decision 220): on every frame of the move the code is inside its white,
     * and its margin on each side is never below the smaller of that side's margins at the move's two ends (the two
     * run on one line, so the margin runs straight from one to the other).
     */
    private fun margins(frames: List<RunningMoves.Frame>): List<String> {
        val both = frames.mapNotNull { f -> val g = f.ground; val c = f.code; if (g != null && c != null) Triple(f, g, c) else null }
        if (both.size < 3) return listOf("the code and its white move together on only ${both.size} frames")
        fun sides(g: Rect, c: Rect) = listOf(c.left - g.left, c.top - g.top, g.right - c.right, g.bottom - c.bottom)
        val first = sides(both.first().second, both.first().third)
        val last = sides(both.last().second, both.last().third)
        val names = listOf("left", "top", "right", "bottom")
        val out = mutableListOf<String>()
        for ((f, g, c) in both) {
            val m = sides(g, c)
            for (i in m.indices) {
                val least = min(first[i], last[i]) - MARGIN_SLACK
                if (m[i] < least) out += "f${f.index}: the code's ${names[i]} margin is ${"%.1f".format(m[i])} px, below the smaller rest margin ${"%.1f".format(least + MARGIN_SLACK)} (code $c, white $g)"
            }
        }
        return out
    }

    /**
     * Back from a sale that came up by itself, with its row on the shift (decision 222): the row it came from takes it
     * back, the same move as a sale the cashier opened, the code whole on its white or not drawn at all on every frame.
     */
    private fun backAfterRise(dark: Boolean) {
        moves.start(Screen.Till, dark)
        moves.run(Screen.Sale("a1"), byItself = true)
        check("back-after-rise", dark, moves.run(Screen.Till, groundKey = "sale/a1"), Move.RowGrow, glyphs = false, grows = true)
    }

    /**
     * In the dark theme the bars' icons turn dark for the code's white, and back, on the frame the code screen passes
     * half shown (M8), not on the change's first or last frame.
     */
    private fun barsTurnAtHalf(frames: List<RunningMoves.Frame>): List<String> {
        val code = Screen.Code("a1")
        val turn = frames.indexOfFirst { it.lightBars != frames.first().lightBars }
        if (turn < 0) return listOf("the bars' icons never turn")
        val at = frames[turn].shown[code] ?: return listOf("f${frames[turn].index}: no code screen as the bars turn")
        val before = frames[turn - 1].shown[code] ?: 0f
        val half = 0.5f
        val crossed = if (frames.last().lightBars) at >= half && before < half else at < half && before >= half
        return if (crossed) emptyList() else listOf("f${frames[turn].index}: the bars' icons turn with the code screen ${"%.2f".format(at)} shown (${"%.2f".format(before)} the frame before), not as it passes half")
    }

    private fun orderGrow(dark: Boolean) {
        moves.start(Screen.Sale("a1"), dark)
        check("order-grow", dark, moves.run(Screen.Order("a1"), groundKey = "order/a1"), Move.OrderGrow, glyphs = true, grows = true)
    }

    private fun orderShrink(dark: Boolean) {
        moves.start(Screen.Order("a1"), dark)
        check("order-shrink", dark, moves.run(Screen.Sale("a1"), groundKey = "order/a1"), Move.OrderGrow, glyphs = true, grows = true)
    }

    /** A paid sale's page back into its row: the hero becomes the row's badge, the ground shrinks without a hold. */
    @Test fun aPaidSaleStepsBackIntoItsRowLight() = paidStepBack(dark = false)

    @Test fun aPaidSaleStepsBackIntoItsRowDark() = paidStepBack(dark = true)

    private fun paidStepBack(dark: Boolean) {
        moves.shown = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100)
        moves.start(Screen.Sale("a1"), dark)
        check("paid-step-back", dark, moves.run(Screen.Till, groundKey = "sale/a1"), Move.RowGrow, glyphs = false, grows = true)
    }

    private fun rise(dark: Boolean) {
        moves.start(Screen.Till, dark)
        check("rise", dark, moves.run(Screen.Sale("a1"), byItself = true), Move.Rise, glyphs = false)
    }

    private fun stillEnlarge(dark: Boolean) {
        moves.start(Screen.Sale("a1"), dark, Motion.Still)
        check("still-grow", dark, moves.run(Screen.Code("a1")), move = null, glyphs = false)
    }

    /**
     * The paid fold: the hero's centre travels from the card's centre to the page's hero without standing still
     * while the fold runs, and the page's words come in only once it is past halfway (decision 188).
     */
    private fun paidFold(dark: Boolean) {
        moves.start(Screen.Sale("a1"), dark)
        val frames = moves.runFor(FOLD_MS, { moves.shown = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100) })
        val failures = mutableListOf<String>()
        if (dark) failures += flashes(frames)
        val card = frames.first().codes.first()
        val path = frames.mapNotNull { f -> f.hero?.let { f to it.center } }
        val rest = path.last().second
        val from = card.center
        val whole = hypot(rest.x - from.x, rest.y - from.y)
        var still = 0
        var last = from
        var halfway: Int? = null
        var started = false
        for ((f, c) in path) {
            val step = hypot(c.x - last.x, c.y - last.y)
            val left = hypot(rest.x - c.x, rest.y - c.y)
            // Counted from its first step (a change composes its new face on one frame and starts its clock on the
            // next, as every Compose animation does), until it is within a few pixels of rest (the spring's tail).
            if (step >= 0.5f) started = true
            if (started && left > NEAR && step < 0.5f) still++ else still = 0
            if (still > 1) failures += "f${f.index} the hero stands still at $c, ${"%.0f".format(left)} px from its place"
            if (hypot(c.x - last.x, c.y - last.y) > 0 && hypot(rest.x - last.x, rest.y - last.y) + 1f < left) failures += "f${f.index} the hero moves away from its place"
            if (halfway == null && left <= whole / 2) halfway = f.index
            last = c
        }
        val before = frames.first().bitmap
        // The heading's place is under the card as the fold begins: its ink counts once the hero has left it.
        val firstWords = frames.firstOrNull { f ->
            val r = f.heading
            r != null && f.hero?.overlaps(r) != true && ink(f.bitmap, r) && changed(f.bitmap, before, r)
        }?.index
        if (halfway != null && firstWords != null && firstWords < halfway) failures += "the page's words show at f$firstWords, before the hero is halfway at f$halfway"
        failures += foldKeepsItsSurface(path.map { it.first }, rest, dark)
        report("paid-fold-${theme(dark)}", frames, failures)
    }

    /**
     * The fold is one surface closing from the card into the hero (decision 225): the hero is drawn at three or more
     * sizes on its way, and in the dark theme, where the card's white, the hero's ground and the screen's are three
     * different lights, inside the hero's bounds there is never the screen's ground showing through on any frame
     * (the card's white gone before the hero came in).
     */
    private fun foldKeepsItsSurface(frames: List<RunningMoves.Frame>, rest: androidx.compose.ui.geometry.Offset, dark: Boolean): List<String> {
        val moving = frames.filter { f -> f.hero!!.center.let { hypot(it.x - rest.x, it.y - rest.y) } > NEAR }
        val sizes = moving.map { it.hero!!.width.toInt() }.distinct()
        val out = mutableListOf<String>()
        if (sizes.size < 3) out += "the hero is drawn at ${sizes.size} sizes between the card and its place: $sizes"
        if (!dark) return out
        for (f in moving) {
            val h = f.hero!!
            // Just inside each side's middle: the card's quiet zone (white, never a module) and inside the hero's circle.
            val i = maxOf(2f, h.width * 0.03f)
            val points = listOf(h.left + i to h.center.y, h.right - i to h.center.y, h.center.x to h.top + i, h.center.x to h.bottom - i)
            for ((x, y) in points) {
                if (x.toInt() !in 0 until f.bitmap.width || y.toInt() !in 0 until f.bitmap.height) continue
                if (lum(f.bitmap.getPixel(x.toInt(), y.toInt())) < SCREEN_GROUND_DARK) out += "f${f.index}: the screen's ground shows inside the folding card at ($x, $y)"
            }
        }
        return out
    }

    /** Every check a running [move] keeps, on each of its [frames]. */
    private fun check(name: String, dark: Boolean, frames: List<RunningMoves.Frame>, move: Move?, glyphs: Boolean, grows: Boolean = false, bars: List<String> = emptyList()) {
        val failures = bars.toMutableList()
        val moving = frames.drop(1).dropLast(RunningMoves.SETTLE_FRAMES)
        if (move != null) {
            for (f in moving) {
                val said = f.moves.filterNotNull()
                if (said.isEmpty()) failures += "f${f.index}: no screen says a move is under way before the stage lands"
                if (said.any { it != move }) failures += "f${f.index}: the screens disagree on the move: $said, not $move"
            }
        }
        // The ground is drawn through its clip on every frame of its move but the first and the last, where the match
        // may not be made yet, or already gone (decisions 203, 206): it never gives up half way.
        if (grows) {
            val shaped = moving.indices.filter { moving[it].ground != null }
            if (shaped.isEmpty()) failures += "the moving ground never has a shape"
            else for (i in shaped.first()..shaped.last()) if (moving[i].ground == null) failures += "f${moving[i].index}: the moving ground has no shape mid-move"
        }
        for (f in frames) failures += if (move == Move.CodeGrow) codeOnWhite(f) else codeNotGrey(f)
        if (move == Move.RowGrow) for (f in moving) failures += codeWholeOrNone(f)
        if (grows) failures += startsAtOnce(moving)
        if (move != null) failures += wordsTogether(frames)
        if (glyphs) for (f in moving) failures += cutGlyphs(f, frames.first().bitmap, frames.last().bitmap)
        if (grows) failures += groundTravels(frames)
        if (dark) failures += flashes(frames)
        report("$name-${theme(dark)}", frames, failures)
    }

    /**
     * The moving ground has a shape from the change's first frame, and travels within [PRIMED] frames more: the
     * arriving screen's first frame is drawn at 0 before the spring's clock starts (decision 235), and then it goes,
     * with no hold while the leaving screen's words go (decision 224).
     */
    private fun startsAtOnce(moving: List<RunningMoves.Frame>): List<String> {
        val first = moving.indexOfFirst { it.ground != null }
        if (first < 0) return emptyList()
        if (first > 1) return listOf("f${moving[first].index}: the ground first has a shape ${first + 1} frames into the move")
        val a = moving[first].ground!!
        val goes = moving.drop(first + 1).indexOfFirst { f ->
            val b = f.ground ?: return@indexOfFirst false
            abs(b.left - a.left) + abs(b.top - a.top) + abs(b.right - a.right) + abs(b.bottom - a.bottom) >= 1f
        }
        return if (goes < 0 || goes >= PRIMED + 1) listOf("f${moving[first].index}: the ground holds at $a for ${if (goes < 0) "the whole move" else "${goes + 1} frames"}") else emptyList()
    }

    /**
     * On a row's grow and shrink, the sale's code is drawn whole or not at all (decision 224): where the ground holds
     * its card, the code's darkest modules are there; where it doesn't, none of its modules show inside the ground.
     */
    private fun codeWholeOrNone(f: RunningMoves.Frame): List<String> {
        val g = f.ground ?: return emptyList()
        val out = mutableListOf<String>()
        for (c in f.codes.filter { it.width > 20f }) {
            val inside = Rect(maxOf(c.left, g.left), maxOf(c.top, g.top), min(c.right, g.right), min(c.bottom, g.bottom))
            if (inside.width <= 2f || inside.height <= 2f) continue
            val holds = g.left <= c.left + 1f && g.top <= c.top + 1f && g.right >= c.right - 1f && g.bottom >= c.bottom - 1f
            // Modules and the white between them: a code, as no ground of either theme is both.
            val (dark, white) = inkAndWhite(f.bitmap, inside)
            if (!holds && dark > CODE_SHARE && white > CODE_SHARE) out += "f${f.index}: part of the code at $c is drawn inside the ground at $g"
        }
        return out
    }

    /** How much of [r] in [b] is a module's ink, and how much the white round it. */
    private fun inkAndWhite(b: Bitmap, r: Rect): Pair<Float, Float> {
        var dark = 0
        var white = 0
        var n = 0
        for (y in r.top.toInt().coerceAtLeast(0) until r.bottom.toInt().coerceAtMost(b.height) step 2) {
            for (x in r.left.toInt().coerceAtLeast(0) until r.right.toInt().coerceAtMost(b.width) step 2) {
                val l = lum(b.getPixel(x, y))
                if (l < MODULE) dark++
                if (l >= WHITE) white++
                n++
            }
        }
        return if (n == 0) 0f to 0f else dark.toFloat() / n to white.toFloat() / n
    }

    /**
     * Two screens' words are never both readable on one frame (decisions 188, 221). How much of a text run shows on a
     * frame is read from the picture: the slope of its region's pixels against the same region at rest (the move's
     * first frame for the screen leaving, its last for the one arriving), which is its alpha whatever ground it is
     * drawn over and whatever else crosses it. No frame has a leaving and an arriving run both above half.
     */
    private fun wordsTogether(frames: List<RunningMoves.Frame>): List<String> {
        val first = frames.first()
        val last = frames.last()
        val from = first.words.entries.firstOrNull { it.value.isNotEmpty() }?.key ?: return emptyList()
        val to = last.words.entries.firstOrNull { it.key != from && it.value.isNotEmpty() }?.key ?: return emptyList()
        fun runs(f: RunningMoves.Frame, s: Screen) = f.words[s].orEmpty().filter { (_, r) -> r.width > 8f && r.height > 6f && spread(f.bitmap, r) > INK_SPREAD }
        val leaving = runs(first, from)
        val arriving = runs(last, to)
        val out = mutableListOf<String>()
        for (f in frames.drop(1).dropLast(1)) {
            val l = leaving.firstOrNull { (_, r) -> shows(f.bitmap, first.bitmap, last.bitmap, r) > HALF }
            val a = arriving.firstOrNull { (_, r) -> shows(f.bitmap, last.bitmap, first.bitmap, r) > HALF }
            if (l != null && a != null) out += "f${f.index}: \"${l.first}\" (leaving, ${"%.2f".format(shows(f.bitmap, first.bitmap, last.bitmap, l.second))}) and \"${a.first}\" (arriving, ${"%.2f".format(shows(f.bitmap, last.bitmap, first.bitmap, a.second))}) both show"
        }
        return out
    }

    /**
     * How much of [rest]'s picture in [region] shows in [f]: [f] fitted by least squares as a mix of the move's two
     * ends there ([rest] and [other], whose own likeness to each other is so taken out), [rest]'s weight.
     */
    private fun shows(f: Bitmap, rest: Bitmap, other: Bitmap, region: Rect, dx: Float = 0f, dy: Float = 0f): Float {
        val a = ArrayList<Float>()
        val b = ArrayList<Float>()
        val y = ArrayList<Float>()
        for (py in region.top.toInt().coerceAtLeast(0) until region.bottom.toInt().coerceAtMost(f.height)) {
            for (px in region.left.toInt().coerceAtLeast(0) until region.right.toInt().coerceAtMost(f.width)) {
                a += lum(rest.getPixel(px, py))
                b += lum(other.getPixel(px, py))
                val fx = (px + dx.roundToInt()).coerceIn(0, f.width - 1)
                val fy = (py + dy.roundToInt()).coerceIn(0, f.height - 1)
                y += lum(f.getPixel(fx, fy))
            }
        }
        if (a.size < 8) return 0f
        val ma = a.average()
        val mb = b.average()
        val my = y.average()
        var saa = 0.0
        var sbb = 0.0
        var sab = 0.0
        var say = 0.0
        var sby = 0.0
        for (i in a.indices) {
            val da = a[i] - ma
            val db = b[i] - mb
            val dy = y[i] - my
            saa += da * da
            sbb += db * db
            sab += da * db
            say += da * dy
            sby += db * dy
        }
        if (saa < 1.0) return 0f
        val det = saa * sbb - sab * sab
        // The two ends alike, or the other flat there: [rest] alone explains the frame.
        if (sbb < 1.0 || det < 1e-6 * saa * sbb) return (say / saa).toFloat()
        return ((say * sbb - sby * sab) / det).toFloat()
    }

    /** The standard deviation of [region]'s luminance in [b]: a text run's ink against its ground. */
    private fun spread(b: Bitmap, region: Rect): Float {
        var sum = 0.0
        var sq = 0.0
        var n = 0
        for (y in region.top.toInt().coerceAtLeast(0) until region.bottom.toInt().coerceAtMost(b.height)) {
            for (x in region.left.toInt().coerceAtLeast(0) until region.right.toInt().coerceAtMost(b.width)) {
                val l = lum(b.getPixel(x, y)).toDouble()
                sum += l
                sq += l * l
                n++
            }
        }
        if (n == 0) return 0f
        val m = sum / n
        return kotlin.math.sqrt((sq / n - m * m).coerceAtLeast(0.0)).toFloat()
    }

    private fun report(name: String, frames: List<RunningMoves.Frame>, failures: List<String>) {
        val bad = failures.mapNotNull { Regex("^f(\\d+)").find(it)?.groupValues?.get(1)?.toInt() }.toSet()
        frames.filter { it.index in bad || it.index == 0 || it === frames.last() }.take(12).forEach { RunningMoves.save(it, name) }
        assertTrue(failures.isEmpty(), "$name:\n" + failures.distinct().take(40).joinToString("\n"))
    }

    /**
     * The enlarge's white on [f], where its track says it is drawn (the code moves in the overlay, away from where
     * either side laid it out): white just inside each edge (the ground is drawn where its bounds are, decision 203),
     * and nothing grey inside it (the code on it whole, never through a fade or two codes through each other).
     */
    private fun codeOnWhite(f: RunningMoves.Frame): List<String> {
        val g = f.ground ?: return emptyList()
        val b = f.bitmap
        val inset = 6f
        val out = listOf(g.left + inset to g.center.y, g.right - inset to g.center.y, g.center.x to g.top + inset, g.center.x to g.bottom - inset)
            .filter { (x, y) -> x.toInt() in 0 until b.width && y.toInt() in 0 until b.height }
            .filter { (x, y) -> lum(b.getPixel(x.toInt(), y.toInt())) < WHITE }
            .map { (x, y) -> "f${f.index}: the code's white at $g is not drawn at ($x, $y)" }
            .toMutableList()
        greyShare(b, g)?.let { if (it > GREY_SHARE) out += "f${f.index}: the code on its white at $g is grey, drawn below full alpha (${(100 * it).toInt()} % grey)" }
        return out
    }

    /** How much of [r] in [b] is neither ink nor white: a code through a fade. Null if [r] is off the picture. */
    private fun greyShare(b: Bitmap, r: Rect): Float? {
        var grey = 0
        var n = 0
        for (y in r.top.toInt().coerceAtLeast(0) until r.bottom.toInt().coerceAtMost(b.height) step 2) {
            for (x in r.left.toInt().coerceAtLeast(0) until r.right.toInt().coerceAtMost(b.width) step 2) {
                if (lum(b.getPixel(x, y)) in GREY) grey++
                n++
            }
        }
        return if (n == 0) null else grey.toFloat() / n
    }

    /** The arriving screen's code on [f], where it is laid out: never grey, as a code drawn through a fade is (decision 205). */
    private fun codeNotGrey(f: RunningMoves.Frame): List<String> = f.codes.filter { it.width > 20f }.mapNotNull { code ->
        greyShare(f.bitmap, code)?.takeIf { it > GREY_SHARE }?.let { "f${f.index}: the code at $code is grey, drawn below full alpha (${(100 * it).toInt()} % grey)" }
    }

    /**
     * No arriving text run on [f] half under the moving ground's edge: ink on the side away from the ground and none
     * under it (decision 204). Words on the ground, clipped to it, are the other way about, and stay.
     */
    private fun cutGlyphs(f: RunningMoves.Frame, first: Bitmap, last: Bitmap): List<String> {
        val g = f.ground ?: return emptyList()
        val out = mutableListOf<String>()
        for ((text, t) in f.texts) {
            if (t.width < 4f || t.height < 4f || !t.overlaps(g)) continue
            val (under, beside) = split(t, g) ?: continue
            // The ink beside the edge is this text's (the picture there is more its rest picture than the first frame's),
            // not a leaving word's going under the ground there (decision 212).
            if (ink(f.bitmap, beside) && changed(f.bitmap, first, beside) && shows(f.bitmap, last, first, beside) > HALF && !ink(f.bitmap, under)) out += "f${f.index}: \"$text\" at $t is cut by the ground's edge at $g"
        }
        return out
    }

    /**
     * [t] split by the first of [g]'s edges that crosses it, inside and outside, each kept [EDGE] off the edge (the
     * ground's own edge is a contrast too); null if none crosses by more than a sliver.
     */
    private fun split(t: Rect, g: Rect): Pair<Rect, Rect>? {
        val sliver = 4f + EDGE
        return when {
            g.top > t.top + sliver && g.top < t.bottom - sliver -> Rect(t.left, g.top + EDGE, t.right, t.bottom) to Rect(t.left, t.top, t.right, g.top - EDGE)
            g.bottom > t.top + sliver && g.bottom < t.bottom - sliver -> Rect(t.left, t.top, t.right, g.bottom - EDGE) to Rect(t.left, g.bottom + EDGE, t.right, t.bottom)
            g.left > t.left + sliver && g.left < t.right - sliver -> Rect(g.left + EDGE, t.top, t.right, t.bottom) to Rect(t.left, t.top, g.left - EDGE, t.bottom)
            g.right > t.left + sliver && g.right < t.right - sliver -> Rect(t.left, t.top, g.right - EDGE, t.bottom) to Rect(g.right + EDGE, t.top, t.right, t.bottom)
            else -> null
        }
    }

    /** Whether [region] of [a] is not as it was in [b], the frame the move began on: words arrived there, not the leaving screen's. */
    private fun changed(a: Bitmap, b: Bitmap, region: Rect): Boolean {
        var diff = 0.0
        var n = 0
        for (y in region.top.toInt().coerceAtLeast(0) until region.bottom.toInt().coerceAtMost(a.height) step 2) {
            for (x in region.left.toInt().coerceAtLeast(0) until region.right.toInt().coerceAtMost(a.width) step 2) {
                diff += abs(lum(a.getPixel(x, y)) - lum(b.getPixel(x, y)))
                n++
            }
        }
        return n > 0 && diff / n > CHANGED
    }

    /** Whether [region] of [bitmap] shows words: a contrast in it a glyph would make. */
    private fun ink(bitmap: Bitmap, region: Rect): Boolean {
        val (lo, hi) = range(bitmap, region)
        return hi - lo > INK
    }

    /**
     * The moving ground goes from its first size to its last without turning back or standing still on the way:
     * its area only grows on a grow and only shrinks on a shrink, and no two frames running leave it where it was
     * while it is still short of its place (a ground that gives up for a frame, a dissolve that restarts, a hold
     * then a jump: decisions 203, 206 and round 3's review).
     */
    private fun groundTravels(frames: List<RunningMoves.Frame>): List<String> {
        val areas = frames.mapNotNull { f -> f.ground?.let { f.index to it.width * it.height } }
        if (areas.size < 3) return emptyList()
        val first = areas.first().second
        val last = areas.last().second
        val whole = abs(last - first)
        if (whole < 1f) return emptyList()
        val grows = last > first
        val out = mutableListOf<String>()
        var still = 0
        // The frames drawn at 0 before the spring starts are [startsAtOnce]'s to bound; the travel is checked after them.
        val off = areas.indexOfFirst { abs(it.second - first) > whole * TURN }.coerceAtLeast(1)
        for (i in off until areas.size) {
            val (index, area) = areas[i]
            val step = area - areas[i - 1].second
            if (if (grows) step < -whole * TURN else step > whole * TURN) out += "f$index: the moving ground turns back (${"%.0f".format(step)} px²)"
            if (abs(step) <= whole * TURN && abs(last - area) > whole * NEAR_SHARE) still++ else still = 0
            if (still > 1) out += "f$index: the moving ground stands still ${"%.0f".format(abs(last - area))} px² short of its place"
        }
        return out
    }

    private fun theme(dark: Boolean) = if (dark) "dark" else "light"

    /** The row title of [sale] on the shift, where a frame's words are laid out. */
    private fun rowOf(f: RunningMoves.Frame, sale: TillSale): Rect? {
        val title = rowTitle(sale)
        return f.words[Screen.Till]?.firstOrNull { it.first == title }?.second
    }

    /** The ground's last place on its way, as the move lands: inside [sale]'s row, a row's height, not the screen's. */
    private fun landsIn(frames: List<RunningMoves.Frame>, sale: TillSale): List<String> {
        val last = frames.lastOrNull { it.ground != null } ?: return listOf("the ground never moves")
        val g = last.ground!!
        val row = rowOf(frames.last(), sale) ?: return listOf("${sale.orderId} has no row on the shift at the end")
        val out = mutableListOf<String>()
        if (!g.contains(row.center)) out += "f${last.index}: the ground at $g is not in ${sale.orderId}'s row at $row"
        if (g.height > last.bitmap.height / 6f) out += "f${last.index}: the ground at $g is still screen-sized, not a row"
        return out
    }

    private fun noRise(frames: List<RunningMoves.Frame>): List<String> =
        frames.filter { f -> f.moves.any { it == Move.Rise } }.map { "f${it.index}: a rise took the move over" }

    /**
     * Back on a sale with others waiting unseen (decision 266, §12.1): the stage lands on the shift, the ground in the
     * left sale's row (the first in the tray), the shift drawn; nothing rises over it, before or after landing.
     */
    @Test fun backWithASaleWaitingLandsOnTheShiftLight() = backWithSaleWaiting(dark = false)

    @Test fun backWithASaleWaitingLandsOnTheShiftDark() = backWithSaleWaiting(dark = true)

    private fun backWithSaleWaiting(dark: Boolean) {
        moves.screenReader = true
        moves.shift = listOf(moves.sale, moves.sales[0], moves.sales[2])
        moves.start(Screen.Sale("a1"), dark)
        // z0 and b2 came while the cashier was on a1: the model has seen them, not the cashier.
        moves.model.salesChanged(moves.shift)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1", during = { if (it % 5 == 0) moves.model.salesChanged(moves.shift) })
        moves.model.salesChanged(moves.shift)
        repeat(2) { compose.mainClock.advanceTimeByFrame() }
        val failures = landsIn(frames, moves.sale) + noRise(frames)
        check("back-with-sale-waiting", dark, frames, Move.RowGrow, glyphs = false, grows = true, bars = failures +
            listOfNotNull(("the model ends on ${moves.model.screen.value}").takeIf { moves.model.screen.value != Screen.Till }))
        // TalkBack: focus is on the row the cashier came from.
        compose.onNodeWithText(rowTitle(moves.sale)).assertIsFocused()
    }

    /** With no screen reader, a Back moves no focus: a sighted cashier gets no grey, focused row. */
    @Test fun backWithoutAScreenReaderFocusesNoRow() {
        moves.start(Screen.Sale("a1"), dark = false)
        moves.run(Screen.Till, groundKey = "sale/a1")
        repeat(2) { compose.mainClock.advanceTimeByFrame() }
        compose.onNodeWithText(rowTitle(moves.sale)).assertIsNotFocused()
    }

    /**
     * Back on a sale the folded tray hides (the fourth live one, 260 a): the tray is open on the change's first frame,
     * so the row the ground shrinks into is there from the start; on no frame does the bare ground cover the window
     * (the lead's recordings: three frames of nothing but the ground before the sale was redrawn); the code is on its
     * card wherever the ground holds it; and the ground lands in its row.
     */
    @Test fun backIntoAFoldedRowOpensTheTrayLight() = backIntoFoldedRow(dark = false)

    @Test fun backIntoAFoldedRowOpensTheTrayDark() = backIntoFoldedRow(dark = true)

    private fun backIntoFoldedRow(dark: Boolean) {
        val c3 = testSale("c3", expiresAt = 1_600)
        moves.shift = listOf(moves.sales[0], moves.sales[2], c3, moves.sale)
        moves.start(Screen.Sale("a1"), dark)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1")
        val failures = landsIn(frames, moves.sale).toMutableList()
        val title = rowTitle(moves.sale)
        val first = frames.drop(1).firstOrNull { f -> f.words[Screen.Till].orEmpty().isNotEmpty() }
        if (first == null) failures += "the shift is never composed"
        else if (first.words[Screen.Till].orEmpty().none { it.first == title }) failures += "f${first.index}: the shift's first frame has no row for a1 (the tray still folded)"
        if (frames.last().words[Screen.Till].orEmpty().none { it.first == "Show fewer" }) failures += "the tray is not open at the end"
        failures += bareGround(frames)
        val card = frames.first().leavingCodes.firstOrNull { it.width > 20f }
        if (card != null) for (f in frames.drop(1).dropLast(RunningMoves.SETTLE_FRAMES)) {
            if (f.p >= 1f) continue
            val g = f.ground ?: continue
            val holds = g.left <= card.left + 1f && g.top <= card.top + 1f && g.right >= card.right - 1f && g.bottom >= card.bottom - 1f
            // The row is low on the screen, so its ground lets go of the card early: carried, it fades out to the shift's words.
            if (holds) dimsWithTheWords(f, frames, card, g, share = 1f - Motion.Full.crossfadeOnTrack)?.let { failures += it }
        }
        check("back-into-folded-row", dark, frames, Move.RowGrow, glyphs = false, grows = true, bars = failures)
    }

    /**
     * Back on a sale that rose by itself, its row two under the fold (the oldest of five waiting; the designer's back5
     * and back6, 260 a): no row was tapped, so no place is asked for, and the tray opens in place with the row low on
     * the screen. The ground leaves the card early as it shrinks down to it, and the code went with it: about two frames
     * of nothing but the ground before the shift's words came in. Now no frame is bare and, until the words begin, the
     * code is on its card on every frame, never off it: carried inside it, by the least move (decision 299), fading out
     * to the words as it goes (313).
     */
    @Test fun backAfterASelfRiseIntoARowUnderTheFoldIsNeverBareLight() = backAfterSelfRiseUnderTheFold(dark = false)

    @Test fun backAfterASelfRiseIntoARowUnderTheFoldIsNeverBareDark() = backAfterSelfRiseUnderTheFold(dark = true)

    private fun backAfterSelfRiseUnderTheFold(dark: Boolean) {
        moves.shift = listOf(moves.sales[0], testSale("c4", expiresAt = 1_600), moves.sales[2], testSale("c3", expiresAt = 1_600), moves.sale)
        moves.start(Screen.Till, dark)
        moves.run(Screen.Sale("a1"), byItself = true)
        repeat(4) { compose.mainClock.advanceTimeByFrame() }
        val frames = moves.run(Screen.Till, groundKey = "sale/a1")
        val failures = landsIn(frames, moves.sale).toMutableList()
        failures += bareGround(frames)
        val card = frames.first().leavingCodes.firstOrNull { it.width > 20f }
        if (card == null) failures += "the sale has no code before the Back"
        else {
            val words = 1f - Motion.Full.crossfadeOnTrack
            var letGo = false
            for (f in frames.drop(1).dropLast(RunningMoves.SETTLE_FRAMES)) {
                // Until the shift's words begin, the code is all there is of either screen: it is on its card on every frame.
                if (f.p >= words) break
                val g = f.ground ?: continue
                if (!(g.left <= card.left + 1f && g.top <= card.top + 1f && g.right >= card.right - 1f && g.bottom >= card.bottom - 1f)) letGo = true
                val on = carried(card, g)
                if (on == null) {
                    failures += "f${f.index}: the code is dropped before the shift's words begin (p ${"%.2f".format(f.p)}, ground $g, card $card)"
                    continue
                }
                val expected = Nav.through(f.p, isArriving = false, leaving = words, arriving = 1f)
                val shown = shows(f.bitmap, frames.first().bitmap, frames.last().bitmap, card.deflate(2f), on.left - card.left, on.top - card.top)
                if (abs(shown - expected) > DIM_SLACK) {
                    failures += "f${f.index}: the carried code at $on shows ${"%.2f".format(shown)}, not ${"%.2f".format(expected)} (p ${"%.2f".format(f.p)})"
                }
            }
            // The case this is about: the ground lets go of the card where it was laid out before the words begin.
            if (!letGo) failures += "the ground holds the card until the shift's words begin: not the designer's back5"
        }
        failures += codeOffItsCard(frames)
        report("back-self-rise-under-fold-${theme(dark)}", frames, failures)
    }

    /**
     * A leaving code on a ground [g] that holds its [card] dims with the screen's words (decision 313): on [f] it shows
     * as much as they do, within [DIM_SLACK] (its picture fitted against the move's two ends, as a word's is), and from
     * the words being gone on, no code is drawn there. Null when it does. A card its ground lets go of before the
     * arriving words begin is carried, and fades out to them instead ([share], 299).
     */
    private fun dimsWithTheWords(
        f: RunningMoves.Frame,
        frames: List<RunningMoves.Frame>,
        card: Rect,
        g: Rect,
        share: Float = Motion.Full.fadeOutOnTrack,
    ): String? {
        val expected = Nav.through(f.p, isArriving = false, leaving = share, arriving = 1f)
        if (expected <= 0f) {
            val (ink, white) = inkAndWhite(f.bitmap, card)
            return if (ink >= CODE_SHARE && white >= CODE_SHARE) "f${f.index}: the row's ground at $g holds the card at $card, and its code is still drawn after the words have gone (p ${"%.2f".format(f.p)})" else null
        }
        val shown = shows(f.bitmap, frames.first().bitmap, frames.last().bitmap, card)
        return if (abs(shown - expected) > DIM_SLACK) {
            "f${f.index}: the code on its card at $card shows ${"%.2f".format(shown)}, but the words ${"%.2f".format(expected)} (p ${"%.2f".format(f.p)})"
        } else {
            null
        }
    }

    /** [card] moved the least way into [g], as the leaving card carries its code (decision 299); null if it can't fit. */
    private fun carried(card: Rect, g: Rect): Rect? {
        if (card.width > g.width + 1f || card.height > g.height + 1f) return null
        val dx = if (card.left < g.left) g.left - card.left else if (card.right > g.right) g.right - card.right else 0f
        val dy = if (card.top < g.top) g.top - card.top else if (card.bottom > g.bottom) g.bottom - card.bottom else 0f
        return card.translate(dx, dy)
    }

    /**
     * Back from a sale with no row on the shift to land in (the filter hides it, or the list has not composed it): the
     * leaving sale is never replaced by the bare ground (260 a's other surface); the code is on its card, or gone with it,
     * faded out over the move, gone as the shift's words come in (decision 296; before, it stayed whole until landing).
     */
    @Test fun backWithNoRowToLandInNeverDrawsTheBareGround() {
        moves.shift = listOf(moves.sales[0], moves.sales[2])
        moves.start(Screen.Sale("a1"), dark = false)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1")
        // With no row its card fades out over the move, gone as the shift's words come in: never whole over the shift
        // and then cut, and never a bare frame between the two (296).
        report("back-no-row-light", frames, bareGround(frames) + codeOffItsCard(frames, from = 1f - Motion.Full.crossfadeOnTrack))
    }

    /**
     * Back from a sale tapped far down an open tray (the canvas's Main: 21 waiting, the last row tapped after scrolling
     * to the end; the lead's backfold, decision 296): the shift comes back scrolled where the cashier left it, so the
     * row is laid out, on screen, from the change's first frame and the ground lands in it; and on no frame is the
     * code drawn where the shrinking ground does not hold its card (backfold: the code over the rows for 300 ms, then
     * cut).
     */
    @Test fun backIntoARowScrolledFarDownTheTrayLandsInItLight() = backIntoScrolledRow(dark = false)

    @Test fun backIntoARowScrolledFarDownTheTrayLandsInItDark() = backIntoScrolledRow(dark = true)

    /**
     * The same Back after a back gesture peeked at the shift and was cancelled: the peek composes the shift (and asks
     * the list for the row) and then leaves it, so the Back that follows composes it afresh and must find the row's
     * place again (decision 297; before, the peek had used the place up and the Back came back at the list's top).
     */
    @Test fun backAfterACancelledPeekStillLandsInTheRowScrolledFarDown() = backIntoScrolledRow(dark = false, peekFirst = true)

    private fun backIntoScrolledRow(dark: Boolean, peekFirst: Boolean = false) {
        moves.shift = List(20) { testSale("w${20 - it}", expiresAt = 1_600) } + moves.sale
        moves.start(Screen.Till, dark)
        compose.mainClock.autoAdvance = true
        compose.onNodeWithContentDescription("Show 18 more waiting sales").performClick()
        compose.waitForIdle()
        compose.onNode(hasScrollToIndexAction()).performScrollToKey("end")
        compose.waitForIdle()
        val title = rowTitle(moves.sale)
        compose.onNodeWithText(title).performClick()
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        moves.run(Screen.Sale("a1"), groundKey = "sale/a1")
        repeat(4) { compose.mainClock.advanceTimeByFrame() }
        if (peekFirst) {
            val back = compose.activity.onBackPressedDispatcher
            compose.runOnUiThread { back.dispatchOnBackStarted(BackEventCompat(10f, 1000f, 0f, BackEventCompat.EDGE_LEFT)) }
            repeat(4) { compose.mainClock.advanceTimeByFrame() }
            compose.runOnUiThread { back.dispatchOnBackProgressed(BackEventCompat(10f, 1000f, 0.3f, BackEventCompat.EDGE_LEFT)) }
            repeat(4) { compose.mainClock.advanceTimeByFrame() }
            compose.runOnUiThread { back.dispatchOnBackCancelled() }
            compose.mainClock.advanceTimeBy(1_500)
        }
        val frames = moves.run(Screen.Till, groundKey = "sale/a1")
        val failures = landsIn(frames, moves.sale).toMutableList()
        val moving = frames.drop(1).filter { it.moves.any { m -> m == Move.RowGrow } && it.p < 1f }
        if (moving.isEmpty()) failures += "the Back never moves by RowGrow"
        val firstMoving = moving.firstOrNull()
        if (firstMoving != null && firstMoving.groundEnd == null) failures += "f${firstMoving.index}: the Back's first frame has no row to land in"
        val end = moving.lastOrNull()?.groundEnd
        val row = rowOf(frames.last(), moves.sale)
        val window = frames.last().bitmap
        if (end == null) {
            failures += "the ground has no end on the Back's last moving frame"
        } else {
            if (end.top < 0f || end.bottom > window.height) failures += "the ground lands at $end, off the ${window.height} px screen"
            if (row != null && !end.contains(row.center)) failures += "the ground lands at $end, not in a1's row at $row"
            // Every frame goes toward the same end, to half a pixel: the row does not move under the ground.
            for (f in moving) {
                val e = f.groundEnd ?: continue
                val off = maxOf(abs(e.left - end.left), abs(e.top - end.top), abs(e.right - end.right), abs(e.bottom - end.bottom))
                if (off > HALF_PIXEL) failures += "f${f.index}: the ground goes to $e, not $end where it lands"
            }
        }
        failures += codeOffItsCard(frames)
        report("back-scrolled-row-${theme(dark)}${if (peekFirst) "-peeked" else ""}", frames, failures)
    }

    /**
     * Frames on which the leaving sale's code shows where the moving ground does not hold its card: the pixels of the
     * card on the frame before the change that differ from the shift at rest and lie outside the ground on that frame,
     * read for how much of the code they still show (a quarter of its contrast or more), on frames whose ground does not
     * cover the whole card; the shift's words, wherever any frame lays them out, are left out. Only from [from] of the move: a code fading
     * out with no ground to go with is read once it is gone.
     */
    private fun codeOffItsCard(frames: List<RunningMoves.Frame>, from: Float = 0f): List<String> {
        val card = frames.first().leavingCodes.firstOrNull { it.width > 20f } ?: return listOf("the sale has no code before the Back")
        val before = frames.first().bitmap
        val after = frames.last().bitmap
        val xs = card.left.toInt().coerceAtLeast(0) until card.right.toInt().coerceAtMost(before.width)
        val ys = card.top.toInt().coerceAtLeast(0) until card.bottom.toInt().coerceAtMost(before.height)
        // Not where the shift's own words are: they fade in through the move, toward neither end.
        val words = frames.flatMap { it.words[Screen.Till].orEmpty().map { w -> w.second } }
        val mask = ArrayList<Int>()
        for (y in ys step 2) for (x in xs step 2) {
            if (words.any { it.contains(androidx.compose.ui.geometry.Offset(x.toFloat(), y.toFloat())) }) continue
            if (abs(lum(before.getPixel(x, y)) - lum(after.getPixel(x, y))) > INK) mask += y * before.width + x
        }
        if (mask.isEmpty()) return listOf("the code's card at $card looks like the shift at rest")
        return frames.drop(1).mapNotNull { f ->
            if (f.p < from) return@mapNotNull null
            val g = f.ground
            if (g != null && g.left <= card.left + 1f && g.top <= card.top + 1f && g.right >= card.right - 1f && g.bottom >= card.bottom - 1f) return@mapNotNull null
            var shown = 0
            var off = 0
            for (i in mask) {
                val x = i % before.width
                val y = i / before.width
                // Inside the ground the code is on its card, wherever the card carries it (decision 299): only what
                // shows outside it is a code with no card round it.
                if (g != null && g.contains(androidx.compose.ui.geometry.Offset(x.toFloat(), y.toFloat()))) continue
                off++
                val a = lum(before.getPixel(x, y))
                val b = lum(after.getPixel(x, y))
                if ((lum(f.bitmap.getPixel(x, y)) - b) / (a - b) > CODE_LEFT) shown++
            }
            if (off == 0) return@mapNotNull null
            val share = shown.toFloat() / off
            "f${f.index}: ${"%.2f".format(share)} of the code is drawn with no card round it (ground $g, p ${"%.2f".format(f.p)})".takeIf { share > CODE_OFF_SHARE }
        }
    }

    /** Under Still the Back into a folded row is one cut onto the open tray: never the folded tray first, then a jump open. */
    @Test fun backIntoAFoldedRowUnderStillCutsToTheOpenTray() {
        val c3 = testSale("c3", expiresAt = 1_600)
        moves.shift = listOf(moves.sales[0], moves.sales[2], c3, moves.sale)
        moves.start(Screen.Sale("a1"), dark = false, Motion.Still)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1")
        val title = rowTitle(moves.sale)
        val shifts = frames.drop(1).filter { f -> f.words[Screen.Till].orEmpty().isNotEmpty() }
        assertTrue(shifts.isNotEmpty(), "the shift is composed")
        for (f in shifts) {
            val words = f.words[Screen.Till].orEmpty()
            assertTrue(words.any { it.first == title } && words.any { it.first == "Show fewer" }, "f${f.index}: the tray is open from the cut's first frame: ${words.map { it.first }}")
        }
    }

    /**
     * Frames on which the screen's bare ground covers the window (more than [BARE_SHARE] of a grid of points are its
     * colour, read from the corner of the frame before the change): no code, no card, no words.
     */
    private fun bareGround(frames: List<RunningMoves.Frame>): List<String> {
        val before = frames.first().bitmap
        val ground = before.getPixel(1, before.height - 2)
        return frames.drop(1).mapNotNull { f ->
            val b = f.bitmap
            var same = 0
            var all = 0
            for (y in 0 until GRID) for (x in 0 until GRID) {
                val c = b.getPixel(x * (b.width - 1) / (GRID - 1), y * (b.height - 1) / (GRID - 1))
                all++
                if (abs(lum(c) - lum(ground)) < 2f && abs((c shr 16 and 0xff) - (ground shr 16 and 0xff)) < 3) same++
            }
            "f${f.index}: the bare ground covers ${100 * same / all}% of the window".takeIf { same.toFloat() / all > BARE_SHARE }
        }
    }

    /**
     * A sale arriving while Back is still landing goes into the tray under the moving card and does not take the move
     * over; once the shift rests, the next new sale rises out of it as before (§12.6 item 4).
     */
    @Test fun aSaleArrivingMidBackWaitsInTheTrayAndTheNextRises() {
        moves.shift = listOf(moves.sale, moves.sales[0])
        moves.start(Screen.Sale("a1"), dark = false)
        moves.model.salesChanged(moves.shift)
        val n9 = testSale("n9", expiresAt = 1_600)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1", during = {
            if (it == 8) {
                moves.shift = listOf(n9) + moves.shift
                moves.model.salesChanged(moves.shift)
            }
        })
        val failures = (landsIn(frames, moves.sale) + noRise(frames)).toMutableList()
        if (moves.model.screen.value != Screen.Till) failures += "n9 came up over the Back: the model is on ${moves.model.screen.value}"
        if (rowOf(frames.last(), n9) == null) failures += "n9 has no row in the tray"
        moves.model.salesChanged(moves.shift)
        if (moves.model.screen.value != Screen.Till) failures += "n9 came up after the Back landed"
        check("sale-mid-back", dark = false, frames, Move.RowGrow, glyphs = false, grows = true, bars = failures)
        // The shift at rest: a new sale rises out of it.
        val n10 = testSale("n10", expiresAt = 1_600)
        moves.shift = listOf(n10) + moves.shift
        moves.model.salesChanged(moves.shift)
        assertTrue(moves.model.screen.value == Screen.Sale("n10") && moves.model.cameUpByItself, "a sale arriving on the resting shift rises: ${moves.model.screen.value}")
        val rise = moves.run(Screen.Sale("n10"), byItself = true)
        assertTrue(rise.drop(1).dropLast(RunningMoves.SETTLE_FRAMES).any { f -> f.moves.any { it == Move.Rise } }, "it rises")
    }

    /**
     * A rise under a slow frame (the after report's aSaleRisesDark flake, docs/reports/motion.md): a 120 ms frame
     * mid-rise, then the frames after it, all read as the rise that was asked; the held move starts no new generation
     * for the change it is in (decision 206).
     */
    @Test fun aSaleRisesThroughASlowFrameLight() = slowRise(dark = false) { if (it == 4 || it == 9) compose.mainClock.advanceTimeBy(SLOW_FRAME_MS) }

    @Test fun aSaleRisesThroughASlowFrameDark() = slowRise(dark = true) { if (it == 4 || it == 9) compose.mainClock.advanceTimeBy(SLOW_FRAME_MS) }

    /** A sales update mid-rise that turns the model's came-up-by-itself off (another look, the same screen) leaves the rise a rise. */
    @Test fun aSaleRisesThroughASalesUpdate() = slowRise(dark = false) {
        if (it == 4) moves.model.go(Screen.Sale("a1"), byItself = false)
        if (it == 5) compose.mainClock.advanceTimeBy(SLOW_FRAME_MS)
    }

    /**
     * A rise sent on the very frame a Back lands (the shift's rest reported after the next change has begun), a sales
     * update turning the model's came-up-by-itself off mid-rise, and a slow frame: still one rise, one generation.
     */
    @Test fun aSaleRisesFromTheFrameABackLands() {
        moves.start(Screen.Sale("b2"), dark = false)
        moves.model.go(Screen.Till)
        var n = 0
        while (!(moves.model.stage.currentState == Screen.Till && moves.model.stage.targetState == Screen.Till) && n++ < 90) compose.mainClock.advanceTimeByFrame()
        val frames = moves.run(Screen.Sale("a1"), byItself = true, during = {
            if (it == 2) moves.model.go(Screen.Sale("a1"), byItself = false)
            if (it == 3) compose.mainClock.advanceTimeBy(SLOW_FRAME_MS)
        })
        val wrong = frames.filter { f -> f.moves.any { it != null && it != Move.Rise } }.map { f -> "f${f.index}: ${f.moves}" }
        assertTrue(wrong.isEmpty(), "every frame of the rise reads it as the rise: $wrong")
    }

    private fun slowRise(dark: Boolean, during: (Int) -> Unit) {
        moves.start(Screen.Till, dark)
        val frames = moves.run(Screen.Sale("a1"), byItself = true, during = during)
        val wrong = frames.filter { f -> f.moves.any { it != null && it != Move.Rise } }.map { f -> "f${f.index}: ${f.moves}" }
        assertTrue(wrong.isEmpty(), "every frame of the rise reads it as the rise: $wrong")
        assertTrue(moves.model.stage.currentState == Screen.Sale("a1") && moves.model.stage.targetState == Screen.Sale("a1"), "and it lands")
    }

    /**
     * The paid step-back after the dwell is the Back (decision 266, §12.2): paid in place on the sale screen, then after
     * the dwell on the test clock the model leaves the page as the dwell's job does, and the move is the row's grow
     * reversed with the sale's course, its ground landing in the row at the top of Earlier (20 dp top corners); a Back
     * on the same paid page then runs the same frames, ground for ground, within half a pixel.
     */
    @Test fun thePaidStepBackIsTheBackLight() = paidStepBackIsBack(dark = false)

    @Test fun thePaidStepBackIsTheBackDark() = paidStepBackIsBack(dark = true)

    private fun paidStepBackIsBack(dark: Boolean) {
        val earlier = testSale("p9", SalePhase.Paid, paidAt = 900)
        moves.shift = listOf(moves.sales[0], moves.sale, moves.sales[2], earlier)
        moves.start(Screen.Sale("a1"), dark)
        val paid = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100)
        moves.runFor(FOLD_MS) {
            moves.shown = paid
            moves.shift = listOf(moves.sales[0], paid, moves.sales[2], earlier)
        }
        compose.mainClock.advanceTimeBy(Motion.Dwell.PAID_MS - FOLD_MS)
        val dwell = moves.run(Screen.Till, groundKey = "sale/a1")
        val failures = landsIn(dwell, paid).toMutableList()
        val row = rowOf(dwell.last(), paid)
        val tray = rowOf(dwell.last(), moves.sales[0])
        val under = rowOf(dwell.last(), earlier)
        if (row != null && (tray == null || under == null || row.top < tray.bottom || row.bottom > under.top)) failures += "the paid row at $row is not at the top of Earlier (tray row $tray, the one paid before $under)"
        if (dwell.none { it.ground != null }) failures += "the step-back draws no sale/a1 course"
        check("paid-step-back-dwell", dark, dwell, Move.RowGrow, glyphs = false, grows = true, bars = failures)

        // The same page, opened again and left by Back: the same move, the same ends, frame for frame.
        moves.run(Screen.Sale("a1"), groundKey = "sale/a1")
        val back = moves.run(Screen.Till, groundKey = "sale/a1")
        val a = dwell.mapNotNull { it.ground }
        val b = back.mapNotNull { it.ground }
        assertTrue(a.size == b.size, "the step-back moves its ground on ${a.size} frames, the Back on ${b.size}")
        for ((n, pair) in a.zip(b).withIndex()) {
            val (x, y) = pair
            val off = maxOf(abs(x.left - y.left), abs(x.top - y.top), abs(x.right - y.right), abs(x.bottom - y.bottom))
            assertTrue(off <= HALF_PIXEL, "frame $n of the move: the step-back's ground at $x, the Back's at $y")
        }
    }

    /**
     * The paid step-back from a sale opened from the first tray row with the list at its top (the designer's stepback,
     * decision 300): the paid sale has moved from the tray to the top of Earlier, and holding its row where it was
     * tapped scrolled the whole list about 400 px, the title, the chips and the tray's first waiting sale off the top.
     * Now the list comes back where it was (its top), the tray's waiting sales in view, and the ground lands in the
     * paid row under Earlier. A shift long enough to scroll, as the designer's (18 paid).
     */
    @Test fun thePaidStepBackFromTheFirstTrayRowKeepsTheListAtItsTop() {
        val earlier = List(18) { testSale("p${it + 10}", SalePhase.Paid, paidAt = 900L - it) }
        val waiting = listOf(moves.sales[0], moves.sales[2], testSale("c3", expiresAt = 1_600), testSale("c4", expiresAt = 1_600))
        moves.shift = listOf(moves.sale) + waiting + earlier
        moves.start(Screen.Till, dark = false)
        compose.mainClock.autoAdvance = true
        compose.onNodeWithText(rowTitle(moves.sale)).performClick()
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        val open = moves.run(Screen.Sale("a1"), groundKey = "sale/a1")
        fun titleOf(f: RunningMoves.Frame) = f.words[Screen.Till]?.firstOrNull { it.first == "This shift" }?.second
        val top = titleOf(open.first()) ?: error("no title on the shift before the tap")
        val paid = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100)
        moves.runFor(FOLD_MS) {
            moves.shown = paid
            moves.shift = waiting + paid + earlier
        }
        compose.mainClock.advanceTimeBy(Motion.Dwell.PAID_MS - FOLD_MS)
        val back = moves.run(Screen.Till, groundKey = "sale/a1")
        val failures = landsIn(back, paid).toMutableList()
        val title = titleOf(back.last())
        if (title == null || abs(title.top - top.top) > 1f) failures += "the list comes back scrolled: the title at $title, at $top before the tap"
        val first = rowOf(back.last(), waiting[0])
        val row = rowOf(back.last(), paid)
        val window = back.last().bitmap.height
        if (first == null || first.top < 0f) failures += "the tray's first waiting sale is off the screen: $first"
        if (row == null || row.bottom > window) failures += "the paid row is not on the screen: $row"
        else if (first != null && row.top < first.bottom) failures += "the paid row at $row is above the tray's first row at $first"
        report("paid-step-back-from-top", back, failures)
    }

    /** Under Still the step-back is the Back's 120 ms crossfade: no surface moves. */
    @Test fun thePaidStepBackUnderStillIsACrossfade() {
        moves.shift = listOf(moves.sales[0], moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100), moves.sales[2])
        moves.shown = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100)
        moves.start(Screen.Sale("a1"), dark = false, Motion.Still)
        val frames = moves.run(Screen.Till, groundKey = "sale/a1")
        assertTrue(frames.all { it.ground == null }, "no ground moves under Still")
        val landed = frames.indexOfFirst { f -> f.shown[Screen.Sale("a1")]?.let { it == 0f } ?: true }
        assertTrue(landed in 1..(STILL_FRAMES + 2), "the crossfade is done in ${STILL_FRAMES} frames or so: $landed")
    }

    /**
     * The sale screen's paid fold draws one glyph in the card at a time (260 b, decision 252): the tick is not drawn
     * while the card is closing over its first share, only over the fold's last [Motion.crossfadeOnTrack], as the code
     * screen's hero comes in; the code is gone by the fold's first [Motion.fadeOutOnTrack], and no module of it is ever
     * under the tick; at rest, the tick and no code.
     */
    @Test fun thePaidFoldDrawsTheTickOnlyInItsLastShareLight() = tickInLastShare(dark = false)

    @Test fun thePaidFoldDrawsTheTickOnlyInItsLastShareDark() = tickInLastShare(dark = true)

    private fun tickInLastShare(dark: Boolean) {
        moves.start(Screen.Sale("a1"), dark)
        val frames = moves.runFor(FOLD_MS) { moves.shown = moves.sale.copy(phase = SalePhase.Paid, paidAt = 1_100) }
        val card = frames.first().codes.first()
        val rest = frames.last().hero ?: error("no hero at rest")
        val failures = mutableListOf<String>()
        val early = 1f - Motion.Full.crossfadeOnTrack - SHARE_SLACK
        val gone = Motion.Full.fadeOutOnTrack + SHARE_SLACK
        for (f in frames.drop(1)) {
            val h = f.hero ?: continue
            // How far the card has closed toward the hero, by its width: the fold's own progress.
            val closed = ((card.width - h.width) / (card.width - rest.width)).coerceIn(0f, 1f)
            val tick = tickShare(f.bitmap, h) > TICK_SHARE
            if (closed < early && tick) failures += "f${f.index}: the tick is drawn with the card ${"%.2f".format(closed)} closed (its last share starts at ${"%.2f".format(1f - Motion.Full.crossfadeOnTrack)})"
            // One glyph at a time: wherever the tick is drawn, no module of the code is, however faint (the lead's
            // stepback f6: modules at 0.2 under the tick at 0.4).
            val modules = moduleShare(f.bitmap, h)
            if (tick && modules > NEAR_SHARE) failures += "f${f.index}: the tick is drawn over the code's modules (${"%.3f".format(modules)} of the card, ${"%.2f".format(closed)} closed)"
            // And the code goes on the fold's own progress, by its first share closed, not on a clock of its own that a
            // slow frame or a late start can leave behind the card (decision 290).
            if (closed > gone && modules > NEAR_SHARE) failures += "f${f.index}: the code's modules are still drawn with the card ${"%.2f".format(closed)} closed (gone by ${"%.2f".format(Motion.Full.fadeOutOnTrack)})"
        }
        val last = frames.last()
        if (tickShare(last.bitmap, rest) <= TICK_SHARE) failures += "at rest the hero shows no tick"
        if (last.codes.any { it.width > 20f }) failures += "at rest a code is still laid out"
        report("tick-last-share-${theme(dark)}", frames, failures)
    }

    /**
     * How much of the middle of [r] in [b] (a square inside the hero's circle) shows a code's modules at any alpha:
     * the share of its lines that cross from dark to light and back [MODULE_EDGES] times or more, a step of
     * [MODULE_STEP] or more apart. A code is a pattern along every line; the hero's ground coming in is one tone, and
     * its tick crosses a line twice.
     */
    private fun moduleShare(b: Bitmap, r: Rect): Float {
        val half = r.width * INSIDE_CIRCLE
        val xs = (r.center.x - half).toInt().coerceAtLeast(0) until (r.center.x + half).toInt().coerceAtMost(b.width)
        val ys = (r.center.y - half).toInt().coerceAtLeast(0) until (r.center.y + half).toInt().coerceAtMost(b.height)
        var lines = 0
        var patterned = 0
        for (y in ys) {
            var light = 0f
            var dark = 255f
            for (x in xs) lum(b.getPixel(x, y)).let { light = maxOf(light, it); dark = min(dark, it) }
            lines++
            if (light - dark < MODULE_STEP) continue
            val mid = (light + dark) / 2f
            var edges = 0
            var was: Boolean? = null
            for (x in xs) {
                val isDark = lum(b.getPixel(x, y)) < mid
                if (was != null && isDark != was) edges++
                was = isDark
            }
            if (edges >= MODULE_EDGES) patterned++
        }
        return if (lines == 0) 0f else patterned.toFloat() / lines
    }

    /** How much of [r] in [b] is the paid tick's green (a code is only ink and white; the hero's ground is a pale green). */
    private fun tickShare(b: Bitmap, r: Rect): Float {
        var n = 0
        var green = 0
        for (y in r.top.toInt().coerceAtLeast(0) until r.bottom.toInt().coerceAtMost(b.height)) {
            for (x in r.left.toInt().coerceAtLeast(0) until r.right.toInt().coerceAtMost(b.width)) {
                val c = b.getPixel(x, y)
                val red = c shr 16 and 0xff
                val g = c shr 8 and 0xff
                val blue = c and 0xff
                if (g - red > TICK_GREEN && g - blue > TICK_GREEN / 2) green++
                n++
            }
        }
        return if (n == 0) 0f else green.toFloat() / n
    }

    /**
     * Nothing of the shift recomposes once a frame while a Back with sales waiting runs, or while the paid step-back
     * does (performance rule 10): the rows, the tray's header and its fold compose as the shift comes in, and the shift
     * itself once more as the stage lands (where focus goes), never per frame of the move.
     */
    @Test fun aBackIntoTheTrayRecomposesNothingPerFrame() {
        moves.shift = listOf(moves.sale, moves.sales[0], moves.sales[2], testSale("c3", expiresAt = 1_600))
        moves.start(Screen.Sale("a1"), dark = false)
        Recompositions().use { counts ->
            var composed = 0
            val frames = moves.run(Screen.Till, groundKey = "sale/a1", during = {
                if (it == PRIMED + 2) {
                    composed = counts.count("com.monokulo.till.ui.SaleRow")
                    counts.reset()
                }
            })
            val moving = frames.count { it.ground != null }
            assertTrue(composed > 0, "the tracer sees the rows compose as the shift comes in")
            for (name in listOf("SaleRow", "TrayHeader", "FoldRow", "RowBadge")) {
                assertEquals(0, counts.count("com.monokulo.till.ui.$name"), "$name recomposed over the $moving frames of the move")
            }
            assertTrue(counts.count("com.monokulo.till.ui.ShiftScreen") <= 1, "the shift recomposed ${counts.count("com.monokulo.till.ui.ShiftScreen")} times over $moving frames")
        }
    }

    private companion object {
        /** How close to rest the hero's spring is done: a pixel or two of tail. */
        const val NEAR = 4f

        /** Half the side of the square read inside a circle, as a share of its width (under 1/(2√2)). */
        const val INSIDE_CIRCLE = 0.33f

        /** A module faded to a tenth over the white is this much darker than the white between modules. */
        const val MODULE_STEP = 12f

        /** A line through a code's middle crosses more modules than this; through the hero and its tick, at most four. */
        const val MODULE_EDGES = 8

        /** A window that is more than this share bare ground is drawing nothing on it. */
        const val BARE_SHARE = 0.98f

        /** The points across and down a frame the bare-ground check reads. */
        const val GRID = 40

        /** A region whose mean luminance moved this much from the move's first frame has something new in it. */
        const val CHANGED = 6f

        /** The paid fold and the hero's settling, with room. */
        const val FOLD_MS = 900L

        /** Neither a module nor the white round it: a code through a fade, over either theme's ground. */
        val GREY = 80f..200f

        /** More of a code's square than this grey is a code drawn through a fade; its edges' anti-aliasing is far less. */
        const val GREY_SHARE = 0.25f

        /** The white round a code, and its ground: nothing darker. */
        const val WHITE = 235f

        /** A glyph's contrast against what it is drawn on. */
        const val INK = 60f

        /** A step of the ground's area this small, as a share of its whole change, is no step: a spring's rounding. */
        const val TURN = 0.002f

        /** Within this share of its whole change the ground is at its place: the spring's tail. */
        const val NEAR_SHARE = 0.02f

        /** How far off a ground's edge a glyph's ink is looked for: the ground's own edge is a contrast too. */
        const val EDGE = 3f

        /** Darker than the dark theme's paid ground (51) over the card's white or alone: the screen's ground (27) showing through. */
        const val SCREEN_GROUND_DARK = 48f

        /** Frames a change is drawn at 0 before its spring starts: the arriving screen's first composition (decision 235). */
        const val PRIMED = 2

        /** A margin may round a pixel of the half-size frame below the smaller rest margin. */
        const val MARGIN_SLACK = 1f

        /** A code's module. */
        const val MODULE = 60f

        /** A pixel showing this much of the code's contrast against the shift is the code, drawn. */
        const val CODE_LEFT = 0.25f

        /** More of the card than this showing the code is the code drawn: a row's glyphs passing are far less. */
        const val CODE_OFF_SHARE = 0.05f

        /** How far a leaving code's share may be from the words' on a frame (decision 313): pixels, not alpha, are read. */
        const val DIM_SLACK = 0.2f

        /** A region with this much of both modules and white has a code in it. */
        const val CODE_SHARE = 0.05f

        /** More of a strip's pixels white than this, on a dark ground, is the code card's foot drawn there. */
        const val FOOT_SHARE = 0.5f

        /** A text run with this much spread of luminance in its region has ink in it to be read. */
        const val INK_SPREAD = 12f

        /** Above half shown: readable (decision 188). */
        const val HALF = 0.5f

        /** More of the hero than this is the tick's green: its stroke is a few percent of the circle. */
        const val TICK_SHARE = 0.004f

        /** How much greener than red a pixel is to be the paid hero's, even faint: no code or white card is green at all. */
        const val TICK_GREEN = 16

        /** The card's width reads the fold's progress to a frame or so. */
        const val SHARE_SLACK = 0.05f

        /** Half a pixel of the full-size window, in the half-size frames. */
        const val HALF_PIXEL = 0.5f

        /** 120 ms of 16 ms frames. */
        const val STILL_FRAMES = 8

        /** A frame a loaded host can draw: six of the test clock's. */
        const val SLOW_FRAME_MS = 120L

        /** Where the stage's spring is after one 16 ms frame from 0, with room: not a quarter of the way. */
        const val FIRST_STEP = 0.15f
    }
}
