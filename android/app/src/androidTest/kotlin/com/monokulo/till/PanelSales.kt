package com.monokulo.till

import com.monokulo.till.overlay.PanelSale
import uniffi.till_app.previewPanel

/**
 * The panel's sale for [sale] at the clock's [now], as the engine draws its
 * bubble's panel (`preview_panel`, `till_screens::bubble::panel`).
 */
fun panelOf(sale: TillSale, now: Long = System.currentTimeMillis() / 1000): PanelSale =
    PanelSale.of(requireNotNull(previewPanel(preview(listOf(sale), now), sale.orderId, TestPlatform)) { "the engine draws the panel" })
