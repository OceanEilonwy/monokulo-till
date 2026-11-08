package com.monokulo.till

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performScrollToKey
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.hasScrollToIndexAction
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.ShiftScreen
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The waiting tray (A4, `docs/motion.md` §12.3) on a real frame clock: the
 * fold after three rows and its words, opening in place, the filters, what
 * TalkBack is told, the fold kept across a rotation, and the rows moving
 * between the tray and Earlier under the list's own item specs.
 */
class TrayTest {
    @get:Rule
    val compose = createComposeRule()

    private val now = mutableLongStateOf(1_000)
    private val recompositions = Recompositions()

    @After
    fun tearDown() = recompositions.close()

    /** [n] live sales, newest first ("w<n>" to "w1"), then a paid one and an expired one. */
    private fun busy(n: Int) = List(n) { testSale("w${n - it}", expiresAt = 1_600) } +
        listOf(testSale("p1", SalePhase.Paid, paidAt = 900), testSale("e1", SalePhase.Expired, expiresAt = 900))

    private fun title(sale: TillSale) = rowTitle(sale)

    private fun shows(sale: TillSale) = compose.onAllNodes(SemanticsMatcher("row ${sale.orderId}") { n ->
        n.config.getOrNull(SemanticsProperties.Text)?.any { it.text == title(sale) } == true
    }, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun top(sale: TillSale): Float =
        compose.onNodeWithText(title(sale), useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.top

    private fun exists(text: String, substring: Boolean = false) =
        compose.onAllNodes(SemanticsMatcher("says $text") { n ->
            val said = n.config.getOrNull(SemanticsProperties.ContentDescription).orEmpty() + n.config.getOrNull(SemanticsProperties.Text).orEmpty().map { it.text }
            said.any { if (substring) it.contains(text) else it == text }
        }, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    /** A fold row, by what TalkBack says for it or by its "+N" pill. */
    private fun folds() = exists("more waiting sale", substring = true) ||
        compose.onAllNodes(SemanticsMatcher("a +N pill") { n ->
            n.config.getOrNull(SemanticsProperties.Text).orEmpty().any { it.text.matches(Regex("\\+\\d+")) }
        }, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()

    private fun show(sales: List<TillSale>, motion: Motion = Motion.Full) {
        compose.setContent { Till(motion) { ShiftScreen(rememberShift(sales, now.longValue), noActions) } }
        compose.waitForIdle()
    }

    @Test
    fun fourWaitingFoldAfterThreeAndOpenInPlace() {
        val sales = busy(4)
        show(sales)
        assertEquals(listOf(true, true, true, false), sales.take(4).map(::shows), "three rows, the fourth folded away")
        assertTrue(exists("+1") && folds())
        assertTrue(shows(sales[4]) && exists(EARLIER), "the paid sale under Earlier")
        compose.onNodeWithContentDescription("Show 1 more waiting sale").performClick()
        compose.waitForIdle()
        assertTrue(sales.take(4).all(::shows), "open, every live sale is a row")
        assertTrue(exists("Show fewer"))
        assertTrue(top(sales[3]) < top(sales[4]), "the fourth opens in place, in the tray above Earlier")
        compose.onNodeWithContentDescription("Show fewer").performClick()
        compose.waitForIdle()
        assertTrue(!shows(sales[3]) && exists("+1"), "and folds again")
    }

    @Test
    fun twentyOneWaitingCountAndFoldEighteen() {
        show(busy(21))
        assertTrue(exists("Waiting for payment, 21 sales"))
        assertTrue(exists("+18"))
        compose.onNodeWithContentDescription("Show 18 more waiting sales").performClick()
        compose.waitForIdle()
        assertTrue(exists("Show fewer"), "the fold row's words change at once; the rows open under it")
    }

    @Test
    fun twoWaitingHaveNoFold() {
        val sales = busy(2)
        show(sales)
        assertTrue(sales.take(2).all(::shows))
        assertTrue(!folds() && !exists("Show fewer"))
        assertTrue(exists("Waiting for payment, 2 sales") && exists(EARLIER))
    }

    @Test
    fun noLiveSaleNoTrayAndNoHeaders() {
        show(busy(0))
        assertTrue(!exists(TRAY_HEADER, substring = true) && !exists(EARLIER) && !folds())
        assertTrue(shows(testSale("p1", SalePhase.Paid, paidAt = 900)), "the list reads as it always has")
    }

    @Test
    fun needsActionShowsTheTrayUnfolded() {
        val sales = busy(5)
        show(sales)
        compose.onNodeWithText("Needs action", useUnmergedTree = true).performClick()
        compose.waitForIdle()
        assertTrue(sales.take(5).all(::shows), "every live sale")
        assertTrue(!folds() && !exists("Show fewer"), "no fold row")
        assertTrue(!shows(sales[5]) && shows(sales[6]), "then what ended unpaid, not the paid sale")
    }

    @Test
    fun theFoldRowAndTheHeaderSayWhatTheyAre() {
        show(busy(4))
        compose.onNodeWithContentDescription("Waiting for payment, 4 sales").assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Heading))
        compose.onNodeWithText(EARLIER).assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Heading))
        val fold = compose.onNodeWithContentDescription("Show 1 more waiting sale")
        fold.assert(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button))
        fold.assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Collapsed"))
        fold.performClick()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Show fewer").assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Expanded"))
    }

    @Test
    fun theFoldIsKeptAcrossARotation() {
        val restore = StateRestorationTester(compose)
        val sales = busy(4)
        restore.setContent { Till { ShiftScreen(rememberShift(sales, now.longValue), noActions) } }
        compose.onNodeWithContentDescription("Show 1 more waiting sale").performClick()
        compose.waitForIdle()
        restore.emulateSavedInstanceStateRestore()
        compose.waitForIdle()
        assertTrue(shows(sales[3]) && exists("Show fewer"), "still open")
    }

    /** A sale paid: its row is the same item, moving from the tray to the top of Earlier while the rows round it settle. */
    @Test
    fun aSalePaidMovesFromTheTrayToTheTopOfEarlier() {
        val start = busy(3)
        var sales by mutableStateOf(start)
        compose.setContent { Till { ShiftScreen(rememberShift(sales, now.longValue), noActions) } }
        compose.waitForIdle()
        val w2 = start[1]
        val from = top(w2)
        compose.mainClock.autoAdvance = false
        val paid = testSale("w2", SalePhase.Paid, paidAt = 1_000)
        // The till keeps the shift newest first by when each sale started: w2 stays where it was in the list.
        sales = listOf(start[0], paid, start[2]) + start.drop(3)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(80)
        val during = top(paid)
        compose.mainClock.advanceTimeBy(800)
        val to = top(paid)
        assertTrue(to > from, "it lands below the tray: $from to $to")
        assertTrue(during > from && during < to, "mid-move it is between: $from < $during < $to")
        assertTrue(to < top(start[3]), "at the top of Earlier, over the sale paid before it")
        assertTrue(top(start[2]) < from + 1f, "the row under it in the tray moved up into its place")
    }

    /** The last live sale paid: the tray's header goes and Earlier's with it, the list reading as it always has. */
    @Test
    fun theLastLiveSaleGoneTakesTheHeadersWithIt() {
        val start = busy(1)
        var sales by mutableStateOf(start)
        compose.setContent { Till { ShiftScreen(rememberShift(sales, now.longValue), noActions) } }
        compose.waitForIdle()
        assertTrue(exists("Waiting for payment, 1 sale"))
        compose.mainClock.autoAdvance = false
        sales = listOf(testSale("w1", SalePhase.Paid, paidAt = 1_000)) + start.drop(1)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(1_000)
        assertTrue(!exists(TRAY_HEADER, substring = true) && !exists(EARLIER))
    }

    /** A new sale on a showing shift lands at the top of the tray; folded, the third row goes into the fold. */
    @Test
    fun aNewSaleLandsAtTheTopOfTheTray() {
        val start = busy(3)
        var sales by mutableStateOf(start)
        compose.setContent { Till { ShiftScreen(rememberShift(sales, now.longValue), noActions) } }
        compose.waitForIdle()
        val first = top(start[0])
        compose.mainClock.autoAdvance = false
        val new = testSale("w4", expiresAt = 1_600)
        sales = listOf(new) + start
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(1_000)
        assertEquals(first, top(new), 0.5f, "at the tray's top")
        assertTrue(!shows(start[2]) && exists("+1"), "the third row moved into the fold")
        assertTrue(exists("Waiting for payment, 4 sales"))
    }

    /** Opening and folding the tray recomposes no row that was already there: the rows entering compose, nothing per frame. */
    @Test
    fun openingTheTrayRecomposesOnlyTheRowsComingIn() {
        show(busy(6))
        compose.mainClock.autoAdvance = false
        recompositions.reset()
        compose.onNodeWithContentDescription("Show 3 more waiting sales").performClick()
        compose.mainClock.advanceTimeByFrame()
        val entering = recompositions.count("com.monokulo.till.ui.SaleRow")
        repeat(10) { compose.mainClock.advanceTimeByFrame() }
        assertTrue(entering in 1..3, "only the three rows coming in compose, and the tracer sees them: $entering")
        assertEquals(entering, recompositions.count("com.monokulo.till.ui.SaleRow"), "no row recomposes over the next ten frames")
    }

    @Test
    fun underStillTheTrayOpensWithASnap() {
        val sales = busy(4)
        show(sales, Motion.Still)
        compose.mainClock.autoAdvance = false
        compose.onNodeWithContentDescription("Show 1 more waiting sale").performClick()
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertTrue(shows(sales[3]) && exists("Show fewer"))
    }

    /** The stack pulled down past a third of its reach and let go slowly opens the tray (the fixes canvas's board 4). */
    @Test
    fun pullingTheStackDownOpensTheTray() {
        val sales = busy(4)
        show(sales)
        val half = with(compose.density) { (FoldStack.REACH_DP / 2f).dp.toPx() }
        compose.onNodeWithContentDescription("Show 1 more waiting sale").performTouchInput {
            swipeDown(centerY, centerY + viewConfiguration.touchSlop + half, durationMillis = 1_000)
        }
        compose.waitForIdle()
        assertTrue(shows(sales[3]) && exists("Show fewer"), "open")
    }

    /** A short pull let go settles back: still folded, the stack its rest height again. */
    @Test
    fun aShortPullSettlesBack() {
        show(busy(4))
        val fold = compose.onNodeWithContentDescription("Show 1 more waiting sale")
        val rest = fold.fetchSemanticsNode().boundsInRoot.height
        val short = with(compose.density) { 12.dp.toPx() }
        fold.performTouchInput { swipeDown(centerY, centerY + viewConfiguration.touchSlop + short, durationMillis = 1_000) }
        compose.waitForIdle()
        assertTrue(!shows(busy(4)[3]) && exists("+1"), "still folded")
        assertEquals(rest, fold.fetchSemanticsNode().boundsInRoot.height, 0.5f, "settled back to rest")
    }

    /** Held, the stack follows the finger to the pixel, laid out from the pull with no recomposition. */
    @Test
    fun theStackFollowsTheFingerWithoutRecomposing() {
        show(busy(4))
        val fold = compose.onNodeWithContentDescription("Show 1 more waiting sale")
        val rest = fold.fetchSemanticsNode().boundsInRoot.height
        assertTrue(rest >= with(compose.density) { 48.dp.toPx() }, "a touch target of 48 dp or more: $rest")
        fold.performTouchInput { down(center); moveBy(Offset(0f, viewConfiguration.touchSlop + 1f)) }
        compose.waitForIdle()
        val held = fold.fetchSemanticsNode().boundsInRoot.height
        recompositions.reset()
        val step = with(compose.density) { 10.dp.toPx() }
        repeat(3) {
            fold.performTouchInput { moveBy(Offset(0f, step)) }
            compose.waitForIdle()
        }
        assertEquals(held + 3 * step, fold.fetchSemanticsNode().boundsInRoot.height, 1f, "the stack is as much taller as the finger went")
        assertEquals(0, recompositions.count("com.monokulo.till.ui.FoldRow"), "and nothing recomposed for it")
        fold.performTouchInput { up() }
        compose.waitForIdle()
        assertTrue(exists("+1"), "30 dp is under a third of the reach: it settles back")
    }

    /** A drag up that starts on the stack is the list's: it scrolls, and the tray stays folded. */
    @Test
    fun aDragUpFromTheStackScrollsTheList() {
        val sales = busy(4)
        show(sales + List(15) { testSale("q$it", SalePhase.Paid, paidAt = 800L - it) })
        val from = top(sales[0])
        val far = with(compose.density) { 120.dp.toPx() }
        compose.onNodeWithContentDescription("Show 1 more waiting sale").performTouchInput { swipeUp(centerY, centerY - far, durationMillis = 500) }
        compose.waitForIdle()
        assertTrue(top(sales[0]) < from - 48f, "the list scrolled: $from to ${top(sales[0])}")
        assertTrue(exists("+1"), "still folded")
    }

    /**
     * Folding a 21-row tray scrolled past its third row keeps the fold row under the finger, to the pixel, on every
     * frame (the lead's fold21: a 28 dp hop on the phone, 18 on the tablet).
     */
    @Test
    fun foldingAScrolledTrayKeepsTheFoldRowWhereItIs() {
        // A shift long enough under the tray that the list can hold the fold row anywhere: the paid sales of the day.
        show(busy(21) + List(15) { testSale("q$it", SalePhase.Paid, paidAt = 800L - it) })
        compose.onNodeWithContentDescription("Show 18 more waiting sales").performClick()
        compose.waitForIdle()
        compose.onNode(hasScrollToIndexAction()).performScrollToKey("w6")
        compose.waitForIdle()
        fun foldTop() = compose.onNodeWithContentDescription("Show", substring = true).fetchSemanticsNode().boundsInRoot.top
        val before = foldTop()
        val screen = compose.onRoot().fetchSemanticsNode().boundsInRoot
        assertTrue(before > screen.height / 4 && before < screen.height * 3 / 4, "the fold row is mid-screen: $before of ${screen.height}")
        compose.mainClock.autoAdvance = false
        compose.onNodeWithContentDescription("Show fewer").performClick()
        repeat(30) {
            compose.mainClock.advanceTimeByFrame()
            assertEquals(before, foldTop(), 1f, "frame $it: the fold row stays under the finger")
        }
        assertTrue(exists("+18"), "folded")
    }

    /**
     * A filter chosen after a fold kept under the finger moves its rows as any filter does (decision 295): the kept
     * fold's held placement is for that fold of those sales only, so Paid's rows settle up to the chips on `settle`
     * rather than jumping there in a frame.
     */
    @Test
    fun aFilterChosenAfterAKeptFoldStillMovesItsRows() {
        val sales = busy(21) + List(15) { testSale("q$it", SalePhase.Paid, paidAt = 800L - it) }
        show(sales)
        compose.onNodeWithContentDescription("Show 18 more waiting sales").performClick()
        compose.waitForIdle()
        compose.onNode(hasScrollToIndexAction()).performScrollToKey("w6")
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Show fewer").performClick()
        compose.waitForIdle()
        compose.onNode(hasScrollToIndexAction()).performScrollToIndex(0)
        compose.waitForIdle()
        val paid = sales[21]
        val from = top(paid)
        compose.mainClock.autoAdvance = false
        compose.onNode(hasText("Paid") and SemanticsMatcher.keyIsDefined(SemanticsProperties.Selected)).performClick()
        val tops = List(12) {
            compose.mainClock.advanceTimeByFrame()
            top(paid)
        }
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        val to = top(paid)
        assertTrue(to < from - 48f, "Paid's first row goes up to the chips: $from to $to")
        assertTrue(tops.any { it > to + 2f && it < from - 2f }, "and settles there, not in one frame: $tops ($from to $to)")
    }

    /**
     * The fold row's two labels are two different words: they fade through, never across (decisions 188, 291), so on
     * no frame can both be read ("Show fewerore waiting"). Read as the label's ink at a quarter or more: two labels
     * over each other draw more of it than the longer one alone.
     */
    @Test
    fun theFoldLabelFadesThroughNeverAcross() {
        show(busy(4))
        val fold = { compose.onNode(hasContentDescription("Show", substring = true) and hasClickAction()) }
        // The window's picture, read where the fold row is: the row's own layer is not one to capture.
        val label = { ink(compose.onRoot().captureToImage(), fold().fetchSemanticsNode().boundsInRoot) }
        val folded = label()
        compose.mainClock.autoAdvance = false
        fold().performClick()
        var most = 0
        repeat(30) {
            compose.mainClock.advanceTimeByFrame()
            most = maxOf(most, label())
        }
        val open = label()
        assertTrue(most <= maxOf(folded, open) * INK_SLACK, "two labels drawn over each other: $most ink pixels, the labels alone $folded and $open")
    }

    /** The pixels of [image] inside [r] darker than a [SEEN] step from the tray's amber toward the ink: words that can be read. */
    private fun ink(image: ImageBitmap, r: Rect): Int {
        val pixels = image.toPixelMap()
        val xs = r.left.toInt().coerceAtLeast(0) until r.right.toInt().coerceAtMost(pixels.width)
        val ys = r.top.toInt().coerceAtLeast(0) until r.bottom.toInt().coerceAtMost(pixels.height)
        var light = 0f
        var dark = 255f
        for (y in ys) for (x in xs) {
            val l = luminance(pixels[x, y])
            light = maxOf(light, l)
            dark = minOf(dark, l)
        }
        val faint = light - (light - dark) * SEEN
        var n = 0
        for (y in ys) for (x in xs) if (luminance(pixels[x, y]) < faint) n++
        return n
    }

    private fun luminance(c: Color) = 255f * (0.2126f * c.red + 0.7152f * c.green + 0.0722f * c.blue)

    private companion object {
        /** A frame's label ink may exceed the longer label's at rest by this much (anti-aliasing); two labels are far more. */
        const val INK_SLACK = 1.1f

        /** Words drawn at a quarter or more of their ink are words a cashier reads. */
        const val SEEN = 0.25f
    }
}
