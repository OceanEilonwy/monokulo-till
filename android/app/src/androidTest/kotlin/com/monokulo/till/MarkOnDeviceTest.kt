package com.monokulo.till

import android.os.Debug
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.LensLook
import com.monokulo.till.ui.till
import org.junit.Test
import kotlin.test.assertTrue

/**
 * The bubble's terminal on a real canvas (`docs/motion.md` §12.5, §13.5): a
 * frame of it allocates nothing, so the running segment, the spread and
 * the tick can run at the display's rate without the collector.
 */
class MarkOnDeviceTest {
    @Test
    @Suppress("DEPRECATION")
    fun aFrameOfTheTerminalAllocatesNothing() {
        val image = ImageBitmap(114, 114)
        val canvas = Canvas(image)
        val scope = CanvasDrawScope()
        val density = Density(3f)
        val size = Size(114f, 114f)
        // The fill part-way, the running segment wrapping past 12 o'clock, half a tick, faded: every path the mark draws.
        val mark: DrawScope.() -> Unit = { till(LensLook.Orange, Fixed.TerminalDim, 0.4f, Fixed.Accent, 0.9f, 0.6f, 0.5f, 0.7f) }
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
        assertTrue(drawn <= base, "100 frames of the terminal (screen, border, fill, segment, half a tick, faded) allocated ${drawn - base} objects more than 100 empty frames")
        assertTrue(image.asAndroidBitmap().getPixel(57, 57) != 0, "and the mark was drawn")
    }
}
