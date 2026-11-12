package com.monokulo.till.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp

/** Where a sale is, as the Till logo shows it. */
enum class TillState { IDLE, WAITING, SEEN, CONFIRMING, PAID }

/**
 * The Till logo (`TillWidget`, from Monokulo's branding canvas, as iOS's
 * TillLogo.swift draws it): the terminal with its screen, three keys and the
 * payment's bar, on its 104-unit box. It is Till's own mark, for Welcome, the
 * empty till and wherever Till stands beside a point of sale; the round
 * Monokulo mark ([Logo]) stays for the Monokulo store. Drawn still: the keys
 * and bar say the state without moving. [label] null when it's decoration.
 */
@Composable
fun TillWidget(size: Dp, modifier: Modifier = Modifier, state: TillState = TillState.IDLE, label: String? = null) {
    val dark = mk.ground.luminance() < 0.5f
    val colours = TillColours.of(state, dark)
    val described = if (label != null) Modifier.semantics { contentDescription = label } else Modifier
    Canvas(modifier.size(size).then(described)) {
        val unit = this.size.minDimension / BOX
        scale(unit, unit, pivot = androidx.compose.ui.geometry.Offset.Zero) {
            translate(-ORIGIN, -ORIGIN) {
                drawPath(TillWidgetPaths.body, colours.body)
                TillWidgetPaths.keys.forEachIndexed { i, key -> drawPath(key, colours.keys[i]) }
                drawPath(TillWidgetPaths.bar, colours.dim)
                if (colours.progress > 0f) {
                    clipPath(TillWidgetPaths.bar) {
                        drawRect(colours.barFill, topLeft = androidx.compose.ui.geometry.Offset(24f, 88f), size = androidx.compose.ui.geometry.Size(72f * colours.progress, 7f))
                    }
                }
                clipPath(TillWidgetPaths.screen) {
                    drawPath(TillWidgetPaths.screen, colours.mid)
                    drawPath(TillWidgetPaths.facets, colours.warm)
                    drawPath(TillWidgetPaths.centre, colours.deep)
                    if (state == TillState.PAID) {
                        drawPath(TillWidgetPaths.tick, Color.White, style = Stroke(width = 5f, cap = StrokeCap.Round, join = StrokeJoin.Round))
                    }
                }
                drawPath(TillWidgetPaths.outline, colours.outline, style = Stroke(width = 5f))
            }
        }
    }
}

private const val BOX = 104f
private const val ORIGIN = 8f

private object TillWidgetPaths {
    private fun p(d: String): Path = PathParser().parsePathString(d).toPath()

    val body = p("M30,12H90A18,18 0 0 1 108,30V90A18,18 0 0 1 90,108H30A18,18 0 0 1 12,90V30A18,18 0 0 1 30,12Z")
    val keys = listOf(
        p("M83,28H93A3,3 0 0 1 96,31V36A3,3 0 0 1 93,39H83A3,3 0 0 1 80,36V31A3,3 0 0 1 83,28Z"),
        p("M83,45.5H93A3,3 0 0 1 96,48.5V53.5A3,3 0 0 1 93,56.5H83A3,3 0 0 1 80,53.5V48.5A3,3 0 0 1 83,45.5Z"),
        p("M83,63H93A3,3 0 0 1 96,66V71A3,3 0 0 1 93,74H83A3,3 0 0 1 80,71V66A3,3 0 0 1 83,63Z"),
    )
    val bar = p("M27.5,88H92.5A3.5,3.5 0 0 1 96,91.5V91.5A3.5,3.5 0 0 1 92.5,95H27.5A3.5,3.5 0 0 1 24,91.5V91.5A3.5,3.5 0 0 1 27.5,88Z")
    val screen = p("M33.0,29.5H63.0A6.5,6.5 0 0 1 69.5,36.0V66.0A6.5,6.5 0 0 1 63.0,72.5H33.0A6.5,6.5 0 0 1 26.5,66.0V36.0A6.5,6.5 0 0 1 33.0,29.5Z")
    val facets = p("M43,46H53L48,39.5ZM53,46V56L59.5,51ZM53,56H43L48,62.5ZM43,56V46L36.5,51ZM48,39.5L26.5,29.5H69.5ZM59.5,51L69.5,29.5V72.5ZM48,62.5L69.5,72.5H26.5ZM36.5,51L26.5,72.5V29.5Z")
    val centre = p("M43,46H53V56H43Z")
    val tick = p("M37.5,51.5 l7,7 l14,-15")
    val outline = p("M33,27H63A9,9 0 0 1 72,36V66A9,9 0 0 1 63,75H33A9,9 0 0 1 24,66V36A9,9 0 0 1 33,27Z")
}

/** The logo's colours for a state, as the branding canvas picks them. */
private class TillColours(
    val body: Color,
    val outline: Color,
    val dim: Color,
    val keys: List<Color>,
    val mid: Color,
    val warm: Color,
    val deep: Color,
    val barFill: Color,
    val progress: Float,
) {
    companion object {
        private val Orange = Color(0xFFFF6600)
        private val Green = Color(0xFF2EA052)

        fun of(state: TillState, dark: Boolean): TillColours {
            val dim = if (dark) Color(0xFF66615A) else Color(0xFF4A4740)
            val paid = state == TillState.PAID
            val step = when (state) {
                TillState.SEEN -> 1
                TillState.CONFIRMING -> 2
                TillState.PAID -> 3
                else -> 0
            }
            val (mid, warm, deep) = when (state) {
                TillState.PAID -> Triple(Color(0xFF2EA052), Color(0xFF238A43), Color(0xFF1A7F37))
                TillState.IDLE -> Triple(Color(0xFF7F7F7F), Color(0xFF747474), Color(0xFF616161))
                else -> Triple(Color(0xFFFF6600), Color(0xFFE85D00), Color(0xFFC24E00))
            }
            val keys = listOf(
                if (paid) Green else if (step >= 1) Orange else dim,
                if (paid) Green else if (step >= 2) Orange else dim,
                if (paid) Green else dim,
            )
            return TillColours(
                body = if (dark) Color(0xFF3A3733) else Color(0xFF1A1917),
                outline = if (dark) Color(0xFF1E1E1E) else Color(0xFFF4F1EA),
                dim = dim,
                keys = keys,
                mid = mid,
                warm = warm,
                deep = deep,
                barFill = if (paid) Green else Orange,
                progress = if (paid) 1f else step / 3f,
            )
        }
    }
}
