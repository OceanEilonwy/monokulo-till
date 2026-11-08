package com.monokulo.till

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeRight
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.LayoutDirection
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.TillActions
import com.monokulo.till.engine.CloseSheet
import uniffi.till_app.Action
import uniffi.till_screens.CloseChoice
import androidx.compose.runtime.mutableStateOf
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * A shift row swiped left ([RowSwipe]): past a third it brings up the sale page's X's sheet, the same answers
 * doing the same; a small swipe settles back; a swipe up the list still scrolls it; only rows whose page has the X
 * swipe; reduced motion still asks; TalkBack reaches it as the row's "Close sale"; right to left, it is still a left swipe.
 */
class SwipeToCloseTest {
    @get:Rule
    val compose = createComposeRule()

    private val now = mutableLongStateOf(1_000)
    private val opened = mutableListOf<String>()
    private val closed = mutableListOf<Pair<String, CloseChoice>>()
    private var sales = emptyList<TillSale>()

    /** The close sheet, as the engine keeps it: up on a row's AskClose, down on an answer or a dismissal. */
    private val sheet = mutableStateOf<CloseSheet?>(null)
    private val actions = TillActions(
        send = { action ->
            when (action) {
                is Action.AskClose -> sheet.value = sales.first { it.orderId == action.orderId }.let { CloseSheet(it.orderId, closeView(it)) }
                is Action.DismissClose -> sheet.value = null
                is Action.Close -> {
                    sheet.value = null
                    closed += action.orderId to action.choice
                }
                is Action.OpenSale -> opened += action.orderId
                else -> Unit
            }
        },
        onBackToPos = {},
    )

    private fun title(sale: TillSale) = rowTitle(sale)

    private fun row(sale: TillSale) = compose.onNodeWithText(title(sale))

    private fun show(sales: List<TillSale>, motion: Motion = Motion.Full, direction: LayoutDirection = LayoutDirection.Ltr) {
        this.sales = sales
        compose.setContent {
            CompositionLocalProvider(LocalLayoutDirection provides direction) {
                Till(motion) { EngineShift(sales, now.longValue, actions, close = sheet.value) }
            }
        }
        compose.waitForIdle()
    }

    private fun asks() = compose.onAllNodes(SemanticsMatcher("the close sheet") { n ->
        n.config.getOrNull(SemanticsProperties.Text).orEmpty().any { it.text == "Is the customer paying?" }
    }).fetchSemanticsNodes().isNotEmpty()

    /** Where the row's end is: its start is clipped at the list's side as it moves past it. */
    private fun end(sale: TillSale) = row(sale).fetchSemanticsNode().boundsInRoot.right

    /** Swiped from four fifths of the row to [to] of it, in [ms]: well inside the window, clear of the edges' Back. */
    private fun swipe(sale: TillSale, to: Float, ms: Long = 200) = row(sale).performTouchInput {
        swipeLeft(startX = width * 0.8f, endX = width * to, durationMillis = ms)
    }

    /** Holds the finger half the row's width to the left of where it went down; how far the row has followed it. */
    private fun held(sale: TillSale, rest: Float): Float {
        row(sale).performTouchInput { down(Offset(width * 0.8f, centerY)) }
        repeat(10) {
            row(sale).performTouchInput { moveBy(Offset(-width * 0.05f, 0f), delayMillis = 16) }
            compose.waitForIdle()
        }
        val moved = end(sale) - rest
        row(sale).performTouchInput { up() }
        compose.waitForIdle()
        return moved
    }

