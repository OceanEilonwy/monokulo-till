package com.monokulo.till.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/** The design's line icons, from its SVG paths. Tinted by `Icon`, so drawn in black here. */
object MkIcons {
    private fun icon(name: String, box: Float, vararg strokes: Pair<String, Float>, fills: List<String> = emptyList()) =
        ImageVector.Builder(name, box.dp, box.dp, box, box).apply {
            strokes.forEach { (d, width) ->
                addPath(
                    PathParser().parsePathString(d).toNodes(),
                    stroke = SolidColor(Color.Black),
                    strokeLineWidth = width,
                    strokeLineCap = StrokeCap.Round,
                    strokeLineJoin = StrokeJoin.Round,
                )
            }
            fills.forEach { d -> addPath(PathParser().parsePathString(d).toNodes(), fill = SolidColor(Color.Black)) }
        }.build()

    val Back = icon("back", 24f, "M20 12H5M11 5l-7 7 7 7" to 2.2f)
    /** V2's ways to enter a wallet: a code in a frame, a key, and lines of words (the boards' VkWallet). */
    val WalletCode = icon(
        "wallet code", 24f,
        "M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3" to 1.8f,
        "M9 8h6a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1Z" to 1.8f,
    )
    val Key = icon("key", 24f, "M8 8a4 4 0 1 0 0.01 0Z" to 1.8f, "M12 12h9M18 12v3M21 12v2" to 1.8f)
    val SeedWords = icon("seed words", 24f, "M5 6h14M5 10h14M5 14h9M5 18h6" to 1.8f)
    /** A chevron to the right: a row or a line that opens something. */
    val Chevron = icon("chevron", 24f, "M9 5l7 7-7 7" to 2.4f)
    /** A chevron pointing down: the waiting tray's fold, turned over when it is open. */
    val Fold = icon("fold", 24f, "M6 9l6 6 6-6" to 2.2f)
    /** A cog: eight teeth round a hole. */
    val Settings = icon(
        "settings", 24f,
        "M9.92 5.21L10.5 2.52A9.6 9.6 0 0 1 13.5 2.52L14.08 5.21A7.1 7.1 0 0 1 15.33 5.73L17.64 4.23A9.6 9.6 0 0 1 19.77 6.36L18.27 8.67A7.1 7.1 0 0 1 18.79 9.92L21.48 10.5A9.6 9.6 0 0 1 21.48 13.5L18.79 14.08A7.1 7.1 0 0 1 18.27 15.33L19.77 17.64A9.6 9.6 0 0 1 17.64 19.77L15.33 18.27A7.1 7.1 0 0 1 14.08 18.79L13.5 21.48A9.6 9.6 0 0 1 10.5 21.48L9.92 18.79A7.1 7.1 0 0 1 8.67 18.27L6.36 19.77A9.6 9.6 0 0 1 4.23 17.64L5.73 15.33A7.1 7.1 0 0 1 5.21 14.08L2.52 13.5A9.6 9.6 0 0 1 2.52 10.5L5.21 9.92A7.1 7.1 0 0 1 5.73 8.67L4.23 6.36A9.6 9.6 0 0 1 6.36 4.23L8.67 5.73A7.1 7.1 0 0 1 9.92 5.21Z" to 1.8f,
        "M12 8.9a3.1 3.1 0 1 0 0.01 0Z" to 1.8f,
    )
    val Bag = icon("bag", 24f, "M4 7h16l-1.5 12.5a1 1 0 0 1-1 .9H6.5a1 1 0 0 1-1-.9L4 7Z M9 7V5.5a3 3 0 0 1 6 0V7" to 1.8f)
    val Code = icon("code", 24f, "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h2v2h-2z M18 18h2v2h-2z M14 18h2 M18 14h2" to 1.8f)
    val CheckCircle = icon("check circle", 24f, "M12 3a9 9 0 1 0 0.01 0Z" to 1.8f, "M8 12.3l2.6 2.6L16 9.5" to 1.8f)
    val Tick = icon("tick", 24f, "M5.5 12.5l4 4L18.5 7.5" to 2.6f)
    val Lock = icon("lock", 24f, "M7 10h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2Z" to 2.4f, "M8 10V7a4 4 0 0 1 8 0v3" to 2.4f)
    val Copy = icon(
        "copy", 24f,
        "M10.5 8h7a2.5 2.5 0 0 1 2.5 2.5v7a2.5 2.5 0 0 1-2.5 2.5h-7A2.5 2.5 0 0 1 8 17.5v-7A2.5 2.5 0 0 1 10.5 8Z" to 2f,
        "M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" to 2f,
    )
    val Clock = icon("clock", 24f, "M12 3a9 9 0 1 0 0.01 0Z" to 2f, "M12 7v5.5l3.5 2" to 2f)
    val Bell = icon("bell", 24f, "M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15L6 16Z M10 20.5a2 2 0 0 0 4 0" to 1.8f)
    /** An X: the tinted button that opens the close sheet, and a sale closed on another till. */
    val Close = icon("close", 14f, "M2 2l10 10M12 2L2 12" to 2.2f)
    /** An arrow turning back: a refund at the point of sale. */
    val Refund = icon("refund", 24f, "M9 14l-4-4 4-4M5 10h9a5 5 0 0 1 0 10h-3" to 2.2f)
    /** A triangle with a mark: a reversed payment. */
    val Warning = icon("warning", 24f, "M12 4l9 16H3z M12 10v4.5 M12 17.2v.1" to 2f)
    /** A closed box: "close it for now". */
    val Tray = icon("tray", 24f, "M6.5 5h11A2.5 2.5 0 0 1 20 7.5v9a2.5 2.5 0 0 1-2.5 2.5h-11A2.5 2.5 0 0 1 4 16.5v-9A2.5 2.5 0 0 1 6.5 5Z M4 10h16" to 2f)
    /** Arrows out to the corners: enlarge the code. */
    val Expand = icon("expand", 24f, "M14 4h6v6M10 20H4v-6M20 4l-6 6M4 20l6-6" to 2.2f)
    /** A banknote: a cash sale offered in Monero, kept as cash or paid in cash (core.md decision 126). */
    val Cash = icon("cash", 24f, "M4.5 6.5h15a1.5 1.5 0 0 1 1.5 1.5v8a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 16V8a1.5 1.5 0 0 1 1.5-1.5Z" to 2f, "M12 9.5a2.5 2.5 0 1 0 0.01 0Z M6.5 12h.1 M17.4 12h.1" to 2f)
    /** Arrows in from the corners: shrink it. */
    /** A circular arrow: start again (New shift), as iOS's arrow.counterclockwise. */
    val Reset = icon("reset", 24f, "M4.5 12a7.5 7.5 0 1 0 2.4-5.5" to 2.2f, "M4 3.5V8h4.5" to 2.2f)

    /** A phone or tablet: this device, in the counter step. */
    val Device = icon("device", 24f, "M8 3h8a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z M11 18h2" to 1.8f)

    /** A shop front: the whole location, in the counter step. */
    val Store = icon("store", 24f, "M4 9l1.5-5h13L20 9 M4 9h16v1.5a2.7 2.7 0 0 1-5.3 0 2.7 2.7 0 0 1-5.4 0 2.7 2.7 0 0 1-5.3 0Z M5.5 12.5V20h13v-7.5 M10 20v-4.5h4V20" to 1.8f)

    /** The keypad's delete key: a tag pointing left with a cross in it, as Monokulo's web POS draws it. */
    val Backspace = icon("delete", 24f, "M8.5 5h11a1.5 1.5 0 0 1 1.5 1.5v11a1.5 1.5 0 0 1-1.5 1.5h-11L3 12Z" to 2f, "M11.5 9.5l5 5M16.5 9.5l-5 5" to 2f)
    val Shrink = icon("shrink", 24f, "M4 10h6V4M20 14h-6v6M10 10L3 3M14 14l7 7" to 2.2f)
}
