package com.monokulo.till

import uniffi.till_app.Screen
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.IntSize
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ShiftScreen
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.semantics.SemanticsProperties
import com.monokulo.till.core.SalePhase
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** M7 on a real frame clock: the shift and a sale reach each other through the stage, the code inside laid out once. */
class StageTest {
    @get:Rule
    val compose = createComposeRule()

    private val sale = testSale("a1", expiresAt = 1_600)
    private val sales = listOf(sale, testSale("b2", expiresAt = 1_600))
    private val now = mutableLongStateOf(1_000)
    private val recompositions = Recompositions()
    private lateinit var scope: CoroutineScope

    @After
    fun stopCounting() = recompositions.close()

    private fun stage(state: SeekableTransitionState<Screen>, byItself: Boolean = false) = compose.setContent {
        scope = rememberCoroutineScope()
        Till {
            TillStage(state, cameUpByItself = { byItself }) { s ->
                when (s) {
                    is Screen.Sale -> SaleScreen(rememberFace(sale, now.longValue), noActions)
                    else -> ShiftScreen(rememberShift(sales, now.longValue), noActions)
                }
            }
        }
    }

    private fun code() = compose.onNodeWithContentDescription("Payment code for", substring = true, useUnmergedTree = true)

    private fun row(): Rect =
        compose.onNodeWithText(rowTitle(sale), substring = true, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot

    @Test
    fun theShiftGrowsIntoTheSaleWithBothOnScreenOnTheWayAndTheCodeLaidOutOnce() {
        var screen by mutableStateOf<Screen>(Screen.Till)
        val state = SeekableTransitionState<Screen>(Screen.Till)
        compose.setContent {
            Till {
                LaunchedEffect(screen) { state.animateTo(screen, Motion.Full.stage) }
                TillStage(state, cameUpByItself = { false }) { s ->
                    when (s) {
                        is Screen.Sale -> SaleScreen(rememberFace(sale, now.longValue), noActions)
                        else -> ShiftScreen(rememberShift(sales, now.longValue), noActions)
                    }
                }
            }
        }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false

        screen = Screen.Sale("a1")
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        val size = code().fetchSemanticsNode().size
        recompositions.reset()
        val sizes = mutableListOf<IntSize>()
        repeat(5) {
            compose.mainClock.advanceTimeBy(40)
            sizes += code().fetchSemanticsNode().size
        }
        compose.onAllNodesWithText("This shift", useUnmergedTree = true).assertCountEquals(1)
        // The sale's status (spoken as it changes), not the shift's tray header of the same words.
        compose.onAllNodes(hasText("Waiting for payment") and SemanticsMatcher.keyIsDefined(SemanticsProperties.LiveRegion), useUnmergedTree = true).assertCountEquals(1)
        assertTrue(sizes.all { it == size }, "the code keeps its layout size through the grow: $size, $sizes")
        // SaleScreen's own body (its shared surface) may run as the transition starts and ends; what it shows doesn't.
        for (name in listOf("QrCode", "SaleRow", "LiveSale", "StatusFoot", "StatusLine", "RingTick", "ShiftScreen")) {
            assertEquals(0, recompositions.count("com.monokulo.till.ui.$name"), "$name isn't recomposed by the transition's frames")
        }

        compose.mainClock.advanceTimeBy(1_500)
        compose.onAllNodesWithText("This shift", useUnmergedTree = true).assertCountEquals(0)
        compose.onNode(hasText("Waiting for payment") and SemanticsMatcher.keyIsDefined(SemanticsProperties.LiveRegion), useUnmergedTree = true).assertExists()
        assertEquals(size, code().fetchSemanticsNode().size, "and lands at the size it was laid out at")
    }

    @Test
    fun aBackGestureScrubsTheSaleTowardItsRowItsCodeAtItsOwnSizeAndCancelledItSpringsBack() {
        val state = SeekableTransitionState<Screen>(Screen.Sale("a1"))
        stage(state)
        compose.waitForIdle()
        val codeWidth = code().fetchSemanticsNode().drawn().width
        val codeSize = code().fetchSemanticsNode().size
        compose.mainClock.autoAdvance = false

        fun seek(fraction: Float) {
            scope.launch { state.seekTo(fraction, Screen.Till) }
            repeat(3) { compose.mainClock.advanceTimeByFrame() }
        }
        seek(0.08f)
        // Only the sale's ground moves toward the row; what is on it fades where it is, at its own size (decision 185).
        assertEquals(codeWidth, code().fetchSemanticsNode().drawn().width, "the code is never drawn through a scale")
        assertEquals(codeSize, code().fetchSemanticsNode().size, "and keeps the size it was laid out at")
        seek(0.99f)
        assertEquals(codeWidth, code().fetchSemanticsNode().drawn().width, "the code is never drawn through a scale")

        scope.launch { state.animateTo(Screen.Sale("a1")) }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(Screen.Sale("a1"), state.currentState, "cancelled, it springs back")
        assertEquals(codeWidth, code().fetchSemanticsNode().drawn().width)
    }

    @Test
    fun theCodeGrowsOutOfItsCardLaidOutOnceAndAShrinkMidGrowReverses() {
        var screen by mutableStateOf<Screen>(Screen.Sale("a1"))
        val state = SeekableTransitionState<Screen>(Screen.Sale("a1"))
        // Where the code is drawn while it moves: its track (the stage draws it there, not at either screen's node).
        val tracks = mutableListOf<GroundTrack>()
        compose.setContent {
            Till {
                LaunchedEffect(screen) { state.animateTo(screen, Motion.Full.stage) }
                CompositionLocalProvider(LocalGroundTracks provides { key -> GroundTrack(key).also { if (key == "code/a1") tracks += it } }) {
                TillStage(state, cameUpByItself = { false }) { s ->
                    when (s) {
                        is Screen.Code -> CodeScreen(rememberFace(sale), onBack = {})
                        else -> SaleScreen(rememberFace(sale, now.longValue), noActions)
                    }
                }
                }
            }
        }
        compose.waitForIdle()
        val small = code().fetchSemanticsNode().size
        compose.mainClock.autoAdvance = false
        fun big() = compose.onNodeWithContentDescription("Payment code, tap to shrink", useUnmergedTree = true).fetchSemanticsNode()
        fun drawnWidth(): Float = tracks.firstNotNullOfOrNull { it.drawn() }?.width ?: big().drawn().width

        screen = Screen.Code("a1")
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        val bigSize = big().size
        recompositions.reset()
        val drawn = mutableListOf<Float>()
        for (t in listOf(0L, 75L, 75L)) {
            compose.mainClock.advanceTimeBy(t)
            assertEquals(bigSize, big().size, "the enlarged code keeps its laid-out size at every frame")
            assertEquals(small, code().fetchSemanticsNode().size, "and so does the sale's")
            drawn += drawnWidth()
        }
        assertEquals(0, recompositions.count("com.monokulo.till.ui.QrCode"), "the code isn't recomposed by the grow")
        assertTrue(drawn[0] < drawn[2] && drawn[2] < bigSize.width, "mid-grow it is drawn between the card and the screen: $drawn of ${bigSize.width}")

        screen = Screen.Sale("a1")
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(50)
        val shrinking = drawnWidth()
        compose.mainClock.advanceTimeBy(50)
        assertTrue(drawnWidth() < shrinking && shrinking <= drawn[2] + 1f, "Back mid-grow shrinks from where it got to: ${drawn[2]} -> $shrinking -> ${drawnWidth()}")

        compose.mainClock.advanceTimeBy(1_500)
        compose.onAllNodesWithContentDescription("Payment code, tap to shrink", useUnmergedTree = true).assertCountEquals(0)
        assertEquals(small, code().fetchSemanticsNode().size, "and lands at its card")
    }

    @Test
    fun backFromTheThirdRowsSaleLandsOnThatRowWithTheRowsAroundItAtRest() {
        // Three live sales fill the tray's three rows, unfolded; the fourth row is a paid one, under Earlier.
        val rows = listOf(testSale("r1", expiresAt = 1_600), testSale("r2", expiresAt = 1_600), testSale("r3", expiresAt = 1_600), testSale("r4", SalePhase.Paid, paidAt = 990))
        var screen by mutableStateOf<Screen>(Screen.Till)
        val state = SeekableTransitionState<Screen>(Screen.Till)
        compose.setContent {
            Till {
                LaunchedEffect(screen) { state.animateTo(screen, Motion.Full.stage) }
                TillStage(state, cameUpByItself = { false }) { s ->
                    when (s) {
                        is Screen.Sale -> SaleScreen(rememberFace(rows.first { it.orderId == s.orderId }, now.longValue), noActions)
                        else -> ShiftScreen(rememberShift(rows, now.longValue), noActions)
                    }
                }
            }
        }
        compose.waitForIdle()
        fun top(sale: com.monokulo.till.sales.TillSale) =
            compose.onNodeWithText(rowTitle(sale), substring = true, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.top
        val rest = rows.map(::top)

        screen = Screen.Sale("r3")
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        screen = Screen.Till
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(rest, rows.map(::top), "landed, every row is where it was before the sale opened")
        compose.mainClock.advanceTimeBy(100)
        assertEquals(rest, rows.map(::top), "and stays there")
    }
}

/** Where this node is drawn, moved and scaled with its layers, not clipped to where its parents sit at rest. */
fun SemanticsNode.drawn(): Rect {
    val c = layoutInfo.coordinates
    return Rect(c.localToRoot(Offset.Zero), c.localToRoot(Offset(c.size.width.toFloat(), c.size.height.toFloat())))
}
