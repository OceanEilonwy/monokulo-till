package com.monokulo.till

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.junit4.createComposeRule
import com.monokulo.till.overlay.BubbleFace
import com.monokulo.till.overlay.PanelActions
import com.monokulo.till.overlay.PanelMotion
import com.monokulo.till.overlay.PanelPage
import com.monokulo.till.overlay.PhonePanel
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import uniffi.till_screens.BubbleLook

/**
 * The overlay's moves recompose nothing per frame (`docs/motion.md` §10
 * rule 1): mid-move, the panel's card, its code, its rings and the order
 * run no composable body; every moving value is read in a layer, layout
 * or draw lambda. Counted from the compiler's trace markers, as Layout
 * Inspector counts them (decision 161).
 */
class OverlayRecompositionsTest {
    @get:Rule
    val compose = createComposeRule()

    private val recompositions = Recompositions()
    private val actions = PanelActions({}, {}, {}, {}, {}, {}, {}, {})
    private val expires = System.currentTimeMillis() / 1000 + 15 * 60
    private var sale by mutableStateOf(testSale("r1", SalePhase.Confirming, expiresAt = expires))
    private var page by mutableStateOf<PanelPage>(PanelPage.Code)
    private lateinit var motion: PanelMotion

    private val named = listOf(
        "com.monokulo.till.overlay.PanelCard", "com.monokulo.till.overlay.Details", "com.monokulo.till.ui.QrCode",
        "com.monokulo.till.ui.PhaseRing", "com.monokulo.till.overlay.BubbleFace", "com.monokulo.till.overlay.PanelOrderContent",
        "com.monokulo.till.overlay.StatusRow",
    )

    @After
    fun close() = recompositions.close()

    private fun panel(raised: Float) {
        compose.mainClock.autoAdvance = false
        motion = PanelMotion(raised, mutableStateOf<Offset?>(Offset(1200f, 900f)))
        compose.setContent {
            Till {
                motion.raise.Attach()
                PhonePanel(panelOf(sale), "Front Counter", BubbleLook.LIT, page, actions, motion)
            }
        }
        compose.mainClock.advanceTimeBy(100)
        assertTrue(recompositions.count("com.monokulo.till.overlay.PanelCard") > 0, "the tracer sees the panel compose")
    }

    /** Starts a move, lets its first frames compose what it brings, then counts ten frames of it. */
    private fun assertNothingRecomposesMidMove(move: String, also: List<String> = emptyList(), start: () -> Unit) {
        start()
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
        recompositions.reset()
        repeat(10) { compose.mainClock.advanceTimeByFrame() }
        for (name in named + also) assertEquals(0, recompositions.count(name), "$move: $name recomposed mid-move")
    }

    @Test
    fun theRaise() {
        panel(raised = 0f)
        assertNothingRecomposesMidMove("raise") { motion.raise.enter() }
    }

    @Test
    fun theLower() {
        panel(raised = 1f)
        assertNothingRecomposesMidMove("lower") { motion.raise.exit() }
    }

    @Test
    fun thePaidFold() {
        panel(raised = 1f)
        assertNothingRecomposesMidMove("paid fold") { sale = testSale("r1", SalePhase.Paid, expiresAt = expires) }
    }

    @Test
    fun theOrdersGrow() {
        panel(raised = 1f)
        assertNothingRecomposesMidMove("order grow") { page = PanelPage.Order }
    }

    @Test
    fun theLateOrderLine() {
        panel(raised = 1f)
        assertNothingRecomposesMidMove("late order line") { sale = sale.copy(receipt = sampleReceipt()) }
    }

    private var look by mutableStateOf(BubbleLook.LIT)

    /** The free bubble alone, lit. */
    private fun bubble() {
        compose.mainClock.autoAdvance = false
        compose.setContent { Till { BubbleFace(look) } }
        compose.mainClock.advanceTimeBy(100)
        for (name in bubbleParts) assertTrue(recompositions.count(name) > 0, "the tracer sees $name compose")
    }

    private val bubbleParts = listOf("com.monokulo.till.overlay.BubbleFace", "com.monokulo.till.overlay.Circle", "androidx.compose.foundation.Canvas")

    /** Ten frames of the border's chase (and the pulse beside it) redraw the mark and recompose none of the bubble. */
    @Test
    fun theBubblesChase() {
        bubble()
        repeat(3) { compose.mainClock.advanceTimeByFrame() }
        recompositions.reset()
        repeat(10) { compose.mainClock.advanceTimeByFrame() }
        for (name in bubbleParts + named) assertEquals(0, recompositions.count(name), "chase: $name recomposed mid-turn")
    }

    /** Ten frames of the paid spread and the tick recompose none of the bubble: the look changed once, on its first frame. */
    @Test
    fun theBubblesPaidSpread() {
        bubble()
        assertNothingRecomposesMidMove("paid spread", bubbleParts) { look = BubbleLook.PAID }
    }

    /** Paid back to idle, the tick fading whole and the lens spreading to grey, recomposes none of the bubble (decision 293). */
    @Test
    fun theBubblesLeavingPaid() {
        bubble()
        look = BubbleLook.PAID
        compose.mainClock.advanceTimeBy(1_000)
        assertNothingRecomposesMidMove("paid to idle", bubbleParts) { look = BubbleLook.IDLE }
    }
}
