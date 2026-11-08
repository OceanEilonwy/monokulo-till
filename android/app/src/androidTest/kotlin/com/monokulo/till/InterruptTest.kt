package com.monokulo.till

import uniffi.till_app.Screen
import androidx.activity.OnBackPressedDispatcher
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.unit.dp
import com.monokulo.till.core.SalePhase
import com.monokulo.till.sales.live
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ShiftScreen
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized
import kotlin.math.abs
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Every screen move interrupted a quarter, half and three quarters of the
 * way (N2), on the test clock, since adb's input is slower than the moves:
 * by Back, by the on-screen way back, by the sale being paid or running
 * out, and by a new sale coming up. In every cell the stage and the model
 * agree once it has settled, Back still works from there, and on the frame
 * of the interruption any live code on screen is square and no smaller
 * than at rest (decision 186).
 */
@RunWith(Parameterized::class)
class InterruptTest(private val move: Moved, private val interruption: Interruption, private val at: Float) {
    @get:Rule
    val compose = createComposeRule()

    enum class Moved(val from: Screen, val to: Screen) {
        RowGrow(Screen.Till, Screen.Sale("a1")),
        CodeGrow(Screen.Sale("a1"), Screen.Code("a1")),
        OrderGrow(Screen.Sale("a1"), Screen.Order("a1")),
    }

    enum class Interruption { Back, WayBack, Paid, Expired, NewSale }

    private val now = mutableLongStateOf(1_000)
    private val other = testSale("b2", expiresAt = 1_600)
    private var sale by mutableStateOf(testSale("a1", expiresAt = 1_600).copy(receipt = sampleReceipt()))
    private lateinit var dispatcher: OnBackPressedDispatcher

    @Test
    fun theStageAndTheModelAgreeAndBackStillWorks() {
        val model = TestModel(move.from)
        compose.setContent {
            dispatcher = LocalOnBackPressedDispatcherOwner.current!!.onBackPressedDispatcher
            Till {
                ModelStage(model) { s ->
                    val shown = if (s.orderIdOrNull() == "b2") other else sale
                    when (s) {
                        // As the activity does: the enlarged code goes back by itself once its sale isn't live.
                        is Screen.Code -> CodeScreen(rememberFace(shown), onBack = { model.back(s) })
                        is Screen.Order -> OrderPage(rememberFace(shown), "Back to the sale", onBack = { model.back(s) }, onShowCode = { model.back(s) })
                        is Screen.Sale -> SaleScreen(rememberFace(shown, now.longValue), noActions)
                        else -> EngineShift(listOf(sale, other), now.longValue, noActions)
                    }
                }
            }
        }
        compose.waitForIdle()
        compose.mainClock.autoAdvance = false
        val rest = with(compose.density) { RestCode.toPx() }

        model.go(move.to)
        var frames = 0
        while (model.stage.fraction < at && frames++ < 120) compose.mainClock.advanceTimeByFrame()
        assertTrue(model.stage.currentState != model.stage.targetState, "$move is under way at $at")

        val codes = mutableListOf<String>()
        fun lookAtTheCode(why: String) {
            if (!sale.live) return
            for (node in compose.onAllNodesWithContentDescription("Payment code", substring = true, useUnmergedTree = true).fetchSemanticsNodes()) {
                val d = node.drawn()
                if (abs(d.width - d.height) > 1f) codes += "$why: not square $d"
                if (d.width < rest - 1f) codes += "$why: ${d.width} px, smaller than its rest ${rest}"
            }
        }
        lookAtTheCode("as it is interrupted")
        when (interruption) {
            Interruption.Back -> compose.runOnUiThread { dispatcher.onBackPressed() }
            Interruption.WayBack -> model.back(model.screen.value)
            Interruption.Paid -> sale = sale.copy(phase = SalePhase.Paid, paidAt = 1_001)
            Interruption.Expired -> sale = sale.copy(phase = SalePhase.Expired)
            Interruption.NewSale -> model.go(Screen.Sale("b2"), byItself = true)
        }
        compose.mainClock.advanceTimeByFrame()
        lookAtTheCode("the frame after")
        assertTrue(codes.isEmpty(), codes.joinToString("\n"))

        compose.mainClock.advanceTimeBy(2_000)
        assertAgree(model, "settled after $interruption at $at of $move")

        // Back still works from wherever that left the till.
        val back = model.backTarget() ?: return
        compose.runOnUiThread { dispatcher.onBackPressed() }
        compose.mainClock.advanceTimeBy(2_000)
        assertEquals(back, model.screen.value, "Back works after $interruption at $at of $move")
        assertAgree(model, "after that Back")
    }

    private fun assertAgree(model: TestModel, why: String) {
        assertEquals(model.screen.value, model.stage.currentState, "$why: the stage is on the model's screen")
        assertEquals(model.screen.value, model.stage.targetState, "$why: and is going nowhere else")
    }

    companion object {
        /**
         * The sale's code image at rest, at the least: its card is the window less 20 dp a side up to 372 dp, so
         * 264 dp on the narrowest phone (304 dp), less the card's 18 dp of quiet zone each side.
         */
        val RestCode = 228.dp

        @JvmStatic
        @Parameterized.Parameters(name = "{0} interrupted by {1} at {2}")
        fun cells(): List<Array<Any>> = Moved.entries.flatMap { m ->
            Interruption.entries.flatMap { i -> listOf(0.25f, 0.5f, 0.75f).map { p -> arrayOf<Any>(m, i, p) } }
        }
    }
}
