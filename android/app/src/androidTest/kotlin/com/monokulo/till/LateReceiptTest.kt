package com.monokulo.till

import android.graphics.Bitmap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import com.monokulo.till.OneClockTest.Companion.noActions
import com.monokulo.till.ui.SaleScreen
import org.junit.After
import org.junit.Rule
import org.junit.Test
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * A sale's receipt reaches the till about a second after the sale (N3):
 * the order card fades in over the sale, the header's second line fades
 * through to the till's own words, and neither the code nor the status
 * pill is recomposed by the fade's frames.
 */
class LateReceiptTest {
    @get:Rule
    val compose = createComposeRule()

    private val recompositions = Recompositions()

    @After
    fun stopCounting() = recompositions.close()

    @Test
    fun aLateReceiptFadesItsCardInAndRecomposesNeitherTheCodeNorThePill() {
        val rung = testSale("a1", expiresAt = 1_600)
        // As Square first tells it: the device it was rung up on, no receipt yet.
        var sale by mutableStateOf(rung.copy(found = null, receipt = null, deviceName = "Register 2"))
        val now = mutableLongStateOf(1_000)
        compose.setContent { Till { SaleScreen(sale, "Front", refunding = false, refundProblem = null, refundSheet = null, actions = noActions, now = now) } }
        compose.waitForIdle()
        compose.onNodeWithText("From Register 2").assertExists()
        compose.mainClock.autoAdvance = false

        sale = rung.copy(receipt = sampleReceipt())
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        // The arrival itself recomposes what shows the receipt; the fade's frames recompose nothing.
        recompositions.reset()
        val label = compose.onNodeWithText("receipt k4Rt", substring = true, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        // Inside the card, beside its folded row's words: the card's own ground once it has faded in.
        val x = (label.left - with(compose.density) { 8f * density }).roundToInt()
        val y = label.center.y.roundToInt()
        fun card(): Int = compose.onRoot().captureToImage().asAndroidBitmap().copy(Bitmap.Config.ARGB_8888, false).getPixel(x, y)
        val start = card()
        compose.mainClock.advanceTimeBy(100)
        val mid = card()
        compose.mainClock.advanceTimeBy(250)
        val end = card()

        assertTrue(far(start, end), "the card isn't there at once: ${hex(start)} -> ${hex(end)}")
        assertTrue(far(mid, start) && far(mid, end), "halfway it is fading in: ${hex(start)}, ${hex(mid)}, ${hex(end)}")
        for (name in listOf("QrCode", "StatusFoot", "StatusLine", "RingTick", "PhaseRing")) {
            assertEquals(0, recompositions.count("com.monokulo.till.ui.$name"), "$name isn't recomposed by the fade")
        }
        compose.onNodeWithText("Front · this till").assertExists()
        compose.onAllNodesWithText("From Register 2").assertCountEquals(0)
    }

    private fun far(a: Int, b: Int) =
        abs((a shr 16 and 0xff) - (b shr 16 and 0xff)) + abs((a shr 8 and 0xff) - (b shr 8 and 0xff)) + abs((a and 0xff) - (b and 0xff)) > 3

    private fun hex(c: Int) = Integer.toHexString(c)
}
