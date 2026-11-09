package com.monokulo.till

import uniffi.till_app.Screen
import androidx.activity.BackEventCompat
import androidx.activity.OnBackPressedDispatcher
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.util.lerp
import androidx.compose.ui.test.onNodeWithText
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.HeroTag
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ShiftScreen
import androidx.compose.runtime.mutableLongStateOf
import org.junit.Rule
import org.junit.Test
import kotlin.math.abs
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The back gesture over the stage (decision 169), driven the way Android
 * drives it, through the activity's back dispatcher: the finger's progress
 * is how far every part has moved, and where it goes is read as it starts.
 */
class StageBackTest {
    @get:Rule
    val compose = createComposeRule()

    private val sale = testSale("a1", expiresAt = 1_600)
    private val other = testSale("b2", expiresAt = 1_600)
    /** A paid sale: its page's hero is its row's badge, the glyph that moves under the finger. */
    private val paid = testSale("p1", phase = SalePhase.PAID, paidAt = 990)
    private val sales = listOf(testSale("z0", expiresAt = 1_600), testSale("y0", expiresAt = 1_600), sale, other, paid)
    private val now = mutableLongStateOf(1_000)
    private lateinit var dispatcher: OnBackPressedDispatcher

    /** Every surface's tracks by key, to read where the code is drawn under the finger. */
    private val tracks = mutableMapOf<String, MutableList<GroundTrack>>()

    private fun stage(model: TestModel) = compose.setContent {
        dispatcher = LocalOnBackPressedDispatcherOwner.current!!.onBackPressedDispatcher
        Till {
            CompositionLocalProvider(LocalGroundTracks provides { key -> GroundTrack(key).also { tracks.getOrPut(key) { mutableListOf() } += it } }) {
            ModelStage(model) { s ->
                when (s) {
                    is Screen.Code -> CodeScreen(rememberFace(sale), onBack = { model.back(s) })
                    is Screen.Sale -> SaleScreen(rememberFace(sales.first { it.orderId == s.orderId }, now.longValue), noActions)
                    else -> EngineShift(sales, now.longValue, noActions)
                }
            }
            }
        }
    }

    private fun started() = compose.runOnUiThread { dispatcher.dispatchOnBackStarted(BackEventCompat(10f, 1000f, 0f, BackEventCompat.EDGE_LEFT)) }

    private fun gesture(progress: Float) {
        compose.runOnUiThread { dispatcher.dispatchOnBackProgressed(BackEventCompat(10f, 1000f, progress, BackEventCompat.EDGE_LEFT)) }
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
    }

    /** The paid sale's hero, where it is drawn: on its way into its row's badge while the finger is across. */
    private fun hero(): Rect = compose.onNodeWithTag(HeroTag, useUnmergedTree = true).fetchSemanticsNode().drawn()

    /** At rest, the stage shows the model's screen and is going nowhere else. */
    private fun assertAgree(model: TestModel, why: String) {
        assertEquals(model.screen.value, model.stage.currentState, "$why: the stage is on the model's screen")
        assertEquals(model.screen.value, model.stage.targetState, "$why: and is going nowhere else")
    }

    @Test
    fun theSaleIsAsFarTowardItsRowAsTheFingerIsAcross() {
        val model = TestModel(Screen.Sale("p1"))
        stage(model)
        compose.waitForIdle()
        val rest = hero()
        compose.mainClock.autoAdvance = false

        started()
        gesture(0.05f)
        val at = listOf(0.25f, 0.5f, 0.75f).map { gesture(it); hero() }
        // The hero moves and shrinks on the same straight line as the ground: a point on it moves as its bounds do.
        fun step(a: Rect, b: Rect) = listOf(b.top - a.top, b.left - a.left, b.width - a.width)
        val first = step(rest, at[0])
        val second = step(at[0], at[1])
        val third = step(at[1], at[2])
        for (i in first.indices) {
            // Within two pixels: each point is rounded to a pixel where it's placed.
            assertTrue(abs(second[i] - third[i]) <= 2f, "0.25 to 0.5 moves as far as 0.5 to 0.75: $second vs $third")
            assertTrue(abs(first[i] - second[i]) <= 2f, "and the first quarter as far as the second: $first vs $second")
        }
        assertTrue(abs(second[0]) > 10f, "and it does move: $second")
        assertEquals(Screen.Sale("p1"), model.screen.value, "nothing changes before the gesture commits")

        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(listOf<Screen>(Screen.Sale("p1")), model.commits, "released, the model is told the sale was left")
        assertEquals(Screen.Till, model.screen.value)
        assertAgree(model, "released")
    }

