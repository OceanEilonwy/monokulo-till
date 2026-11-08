package com.monokulo.till

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.IntSize
import com.monokulo.till.overlay.ORDER_TAG
import com.monokulo.till.overlay.PanelActions
import com.monokulo.till.overlay.PanelMotion
import com.monokulo.till.overlay.PanelPage
import com.monokulo.till.overlay.PhonePanel
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.mk
import org.junit.Rule
import org.junit.Test
import kotlin.math.abs
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import uniffi.till_screens.BubbleLook

/**
 * The phone panel's order growing out of its code card (M5, decision 173):
 * the order is laid out once at the window's size and only its clip moves,
 * from the card's rounded bounds to the whole window, square at the end.
 */
class PhoneOrderOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    private var page by mutableStateOf<PanelPage>(PanelPage.Code)
    private val actions = PanelActions({}, {}, {}, {}, {}, {}, {}, {})
    private var backdrop = Color.Unspecified
    private var ground = Color.Unspecified

    private val raised = PanelMotion(1f, mutableStateOf<Offset?>(null))

    private fun panel(motion: Motion) {
        compose.mainClock.autoAdvance = false
        val sale = testSale("o1")
        compose.setContent {
            Till(motion) {
                // A colour that is no part of the panel behind it, so a frame shows where the container is.
                backdrop = mk.danger
                ground = mk.ground
                Box(Modifier.fillMaxSize().background(backdrop).testTag("window")) {
                    PhonePanel(panelOf(sale), "Front Counter", BubbleLook.LIT, page, actions, raised)
                }
            }
        }
        compose.mainClock.advanceTimeBy(500)
    }

    private fun Color.near(other: Color) =
        abs(red - other.red) < 0.05f && abs(green - other.green) < 0.05f && abs(blue - other.blue) < 0.05f

    /** The pixel 4 px in from the window's top-left corner, where the card's rounded corner never reaches. */
    private fun corner(): Color = compose.onNodeWithTag("window").captureToImage().toPixelMap()[4, 4]

    @Test
    fun theOrderGrowsFromTheCardToTheWholeWindowAndIsLaidOutOnce() {
        panel(Motion.Full)
        val window = compose.onNodeWithTag("window").fetchSemanticsNode().size
        page = PanelPage.Order
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        val sizes = mutableSetOf<IntSize>()
        var cornerFilledAt = -1
        repeat(40) { frame ->
            compose.mainClock.advanceTimeBy(16)
            sizes += compose.onNodeWithTag(ORDER_TAG, useUnmergedTree = true).fetchSemanticsNode().size
            if (cornerFilledAt < 0 && corner().near(ground)) cornerFilledAt = frame
        }
        assertEquals(1, sizes.size, "the order keeps one size on every frame: $sizes")
        assertTrue(cornerFilledAt > 2, "the window's corner is not the order's ground until the container reaches it (frame $cornerFilledAt)")
        assertTrue(corner().near(ground), "at rest the order fills the window, its corners square")
        assertEquals(window, sizes.single(), "laid out at the window's size")
    }

    /** The top of the order's heading on screen, where it is placed on this frame. */
    private fun headingTop(): Float = compose.onNodeWithText("The order", useUnmergedTree = true).fetchSemanticsNode().positionOnScreen.y

    @Test
    fun theOrdersHeadingRidesTheGrowingCardsTopSoTheCardIsNeverAnEmptyGround() {
        panel(Motion.Full)
        val card = assertNotNull(raised.card, "the code's card is placed")
        page = PanelPage.Order
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        // The heading is inside the container (at or below the card's top, the clip's top at the start) from the
        // grow's first frames, while the order's fade is under way, instead of waiting above the clip.
        val early = headingTop()
        assertTrue(early >= card.top, "the heading starts at the card's top ($early), not above it (${card.top})")
        var last = early
        repeat(40) {
            compose.mainClock.advanceTimeBy(16)
            val top = headingTop()
            assertTrue(top <= last + 0.5f, "the heading only rises with the container's top: $last then $top")
            last = top
        }
        assertTrue(last < card.top, "at rest the heading is at the top of the window it was laid out for ($last)")
    }

    @Test
    fun underStillMotionTheOrderIsAcrossfadeWithNoGrow() {
        panel(Motion.Still)
        page = PanelPage.Order
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        assertTrue(corner().near(ground), "the container is at the window at once")
        compose.mainClock.advanceTimeBy(300)
        assertTrue(corner().near(ground))
    }
}
