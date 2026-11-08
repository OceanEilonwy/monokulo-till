package com.monokulo.till

import android.os.Debug
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.LensLook
import com.monokulo.till.ui.Logo
import com.monokulo.till.ui.mark
import org.junit.Rule
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The bubble's mark on a real canvas (`docs/motion.md` §12.5, §13.5): it
 * is the logo's own drawing, and a frame of it allocates nothing, so the
 * chase, the spread and the tick can run at the display's rate without
 * the collector.
 */
class MarkOnDeviceTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    @Suppress("DEPRECATION")
    fun aFrameOfTheMarkAllocatesNothing() {
        val image = ImageBitmap(114, 114)
        val canvas = Canvas(image)
        val scope = CanvasDrawScope()
        val density = Density(3f)
        val size = Size(114f, 114f)
        val mark: DrawScope.() -> Unit = { mark(LensLook.Orange, Fixed.ChaseTrack, 123f, 0.6f, 0.5f, 0.7f) }
        val empty: DrawScope.() -> Unit = { }
        fun allocations(draw: DrawScope.() -> Unit): Int {
            // Warmed first, so the mark's paths and strokes, made once, are made before the count.
            repeat(5) { scope.draw(density, LayoutDirection.Ltr, canvas, size, draw) }
            Debug.resetThreadAllocCount()
            Debug.startAllocCounting()
            repeat(100) { scope.draw(density, LayoutDirection.Ltr, canvas, size, draw) }
            Debug.stopAllocCounting()
            return Debug.getThreadAllocCount()
        }
        // The counter counts: a frame that makes an object is seen.
        var sink: Any? = null
        val made = allocations { sink = FloatArray(1) }
        assertTrue(made >= 100 && sink != null, "the counter sees an array made a frame: $made")
        val base = allocations(empty)
        val drawn = allocations(mark)
        assertTrue(drawn <= base, "100 frames of the mark (lens, border, arc, half a tick, faded) allocated ${drawn - base} objects more than 100 empty frames")
        assertTrue(image.asAndroidBitmap().getPixel(57, 57) != 0, "and the mark was drawn")
    }

    /** The idle bubble's mark is the grey logo: the same drawing, by the bubble's entry. */
    @Test
    fun theBubblesEntryDrawsTheLogo() {
        compose.setContent {
            Row {
                Logo(38.dp, Modifier.testTag("logo"), ring = Fixed.IdleBorder, grey = true)
                Canvas(Modifier.size(38.dp).testTag("mark")) { mark(LensLook.Grey, Fixed.IdleBorder, 0f, 0f, 0f, 0f) }
            }
        }
        val logo = compose.onNodeWithTag("logo").captureToImage().asAndroidBitmap()
        val mark = compose.onNodeWithTag("mark").captureToImage().asAndroidBitmap()
        assertEquals(logo.width, mark.width)
        assertTrue(logo.sameAs(mark), "the bubble's mark and the logo differ")
    }
}