    @Test
    fun cancelledTheSaleSpringsBackToWhereItWas() {
        val model = TestModel(Screen.Sale("p1"))
        stage(model)
        compose.waitForIdle()
        val rest = hero()
        compose.mainClock.autoAdvance = false

        started()
        gesture(0.4f)
        assertTrue(abs(hero().top - rest.top) > 10f, "the preview moved it: ${rest.top} -> ${hero().top}")
        compose.runOnUiThread { dispatcher.dispatchOnBackCancelled() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(rest, hero(), "cancelled, it springs back")
        assertTrue(model.commits.isEmpty(), "and nothing went back")
        assertAgree(model, "cancelled")
    }

    @Test
    fun whereBackGoesIsReadAsTheGestureStartsNotAtTheLastComposition() {
        val model = TestModel(Screen.Sale("a1"))
        stage(model)
        compose.waitForIdle()
        // The model's answer changes with nothing recomposing (a sale arriving while Settings is up, say).
        model.backTarget = { Screen.Sale("b2") }
        compose.mainClock.autoAdvance = false
        started()
        gesture(0.3f)
        assertEquals(Screen.Sale("b2"), model.stage.targetState, "the preview is toward where back goes now")
        compose.runOnUiThread { dispatcher.dispatchOnBackCancelled() }
        compose.mainClock.advanceTimeBy(1_500)
        assertAgree(model, "cancelled")
    }

    @Test
    fun aScrubbedBackLandsOnTheSalesOwnRowWithTheRowsAroundItAtRest() {
        val model = TestModel(Screen.Till)
        stage(model)
        compose.waitForIdle()
        fun top(id: String) =
            compose.onNodeWithText(rowTitle(sales.first { it.orderId == id }), substring = true, useUnmergedTree = true)
                .fetchSemanticsNode().boundsInRoot.top
        // The rows on screen: four live sales fill the folded tray's three rows and its fold, the fourth (b2) folded.
        val shown = sales.filter { it.orderId != "b2" }
        val rest = shown.map { top(it.orderId) }
        compose.mainClock.autoAdvance = false
        model.go(Screen.Sale("a1"))
        compose.mainClock.advanceTimeBy(1_500)

        started()
        gesture(0.3f)
        gesture(0.6f)
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertAgree(model, "landed")
        assertEquals(Screen.Till, model.stage.currentState)
        assertEquals(rest, shown.map { top(it.orderId) }, "landed, every row is where it was")
        compose.mainClock.advanceTimeBy(100)
        assertEquals(rest, shown.map { top(it.orderId) }, "and stays there")
    }

    @Test
    fun aSaleComingUpUnderACancelledGestureIsWhereTheStageGoes() {
        val model = TestModel(Screen.Sale("a1"))
        stage(model)
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        started()
        gesture(0.3f)
        // A new sale comes up by itself while the finger is down.
        model.go(Screen.Sale("b2"), byItself = true)
        gesture(0.5f)
        compose.runOnUiThread { dispatcher.dispatchOnBackCancelled() }
        compose.mainClock.advanceTimeBy(1_500)
        assertTrue(model.commits.isEmpty(), "cancelled, nothing went back")
        assertEquals(Screen.Sale("b2"), model.screen.value)
        assertAgree(model, "after the cancel")
    }

    @Test
    fun aSaleComingUpUnderACommittedGestureIsWhereTheStageGoes() {
        val model = TestModel(Screen.Sale("a1"))
        stage(model)
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        started()
        gesture(0.3f)
        model.go(Screen.Sale("b2"), byItself = true)
        gesture(0.6f)
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(listOf<Screen>(Screen.Sale("a1")), model.commits, "the Back was from the sale the gesture started on")
        assertEquals(Screen.Sale("b2"), model.screen.value, "which had already gone, so the model stays on the new sale")
        assertAgree(model, "after the commit")
        // And Back still works from there.
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(Screen.Till, model.screen.value)
        assertAgree(model, "after a second Back")
    }

    @Test
    fun aBackPressedMidGrowGoesBackFromTheCodeAndASecondReachesTheShift() {
        val model = TestModel(Screen.Sale("a1"))
        stage(model)
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        // The cashier taps the code and presses Back before it has landed (decision 186).
        model.go(Screen.Code("a1"))
        compose.mainClock.advanceTimeBy(96)
        assertTrue(model.stage.currentState != model.stage.targetState, "the grow is still running")
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(listOf<Screen>(Screen.Code("a1")), model.commits, "Back was from the code, the model's screen")
        assertEquals(Screen.Sale("a1"), model.screen.value)
        assertAgree(model, "after the first Back")

        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(Screen.Till, model.screen.value, "a second Back reaches the shift")
        assertAgree(model, "after the second Back")
    }

    /**
     * A back gesture on the enlarged code shrinks the code toward its card as the finger moves, not only the words
     * (decision 223): half across, the code is drawn half way between its two rest places; released, it lands on the
     * card from there.
     */
    @Test
    fun theCodeShrinksTowardItsCardAsFarAsTheFingerIsAcross() {
        val model = TestModel(Screen.Sale("a1"))
        stage(model)
        compose.waitForIdle()
        val card = compose.onNodeWithContentDescription("Payment code for", substring = true, useUnmergedTree = true).fetchSemanticsNode().drawn()
        model.go(Screen.Code("a1"))
        compose.mainClock.advanceTimeBy(1_500)
        compose.waitForIdle()
        val big = compose.onNodeWithContentDescription("Payment code, tap to shrink", useUnmergedTree = true).fetchSemanticsNode().drawn()
        compose.mainClock.autoAdvance = false

        started()
        gesture(0.05f)
        for (p in listOf(0.25f, 0.5f, 0.75f)) {
            gesture(p)
            val drawn = tracks["code/a1"]?.firstNotNullOfOrNull { it.drawn() }
            assertTrue(drawn != null, "at $p the code is moving under the finger")
            val want = lerp(big.width, card.width, model.stage.fraction)
            assertTrue(abs(drawn.width - want) <= 2f, "at $p (the stage at ${model.stage.fraction}) the code is ${drawn.width} px wide, not $want: between ${big.width} and ${card.width}")
            assertTrue(drawn.width < big.width - 10f, "at $p the code has shrunk from ${big.width}: ${drawn.width}")
        }
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(1_500)
        assertEquals(Screen.Sale("a1"), model.screen.value)
        assertAgree(model, "after the release")
    }

    /**
     * A second Back while the shift is still arriving (decision 260 c): the stage's handler takes it, the model's
     * Back from the shift does nothing, and the stage lands on the shift; no second move starts, and the app is not
     * left. Once the shift rests, Back is not the stage's (decision 77: Back on the shift leaves the app).
     */
    @Test
    fun aSecondBackWhileTheShiftArrivesLandsOnTheShift() {
        val model = TestModel(Screen.Sale("a1"))
        stage(model)
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        Recompositions().use { counts ->
            compose.runOnUiThread { dispatcher.onBackPressed() }
            var frames = 0
            while (model.stage.fraction < 0.5f && frames++ < 60) compose.mainClock.advanceTimeByFrame()
            assertTrue(model.stage.currentState != model.stage.targetState, "the shrink into the row is half way: ${model.stage.fraction}")
            var handled = false
            compose.runOnUiThread { handled = dispatcher.hasEnabledCallbacks(); dispatcher.onBackPressed() }
            assertTrue(handled, "the stage still takes Back while the shift arrives")
            var landedAt = 0
            while (model.stage.currentState != model.stage.targetState && landedAt++ < 90) {
                assertEquals(Screen.Till, model.stage.targetState, "no second move starts")
                compose.mainClock.advanceTimeByFrame()
            }
            compose.mainClock.advanceTimeBy(200)
            assertEquals(listOf(Screen.Sale("a1"), Screen.Till), model.commits, "the second Back reached the model, from the shift, as a no-op")
            assertEquals(Screen.Till, model.screen.value)
            assertAgree(model, "landed")
            // The handler's enablement is read off the stage as a derived state: the stage's root composes as the
            // change starts and as it lands, never once a frame.
            val roots = counts.count("com.monokulo.till.ModelStage")
            assertTrue(roots in 1..4, "the root recomposed $roots times over ${frames + landedAt} frames")
        }
        var handled = true
        compose.runOnUiThread { handled = dispatcher.hasEnabledCallbacks() }
        assertTrue(!handled, "on the resting shift Back is not the stage's: it leaves the app")
    }
}
