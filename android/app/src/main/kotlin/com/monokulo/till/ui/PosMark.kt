package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.monokulo.till.R
import uniffi.till_screens.PosMark

/**
 * The point of sale's mark, [size] square: Square's in the theme's ink (it
 * is one colour, black or white), Clover's in its own green. Decoration:
 * the name beside it is what TalkBack reads.
 */
@Composable
fun PosMark(pos: PosMark, size: Dp, modifier: Modifier = Modifier, tint: Color? = null) = Icon(
    painterResource(
        when (pos) {
            PosMark.SQUARE -> R.drawable.ic_pos_square
            PosMark.CLOVER -> R.drawable.ic_pos_clover
        },
    ),
    contentDescription = null,
    modifier = modifier.size(size),
    tint = tint ?: if (pos == PosMark.SQUARE) mk.ink else Color.Unspecified,
)

/** The mark on its 40 dp inset tile, leading a point of sale's row (the design's Logos board, A). */
@Composable
fun PosTile(pos: PosMark) = Box(
    Modifier.size(40.dp).clip(TileShape).background(mk.inset),
    contentAlignment = Alignment.Center,
) { PosMark(pos, 26.dp) }

/**
 * The shift header's mark (the Logos board, B): the till's point of sale,
 * or a plain chip-coloured circle for a till without one, where the dot
 * that said "watching" was.
 */
@Composable
fun PosBadge(pos: PosMark?) {
    if (pos != null) PosMark(pos, 18.dp) else Box(Modifier.size(18.dp).clip(CircleShape).background(mk.chip))
}

/**
 * Till's mark and [pos]'s on card tiles joined by a dashed line, over
 * the Connect card (the Logos board, C), as sign-in pages show two apps
 * linking. Decoration: the heading under it says the same.
 */
@Composable
fun LinkedMarks(pos: PosMark, modifier: Modifier = Modifier) = Row(
    modifier.padding(vertical = 8.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(10.dp),
) {
    val card = mk.card
    Box(Modifier.size(56.dp).clip(LinkTileShape).background(card), contentAlignment = Alignment.Center) { TillWidget(42.dp) }
    val outline = mk.outline
    Spacer(
        Modifier.size(40.dp, 12.dp).drawWithCache {
            // The board's 2 dp dashes, 3 on and 4 off, round-capped, inset 2 dp at each end: made once per size.
            val inset = 2.dp.toPx()
            val y = size.height / 2f
            val stroke = 2.dp.toPx()
            val dashes = PathEffect.dashPathEffect(floatArrayOf(3.dp.toPx(), 4.dp.toPx()))
            onDrawBehind {
                drawLine(outline, Offset(inset, y), Offset(size.width - inset, y), stroke, StrokeCap.Round, dashes)
            }
        },
    )
    Box(Modifier.size(56.dp).clip(LinkTileShape).background(card), contentAlignment = Alignment.Center) { PosMark(pos, 34.dp) }
}

private val TileShape = RoundedCornerShape(12.dp)
private val LinkTileShape = RoundedCornerShape(18.dp)
