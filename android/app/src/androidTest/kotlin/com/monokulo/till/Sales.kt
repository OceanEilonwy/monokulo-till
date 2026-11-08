package com.monokulo.till

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
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
import com.monokulo.till.ui.SaleFace
import com.monokulo.till.ui.TillActions
import com.monokulo.till.engine.Money
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import uniffi.till_app.Platform
import uniffi.till_app.Pos
import uniffi.till_app.TillPreview
import uniffi.till_app.Update
import uniffi.till_app.previewClose
import uniffi.till_app.previewSale
import uniffi.till_app.previewShift
import uniffi.till_screens.CloseView
import uniffi.till_screens.ShiftEntry
import uniffi.till_screens.ShiftFilter
import uniffi.till_screens.ShiftView

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

/** Money written as the device writes it, for the engine's previews; it keeps nothing. */
object TestPlatform : Platform {
    override fun secretGet(key: String): ByteArray? = null
    override fun secretPut(key: String, value: ByteArray) = Unit
    override fun secretRemove(key: String) = Unit
    override fun formatMoney(minor: Long, currency: String) = Money.format(minor, currency)
}

/** As `Stores.kt` saved a shift, which the engine's previews read. */
private val shiftJson = Json { ignoreUnknownKeys = true; explicitNulls = false }

/** A till on Square at "Front" with [sales], watching, at the clock's [now]: the engine's preview of its screens. */
fun preview(
    sales: List<TillSale>,
    now: Long = 1_000,
    filter: ShiftFilter? = null,
    folded: Boolean = true,
    running: Boolean = true,
    live: Boolean = true,
    startedAt: Long? = 900,
    refunding: Boolean = false,
    refundProblem: String? = null,
) = TillPreview(
    salesJson = shiftJson.encodeToString(ListSerializer(TillSale.serializer()), sales),
    now = now,
    till = Pos.SQUARE,
    store = "Corner shop",
    location = "Front",
    running = running,
    live = live,
    startedAt = startedAt,
    filter = filter,
    folded = folded,
    refunding = refunding,
    refundProblem = refundProblem,
)

/** The shift the engine draws for [sales] (see [preview]). */
fun shiftView(sales: List<TillSale>, now: Long = 1_000, filter: ShiftFilter? = null, folded: Boolean = true, running: Boolean = true): ShiftView =
    requireNotNull(previewShift(preview(sales, now, filter, folded, running), TestPlatform)) { "the engine reads the shift" }

/** [sale]'s screens as the engine draws them (see [preview]). */
fun saleFace(sale: TillSale, now: Long = 1_000, refunding: Boolean = false, refundProblem: String? = null): SaleFace =
    SaleFace.of(previewSale(preview(listOf(sale), now, refunding = refunding, refundProblem = refundProblem), sale.orderId, TestPlatform) as Update.Sale)

/** The close sheet the engine draws over [sale], while it has answers. */
fun closeView(sale: TillSale, now: Long = 1_000): CloseView? = previewClose(preview(listOf(sale), now), sale.orderId, TestPlatform)

/** Actions that go nowhere, for screens under test. */
val noActions = TillActions(send = {}, onBackToPos = {})

/** [sale]'s screens as the engine draws them, made again only when the sale or the clock changes. */
@Composable
fun rememberFace(sale: TillSale, now: Long = 1_000): SaleFace = remember(sale, now) { saleFace(sale, now) }

/** The shift the engine draws for [sales], made again only when they or the clock change. */
@Composable
fun rememberShift(sales: List<TillSale>, now: Long = 1_000, running: Boolean = true): ShiftView = remember(sales, now, running) { shiftView(sales, now, running = running) }

/** [sale]'s row's title on the shift: "Sale #m2Qe · $8.40". */
fun rowTitle(sale: TillSale): String = shiftView(listOf(sale)).entries.filterIsInstance<ShiftEntry.Row>().single().row.title

/** The shift's headers, as the engine writes them. */
const val EARLIER = "Earlier"
const val TRAY_HEADER = "Waiting for payment"
