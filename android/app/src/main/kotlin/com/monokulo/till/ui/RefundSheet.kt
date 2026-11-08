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
import com.monokulo.till.PosText
import com.monokulo.till.SaleText
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.sales.TillSale

/**
 * The refund sheet over a sale's page, open while the cashier refunds [orderId]
 * at [pointOfSale], the one that took it (decision 130): one whose API can't
 * refund it (Clover), or one the till has changed from since. [asks]: the till
 * can ask [pointOfSale] whether it's done. [checking] while "I've refunded it"
 * asks, [notSeen] when it answered no refund yet.
 */
data class RefundSheet(
    val orderId: String,
    val pointOfSale: PointOfSale,
    val asks: Boolean,
    val checking: Boolean = false,
    val notSeen: Boolean = false,
)

/**
 * "Refund in Clover" (acceptance T4.1): what to refund and where, with **I've
 * refunded it**, which asks the point of sale now, and **Not yet**, which
 * leaves the sale pending under Needs action. The scrim, Back and a drag down
 * are Not yet too. The sheet goes by itself once the sale shows refunded.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RefundInPosSheet(sale: TillSale, sheet: RefundSheet, onRefunded: () -> Unit, onNotYet: () -> Unit) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val title = PosText.refundSheetTitle(sheet.pointOfSale)
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
        modifier = Modifier.semantics { paneTitle = title },
    ) {
        Column(
            Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(title, style = display(22), modifier = Modifier.padding(horizontal = 4.dp).semantics { heading() })
            Text(
                PosText.refundSheetBody(
                    sheet.pointOfSale,
                    sheet.asks,
                    order = sale.found?.let { it.title ?: it.orderId },
                    time = sale.createdAt.takeIf { it > 0 }?.let { SaleText.time(it) },
                    amount = SaleText.amount(sale),
                ),
                fontSize = 15.sp, lineHeight = 22.sp, modifier = Modifier.padding(horizontal = 4.dp),
            )
            if (sheet.notSeen) {
                Text(
                    PosText.refundNotSeen(sheet.pointOfSale), fontSize = 14.sp, lineHeight = 20.sp, color = mk.ink,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(mk.warnGround)
                        .padding(horizontal = 14.dp, vertical = 12.dp).semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
            PrimaryButton(
                if (sheet.checking) PosText.LOOKING else PosText.REFUNDED_IT, onRefunded, Modifier.fillMaxWidth(),
                enabled = !sheet.checking, height = 56.dp, size = 16,
            )
            OutlineButton(PosText.NOT_YET, onNotYet, Modifier.fillMaxWidth(), height = 52.dp)
        }
    }
}
