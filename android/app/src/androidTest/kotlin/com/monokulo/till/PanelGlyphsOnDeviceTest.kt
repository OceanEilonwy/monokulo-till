package com.monokulo.till

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.overlay.FloatingPanel
import com.monokulo.till.overlay.PanelActions
import com.monokulo.till.overlay.PanelMotion
import com.monokulo.till.overlay.PanelPage
import com.monokulo.till.overlay.PhonePanel
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.mk
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.FileInputStream
import kotlin.test.assertTrue

/**
 * The panel growing out of the bubble and collapsing into it draws its
 * words at one size (decision 210): its content is one layer while the
 * raise scales it, so no glyph is rasterised at the scales the grow passes
 * through. Read from the renderer's own glyph count (`dumpsys gfxinfo`),
 * with every frame of three lowers and raises drawn: text drawn through
 * the scale adds a strike of every glyph on the card at each frame. The
 * renderer's cache is the process's, so the two tests pass through
 * different scales (the tablet's lower is turned back halfway) and the
 * second can't find the first's strikes already there.
 */
class PanelGlyphsOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    private val actions = PanelActions({}, {}, {}, {}, {}, {}, {}, {})
    private val sale = testSale("g1", expiresAt = System.currentTimeMillis() / 1000 + 15 * 60)

    /** The renderer's cached glyphs for this process, or null where `gfxinfo` doesn't say. */
    private fun glyphs(): Int? {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val out = instrumentation.uiAutomation.executeShellCommand("dumpsys gfxinfo ${instrumentation.targetContext.packageName}")
        val text = FileInputStream(out.fileDescriptor).use { it.readBytes().decodeToString() }
        out.close()
        return Regex("Glyph Count: (\\d+)").find(text)?.groupValues?.get(1)?.toInt()
    }

    /** Every frame drawn, as a screen would, for [ms] of the test clock. */
    private fun drawFor(ms: Long) {
        var at = 0L
        while (at < ms) {
            compose.mainClock.advanceTimeByFrame()
            compose.onNodeWithTag("window").captureToImage()
            at += 16
        }
    }

    private fun raisesAndLowersAtOneSize(tablet: Boolean, lowerMs: Long) {
        compose.mainClock.autoAdvance = false
        val motion = PanelMotion(1f, mutableStateOf<Offset?>(Offset(1200f, 900f)))
        compose.setContent {
            Till(Motion.Full) {
                motion.raise.Attach()
                Box(Modifier.fillMaxSize().background(mk.danger).testTag("window")) {
                    if (tablet) {
                        FloatingPanel(sale, "Front Counter", PanelPage.Code, actions, motion)
                    } else {
                        PhonePanel(sale, "Front Counter", BubbleLook.Lit, PanelPage.Code, actions, motion)
                    }
                }
            }
        }
        compose.mainClock.advanceTimeBy(100)
        drawFor(100)
        val atRest = glyphs()
        assumeTrue("this renderer reports no glyph count", atRest != null)
        repeat(3) {
            motion.raise.exit()
            drawFor(lowerMs)
            motion.raise.enter()
            drawFor(600)
        }
        val after = glyphs()!!
        // The countdown's digits may add a few; a strike per frame adds the whole card's glyphs again and again.
        assertTrue(after - atRest!! <= 40, "three lowers and raises added ${after - atRest} glyphs to the $atRest at rest")
    }

    @Test
    fun thePhonePanelsWordsAreDrawnAtOneSizeThroughItsRaiseAndLower() = raisesAndLowersAtOneSize(tablet = false, lowerMs = 600)

    @Test
    fun theFloatingPanelsWordsAreDrawnAtOneSizeThroughItsRaiseAndLower() = raisesAndLowersAtOneSize(tablet = true, lowerMs = 200)
}
