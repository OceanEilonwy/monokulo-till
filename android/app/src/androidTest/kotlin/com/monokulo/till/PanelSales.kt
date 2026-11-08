package com.monokulo.till

import com.monokulo.till.core.SalePhase
import com.monokulo.till.overlay.Hero
import com.monokulo.till.overlay.PanelFace
import com.monokulo.till.overlay.PanelSale
import com.monokulo.till.sales.Standing
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.standing
import com.monokulo.till.ui.CodeText
import com.monokulo.till.ui.secondsLeft
import uniffi.till_screens.OrderCard
import uniffi.till_screens.OrderLine

/**
 * The panel's sale for [sale], in the words the engine's bubble gives
 * (`till_screens::bubble` and `sale`), written here by the Kotlin text the
 * engine's was ported from, so the on-device tests can keep making their
 * sales as [TillSale]s. The engine is never asked.
 */
fun panelOf(sale: TillSale, now: Long = System.currentTimeMillis() / 1000): PanelSale {
    val face = when (sale.standing) {
        is Standing.Live -> PanelFace.Code
        is Standing.Paid -> PanelFace.Paid
        else -> PanelFace.Ended
    }
    val amount = SaleText.amount(sale)
    val receipt = sale.receipt?.takeIf { it.lineItems.isNotEmpty() }
    return PanelSale(
        orderId = sale.orderId,
        face = face,
        hero = when {
            face == PanelFace.Paid -> Hero.Paid
            sale.refunded -> Hero.Refunded
            sale.phase == SalePhase.DoubleSpend -> Hero.Reversed
            else -> Hero.Ended
        },
        title = SaleText.title(sale),
        amount = amount,
        xmr = SaleText.xmr(sale),
        code = sale.code.uri,
        codeSaid = "Payment code for $amount",
        status = SaleText.status(sale, null),
        left = if (face == PanelFace.Code) CodeText.left(sale, secondsLeft(sale.expiresAt, now))?.let { "· $it" } else null,
        step = SaleText.step(sale.phase),
        received = "${SaleText.xmr(sale)} received",
        endedHeading = if (!sale.refunded && sale.phase == SalePhase.DoubleSpend) "Payment reversed · don't hand it over" else SaleText.status(sale, null),
        endedLine = when {
            sale.refunded -> "Nothing more to do."
            sale.found == null -> PosText.refundOnOtherTill(null, sale.deviceName ?: "the till that took it")
            else -> "Open the shift to ${PosText.refundItIn(null)}."
        },
        order = receipt?.let {
            val (lines, more) = SaleText.shownLines(it)
            OrderCard(
                summary = SaleText.receiptSummary(it, null),
                lines = lines.map { line -> OrderLine(SaleText.line(line), SaleText.plain(line.lineTotal, it.currency)) },
                more = if (more > 0) "+$more more" else null,
                foot = SaleText.tax(it) ?: "Total",
                total = SaleText.total(it),
            )
        },
        copyAddress = "Copy address",
    )
}
