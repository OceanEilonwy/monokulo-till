package com.monokulo.till

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.StatusFoot
import org.junit.Rule
import org.junit.Test
import kotlin.math.abs
import kotlin.test.assertTrue

/** Board 8: on a 412 dp phone the code card is 372 dp, and its status is the card's foot, right under the code. */
class CodeCardTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun onAPhoneTheCodeIsTheWindowLessTwentyAndItsStatusIsTheCardsFoot() {
        val now = mutableLongStateOf(1_000)
        compose.setContent {
            Till {
                Box(Modifier.size(412.dp, 915.dp)) {
                    SaleScreen(saleFace(testSale("a", expiresAt = 1_065), now = 1_000), noActions)
                }
            }
        }
        compose.waitForIdle()
        // The code image, inside the card's 18 dp of quiet zone each side.
        val code = compose.onNodeWithContentDescription("Payment code for", substring = true, useUnmergedTree = true).getBoundsInRoot()
        assertNear(336.dp, code.right - code.left, "the code's card is 412 dp less 20 dp a side: $code")
        assertNear(336.dp, code.bottom - code.top, "and square: $code")
        val words = compose.onNodeWithText("Waiting for payment", useUnmergedTree = true).getBoundsInRoot()
        val hint = compose.onNodeWithText("Scan with any Monero wallet · 1:05 left", useUnmergedTree = true).getBoundsInRoot()
        // The foot: under the card's quiet zone, a 1 dp line and 12 dp; its words 14 dp, the 30 dp ring and 12 dp in.
        val foot = code.bottom + 18.dp + 1.dp + 12.dp
        assertTrue(words.top >= foot - 1.dp && words.top < foot + 12.dp, "the words are the foot under the code: $words under $code")
        assertNear(code.left - 18.dp + 56.dp, words.left, "after the ring")
        assertTrue(hint.top >= words.bottom - 1.dp, "the hint under the words")
    }

    private fun assertNear(expected: Dp, actual: Dp, why: String) =
        assertTrue(abs((expected - actual).value) < 1f, "$why: $actual, not $expected")

    @Test
    fun paidTheFootSaysWhatCameIn() {
        val sale = testSale("a", SalePhase.PAID, paidAt = 990)
        val now = mutableLongStateOf(1_000)
        compose.setContent { Till { StatusFoot(saleFace(sale).view) } }
        compose.onNodeWithText("Paid", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("${sale.xmr} XMR received", useUnmergedTree = true).assertExists()
    }
}
