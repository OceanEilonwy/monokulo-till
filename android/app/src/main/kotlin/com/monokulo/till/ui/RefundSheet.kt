package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.till_app.RefundSheetView

/**
 * "Refund in Clover" (acceptance T4.1; decision 130): what to refund and
 * where, as the engine wrote it, with **I've refunded it** ([onRefunded]),
 * which asks the point of sale now, and **Not yet** ([onNotYet]), which
 * leaves the sale pending under Needs action. The scrim, Back and a drag
 * down are Not yet too. The engine takes the sheet down once the sale shows
 * refunded.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RefundInPosSheet(sheet: RefundSheetView, onRefunded: () -> Unit, onNotYet: () -> Unit) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onNotYet,
        sheetState = state,
        shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
        containerColor = mk.card,
        contentColor = mk.ink,
        scrimColor = Fixed.Scrim,
        dragHandle = {
            Box(Modifier.padding(top = 12.dp, bottom = 12.dp).size(width = 40.dp, height = 5.dp).clip(CircleShape).background(mk.edge))
        },
        modifier = Modifier.semantics { paneTitle = sheet.title },
    ) {
        Column(
            Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(sheet.title, style = display(22), modifier = Modifier.padding(horizontal = 4.dp).semantics { heading() })
            Text(sheet.body, fontSize = 15.sp, lineHeight = 22.sp, modifier = Modifier.padding(horizontal = 4.dp))
            sheet.notSeen?.let { notSeen ->
                Text(
                    notSeen, fontSize = 14.sp, lineHeight = 20.sp, color = mk.ink,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(mk.warnGround)
                        .padding(horizontal = 14.dp, vertical = 12.dp).semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
            PrimaryButton(
                sheet.refunded.label, onRefunded, Modifier.fillMaxWidth(),
                enabled = sheet.refunded.enabled, height = 56.dp, size = 16,
            )
            OutlineButton(sheet.notYet, onNotYet, Modifier.fillMaxWidth(), height = 52.dp)
        }
    }
}
