package com.monokulo.till

import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.TillActions
import com.monokulo.till.ui.TillStatus
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals

/** Amendment 10: one clock per screen, and a tick recomposes the texts that show it and nothing above them. */
class OneClockTest {
    @get:Rule
    val compose = createComposeRule()

    private val recompositions = Recompositions()

    @After
    fun stopCounting() = recompositions.close()

    @Test
    fun aTickRecomposesEachLiveRowOnceAndNotTheShift() {
        val now = mutableLongStateOf(1_000)
        val sales = List(6) { testSale("live$it", expiresAt = 1_900L + it) } +
            List(6) { testSale("paid$it", phase = com.monokulo.till.core.SalePhase.Paid, paidAt = 990) }
        compose.setContent { Till { ShiftScreen(status, sales, noActions, now) } }
        compose.waitForIdle()
        assertEquals(true, recompositions.count("androidx.compose.animation.AnimatedContent") > 0, "the tracer sees the badges' transitions")
        recompositions.reset()

        repeat(5) {
            now.longValue++
            compose.waitForIdle()
        }

        // The six paid rows are composed too, but only the live ones are on screen first; count those that read the clock.
        assertEquals(0, recompositions.count("com.monokulo.till.ui.ShiftScreen"), "the shift doesn't tick")
        val rows = recompositions.count("com.monokulo.till.ui.SaleRow")
        assertEquals(0, rows % 5, "each live row on screen once a tick, nothing else: $rows")
        assertEquals(true, rows in 5..30, "only the live rows: $rows")
        // A row's badge transition is called only when its glyph changes, never by the row's countdown.
        assertEquals(0, recompositions.count("com.monokulo.till.ui.RowBadge"), "the badges don't tick")
        assertEquals(0, recompositions.count("androidx.compose.animation.AnimatedContent"), "nor their transitions")
    }

    @Test
    fun aTickRecomposesTheSaleScreensCountdownAndNotTheFootOrTheCode() {
        val now = mutableLongStateOf(1_000)
        val sale = testSale("a", expiresAt = 1_900)
        compose.setContent { Till { SaleScreen(sale, "Front", refunding = false, refundProblem = null, refundSheet = null, actions = noActions, now = now) } }
        compose.waitForIdle()
        recompositions.reset()

        repeat(5) {
            now.longValue++
            compose.waitForIdle()
        }

        assertEquals(5, recompositions.count("com.monokulo.till.ui.Countdown"), "the foot's second line, once a tick")
        for (name in listOf("SaleScreen", "LiveSale", "StatusFoot", "StatusLine", "RingTick", "QrCode", "PhaseRing")) {
            assertEquals(0, recompositions.count("com.monokulo.till.ui.$name"), "$name doesn't tick")
        }
    }

    companion object {
        val status = TillStatus(
            store = "Corner shop", location = "Front", running = true, live = true,
            disconnected = false, problem = null, signInAgain = false, startedAt = 900,
        )

        val noActions = TillActions(
            onSettings = {}, onOpen = {}, onBack = {}, onEnlarge = {}, onShowOrder = {}, onClose = { _, _ -> },
            onRefund = {}, onReopen = {}, onCopy = {}, onCopyXmr = {}, onBackToPos = {}, onRefundDone = {}, onRefundSheetDismissed = {}, onSignIn = {},
            onForget = {}, onStartShift = {},
        )
    }
}
