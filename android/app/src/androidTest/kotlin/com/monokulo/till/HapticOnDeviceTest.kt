package com.monokulo.till

import android.content.Context
import android.view.View
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.core.SalePhase
import com.monokulo.till.overlay.BubbleFace
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals

/** One buzz per payment, from the surface the cashier is looking at (`docs/motion.md` §6, decision 176). */
class HapticOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    private class Counting(context: Context) : View(context) {
        var buzzes = 0
        override fun performHapticFeedback(feedbackConstant: Int): Boolean {
            buzzes++
            return true
        }
    }

    private var look by mutableStateOf(BubbleLook.Lit)

    private fun buzzesForAPayment(appVisible: Boolean): Int {
        val view = Counting(InstrumentationRegistry.getInstrumentation().targetContext)
        compose.setContent {
            Till {
                CompositionLocalProvider(LocalView provides view) {
                    BubbleFace(look, phase = SalePhase.Confirming, confirms = PaidBuzz.bubble(appVisible))
                }
            }
        }
        compose.waitForIdle()
        look = BubbleLook.Paid
        compose.waitForIdle()
        // The same payment seen again (a poll, the free bubble taking over) is not a second buzz.
        look = BubbleLook.Paid
        compose.mainClock.advanceTimeBy(1_000)
        return view.buzzes
    }

    @Test
    fun withTheAppAwayTheBubbleBuzzesOnce() {
        assertEquals(1, buzzesForAPayment(appVisible = false))
    }

    @Test
    fun withTheAppInFrontTheBubbleLeavesTheBuzzToTheSaleScreen() {
        assertEquals(0, buzzesForAPayment(appVisible = true))
    }
}
