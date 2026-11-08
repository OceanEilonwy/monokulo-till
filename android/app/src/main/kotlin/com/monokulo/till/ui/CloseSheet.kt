package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import uniffi.till_screens.CloseAnswer
import uniffi.till_screens.CloseChoice
import uniffi.till_screens.CloseView
import uniffi.till_screens.Ring

/**
 * "Is the customer paying?" (the design's CloseAsk; decisions 62, 70): the
 * tinted X's sheet over the sale, or over the shift when its row is swiped,
 * as the engine wrote it. Each answer slides the sheet away, then goes up as
 * [onChoice]; the scrim, Back and a drag down dismiss it unchanged.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CloseSheet(view: CloseView, onChoice: (CloseChoice) -> Unit, onDismiss: () -> Unit) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = state,
        shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
        containerColor = mk.card,
        contentColor = mk.ink,
        scrimColor = Fixed.Scrim,
        dragHandle = {
            Box(Modifier.padding(top = 12.dp, bottom = 12.dp).size(width = 40.dp, height = 5.dp).clip(CircleShape).background(mk.edge))
        },
        modifier = Modifier.semantics { paneTitle = view.question },
    ) {
        Column(
            Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(Modifier.padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(view.question, style = display(22), modifier = Modifier.semantics { heading() })
                Text(view.subtitle, fontSize = 14.sp, color = mk.muted)
            }
            for (answer in view.answers) {
                Answer(answer, view.ring) {
                    // Only once the sheet is down: a sale paid meanwhile swaps
                    // the screen out, cancels the hide and drops the answer.
                    scope.launch { state.hide() }.invokeOnCompletion { cause ->
                        if (cause == null) onChoice(answer.choice)
                    }
                }
            }
        }
    }
}

/** One answer: its icon on its tint, what it says and what it does. */
@Composable
private fun Answer(answer: CloseAnswer, ring: Ring, onClick: () -> Unit) = Row(
    Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(18.dp)).background(mk.ground)
        .clickable(role = Role.Button, onClick = onClick).padding(14.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(14.dp),
) {
    when (answer.choice) {
        CloseChoice.REFUND_IT -> Glyph(mk.dangerGround) { Icon(MkIcons.Refund, null, tint = mk.danger, modifier = Modifier.size(20.dp)) }
        CloseChoice.CLOSE_FOR_NOW -> Glyph(mk.chip) { Icon(MkIcons.Tray, null, tint = mk.ink, modifier = Modifier.size(18.dp)) }
        CloseChoice.KEEP_UP -> Glyph(mk.warnGround) { PhaseRing(ring, 22.dp, track = mk.edge) }
    }
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(answer.title, fontSize = 16.sp, fontWeight = FontWeight.Bold)
        Text(answer.detail, fontSize = 13.sp, color = mk.muted, style = LocalTextStyle.current.merge(tabular))
    }
}

@Composable
private fun Glyph(ground: Color, content: @Composable () -> Unit) = Box(
    Modifier.size(40.dp).clip(CircleShape).background(ground),
    contentAlignment = Alignment.Center,
) { content() }
