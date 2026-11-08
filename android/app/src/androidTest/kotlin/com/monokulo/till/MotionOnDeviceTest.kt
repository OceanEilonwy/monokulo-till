package com.monokulo.till

import android.provider.Settings
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.codeBitmap
import com.monokulo.till.ui.motion
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertNotSame
import kotlin.test.assertSame

/** What the motion foundation does with a real window: the theme reads the system's switch, and a code is encoded once. */
class MotionOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theThemeProvidesTheMotionTheSystemAsksFor() {
        val resolver = InstrumentationRegistry.getInstrumentation().targetContext.contentResolver
        val off = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
        var seen: Motion? = null
        compose.setContent { TillTheme { seen = motion } }
        compose.waitForIdle()
        assertSame(if (off) Motion.Still else Motion.Full, seen)
    }

    @Test
    fun theSameCodeIsOneBitmapUntilOthersPushItOut() {
        val first = codeBitmap("monero:4abc?tx_amount=0.1")
        assertSame(first, codeBitmap("monero:4abc?tx_amount=0.1"), "the sale screen, the panel and the enlarged code draw one image")
        repeat(4) { codeBitmap("monero:4abc?tx_amount=0.${it + 2}") }
        assertNotSame(first, codeBitmap("monero:4abc?tx_amount=0.1"), "only the last few codes are kept")
    }
}
