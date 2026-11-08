package com.monokulo.till.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.monokulo.till.Move
import com.monokulo.till.GroundLook
import com.monokulo.till.sharedGround
import com.monokulo.till.GroundTrack
import com.monokulo.till.rememberGroundTrack
import com.monokulo.till.sharedCode
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

/**
 * [text] as a QR code, error correction M, [scale] pixels a module with a
 * white border of [margin] modules. `QR_COMPACT` lets ZXing switch modes
 * inside the code, so a receipt's digits pack in numeric mode.
 */
fun qrBitmap(text: String, scale: Int = 1, margin: Int = 0): Bitmap {
    val hints = mapOf(
        EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
        EncodeHintType.MARGIN to 0,
        EncodeHintType.QR_COMPACT to true,
    )
    val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 0, 0, hints)
    val side = (matrix.width + 2 * margin) * scale
    val pixels = IntArray(side * side) { i ->
        val x = (i % side) / scale - margin
        val y = (i / side) / scale - margin
        val on = x in 0 until matrix.width && y in 0 until matrix.height && matrix.get(x, y)
        if (on) 0xFF1A1917.toInt() else 0xFFFFFFFF.toInt()
    }
    return Bitmap.createBitmap(pixels, side, side, Bitmap.Config.ARGB_8888)
}

/** How many codes [codeBitmap] keeps: the sale on screen, its panel and a couple that just ended. */
private const val KEEP = 4

/** The last [KEEP] codes drawn, by text, most recently used last. Read on the main thread, as composition is. */
private val codes = object : LinkedHashMap<String, ImageBitmap>(KEEP, 0.75f, true) {
    override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, ImageBitmap>): Boolean = size > KEEP
}

/**
 * [text] as a code to draw: the same bitmap for the same text while it's
 * one of the last [KEEP] asked for, so the sale screen, the enlarged code,
 * the panel and a shared element between them draw one image instead of
 * encoding it again (ZXing version 25 with a receipt in it) on every
 * screen change.
 */
fun codeBitmap(text: String): ImageBitmap = codes.getOrPut(text) { qrBitmap(text).asImageBitmap() }

/**
 * A payment code on its white card (the design's QR part): ink on white
 * whatever the theme, the card's padding its quiet zone. With [shared]
 * (the sale's order id) in the app, the code is one shared element
 * between the sale and the enlarged code (M8): the image moves and scales
 * as one layer over everything else that moves, never re-laid out, and
 * drawn whole on every frame; with [sharedCard] the card's white is the
 * ground that spreads into the enlarged code's white screen, the image
 * never inside it (decision 170); without, the code is on a ground
 * already and has no white of its own.
 */
@Composable
fun QrCode(
    text: String,
    description: String,
    modifier: Modifier = Modifier,
    padding: Dp = 20.dp,
    radius: Dp = 36.dp,
    shared: String? = null,
    sharedCard: Boolean = true,
    ground: GroundTrack? = null,
) {
    val bitmap = remember(text) { codeBitmap(text) }
    val shape = remember(radius) { RoundedCornerShape(radius) }
    CodeCard(modifier.aspectRatio(1f), shape, radius, shared, sharedCard, ground) {
        Box(Modifier.padding(padding)) { CodeImage(bitmap, description, shared) }
    }
}

/**
 * The white card: its white the code's ground with [sharedCard], none on
 * the enlarged code, which sits on the screen's own ground (one white,
 * never a second card drawn under the moving one), plain white with no
 * [shared]. Its own restartable scope, so a transition starting or
 * landing re-runs this and not [QrCode].
 */
@Composable
private fun CodeCard(modifier: Modifier, shape: Shape, radius: Dp, shared: String?, sharedCard: Boolean, ground: GroundTrack?, content: @Composable () -> Unit) = Box(
    modifier.clip(shape).then(if (shared == null) Modifier.background(Fixed.White) else Modifier),
) {
    if (shared != null && sharedCard) {
        Box(Modifier.fillMaxSize().sharedGround(codeGround(shared), Move.CodeGrow, GroundLook(Fixed.White, radius), ground))
    }
    content()
}

/** The key of the white a sale's code sits on, the card's and the enlarged screen's. */
fun codeGround(orderId: String) = "code-card/$orderId"

/** The sale's code card's corners. */
val CodeCorner = 32.dp

/** The enlarged code's ground: the whole screen, square. */
val CodeScreenCorner = 0.dp

/** The code's one bitmap, scaled to fit with no smoothing; one shared layer over the rest when [shared]. */
@Composable
private fun CodeImage(bitmap: ImageBitmap, description: String, shared: String?) = Image(
    bitmap = bitmap,
    contentDescription = null,
    modifier = Modifier.fillMaxSize()
        .then(if (shared != null) Modifier.sharedCode("code/$shared", Move.CodeGrow, rememberGroundTrack("code/$shared"), bitmap, white = codeGround(shared)) else Modifier)
        .semantics { contentDescription = description },
    contentScale = ContentScale.Fit,
    filterQuality = FilterQuality.None,
)