    @Test
    fun theRowFollowsTheFinger() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale))
        val rest = end(sale)
        val moved = held(sale, rest)
        val width = row(sale).fetchSemanticsNode().boundsInRoot.width
        assertTrue(moved < -width * 0.4f, "about half the row to the start, less the touch slop: $moved of $width")
        assertTrue(asks(), "let go past a third")
    }

    @Test
    fun aSwipePastAThirdAsksWhatTheXAsks() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale))
        val rest = end(sale)
        swipe(sale, to = 0.2f)
        compose.waitForIdle()
        assertTrue(asks(), "the X's sheet is up")
        for (answer in closeView(sale)!!.answers) assertTrue(compose.onAllNodes(SemanticsMatcher("answer") { n ->
            n.config.getOrNull(SemanticsProperties.Text).orEmpty().any { it.text == answer.title }
        }).fetchSemanticsNodes().isNotEmpty(), "its answer ${answer.choice}")
        assertEquals(rest, end(sale), 0.5f, "the row is back in its place under the sheet")
        compose.onNodeWithText("Yes: keep the code up").performClick()
        compose.waitForIdle()
        assertEquals(listOf("w1" to CloseChoice.KEEP_UP), closed, "the answer goes up as the X's does")
        assertFalse(asks())
        assertEquals(emptyList(), opened, "a swipe opens nothing")
    }

    @Test
    fun refundingFromTheRowOpensTheSaleWhereTheRefundShows() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale))
        swipe(sale, to = 0.2f)
        compose.waitForIdle()
        compose.onNodeWithText(closeView(sale)!!.answers.first { it.choice == CloseChoice.REFUND_IT }.title).performClick()
        compose.waitForIdle()
        assertEquals(listOf("w1" to CloseChoice.REFUND_IT), closed)
        assertEquals(listOf("w1"), opened, "the X's refund stays on the sale: from the row, the sale opens")
    }

    @Test
    fun aSmallSlowSwipeSettlesBack() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale))
        val rest = end(sale)
        swipe(sale, to = 0.65f, ms = 800)
        compose.waitForIdle()
        assertFalse(asks(), "under a third and not flung")
        assertEquals(rest, end(sale), 0.5f, "settled back")
        row(sale).performTouchInput { swipeRight(startX = width * 0.2f, endX = width * 0.8f) }
        compose.waitForIdle()
        assertFalse(asks(), "to the end is no swipe")
        assertEquals(emptyList(), closed)
    }

    @Test
    fun aSwipeUpTheRowsStillScrollsTheList() {
        val sales = List(24) { testSale("p$it", SalePhase.Paid, paidAt = 990) }.let { listOf(testSale("w1", expiresAt = 1_900)) + it }
        show(sales)
        val before = compose.onNodeWithText(title(sales[3])).fetchSemanticsNode().boundsInRoot.top
        row(sales[0]).performTouchInput { swipeUp() }
        compose.waitForIdle()
        assertFalse(asks(), "a swipe up is the list's")
        val after = compose.onAllNodes(SemanticsMatcher("row") { n ->
            n.config.getOrNull(SemanticsProperties.Text).orEmpty().any { it.text == title(sales[3]) }
        }).fetchSemanticsNodes().map { it.boundsInRoot.top }
        assertTrue(after.all { it < before - 100f }, "the list scrolled: $before to $after")
    }

    @Test
    fun onlyARowWhosePageHasTheXSwipes() {
        val paid = testSale("p1", SalePhase.Paid, paidAt = 990)
        show(listOf(testSale("w1", expiresAt = 1_900), paid))
        swipe(paid, to = 0.1f)
        compose.waitForIdle()
        assertFalse(asks(), "a paid sale's page has no X")
        assertTrue(row(paid).fetchSemanticsNode().config.getOrNull(SemanticsActions.CustomActions).isNullOrEmpty(), "nor TalkBack's action")
    }

    @Test
    fun reducedMotionDoesNotFollowButStillAsks() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale), Motion.Still)
        val rest = end(sale)
        assertEquals(0f, held(sale, rest), 0.5f, "the row stays where it is under the finger")
        assertTrue(asks(), "and the swipe asks all the same")
        assertEquals(rest, end(sale), 0.5f)
    }

    @Test
    fun talkBackReachesItAsTheRowsCloseSale() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale))
        val custom = row(sale).fetchSemanticsNode().config.getOrNull(SemanticsActions.CustomActions).orEmpty()
        assertEquals(listOf("Close sale"), custom.map { it.label }, "the X's own label")
        compose.runOnUiThread { custom.single().action() }
        compose.waitForIdle()
        assertTrue(asks())
    }

    @Test
    fun rightToLeftTheRowStillFollowsTheFingerLeft() {
        val sale = testSale("w1", expiresAt = 1_900)
        show(listOf(sale), direction = LayoutDirection.Rtl)
        val rest = end(sale)
        val moved = held(sale, rest)
        val width = row(sale).fetchSemanticsNode().boundsInRoot.width
        assertTrue(moved < -width * 0.4f, "with the finger, not against it: $moved of $width")
        assertTrue(asks(), "let go past a third")
        assertEquals(rest, end(sale), 0.5f, "settled back")
    }
}
