package com.monokulo.till

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.receipt.PaymentCode
import com.monokulo.till.receipt.Receipt
import com.monokulo.till.sales.FoundSale
import com.monokulo.till.sales.TillSale
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.LocalPointOfSale
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion

/** A sale of this till's for the screens under test, live until [expiresAt] unless [phase] says otherwise. */
fun testSale(
    id: String,
    phase: SalePhase = SalePhase.Waiting,
    expiresAt: Long = Long.MAX_VALUE / 2,
    paidAt: Long? = null,
) = TillSale(
    orderId = id,
    amount = "8.40",
    currency = "USD",
    reference = id,
    code = PaymentCode("monero:4abc?tx_amount=0.0336&tx_description=$id", hasReceipt = false),
    phase = phase,
    expiresAt = expiresAt,
    xmr = "0.0336",
    address = "4abc",
    found = FoundSale("p_$id", 840, "USD", id, "o", "2026-10-22T22:50:00Z", null),
    createdAt = 1_761_000_000,
    paidAt = paidAt,
)

/**
 * [content] in the till's theme with [motion] whatever the emulator's
 * animator scale is, so a timing test runs the presets it names; [dark]
 * picks the theme, else the emulator's.
 */
@Composable
fun Till(motion: Motion = Motion.Full, dark: Boolean? = null, content: @Composable () -> Unit) = TillTheme(dark ?: isSystemInDarkTheme()) {
    // A till on Square, as the activity and the overlay provide it (the sample sales are Square's), so a sale of
    // this till reads "Front · this till", not as one rung up at a point of sale the till has changed from.
    CompositionLocalProvider(LocalMotion provides motion, LocalPointOfSale provides PointOfSale.Square, content = content)
}

/** The design's order: 15 items on 8 lines, so the sale's card shows three and "+5 more". */
fun sampleReceipt() = Receipt(
    receipt = Receipt.Info("k4Rt", "2026-10-22T16:07:00-07:00"),
    merchant = Receipt.Merchant("The Bean Room", "US"),
    lineItems = listOf(
        Receipt.LineItem("Flat white", 2.0, 4.5, 9.0),
        Receipt.LineItem("Oat latte", 1.0, 5.25, 5.25),
        Receipt.LineItem("Almond croissant", 3.0, 3.75, 11.25),
        Receipt.LineItem("Banana bread", 1.0, 4.5, 4.5),
        Receipt.LineItem("Cold brew", 2.0, 5.0, 10.0),
        Receipt.LineItem("Chai", 1.0, 4.75, 4.75),
        Receipt.LineItem("Choc chip cookie", 4.0, 2.5, 10.0),
        Receipt.LineItem("House blend beans, 250 g", 1.0, 18.0, 18.0),
    ),
    tax = Receipt.Tax(listOf(Receipt.TaxLine(0.08875, 6.46))),
    total = 79.21,
    currency = "USD",
)
