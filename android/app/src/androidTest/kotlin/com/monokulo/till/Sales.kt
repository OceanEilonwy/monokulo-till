package com.monokulo.till

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import com.monokulo.till.engine.CloseSheet
import com.monokulo.till.ui.ShiftPlace
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.rememberShiftPlace
import uniffi.till_app.Action
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.LocalPointOfSale
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.SaleFace
import com.monokulo.till.ui.TillActions
import com.monokulo.till.engine.Money
import uniffi.till_app.Platform
import uniffi.till_app.PreviewPhase
import uniffi.till_app.PreviewSale
import uniffi.till_app.previewPhaseStep
import uniffi.till_app.previewSaleLive
import uniffi.till_app.previewSalesJson
import uniffi.till_app.Pos
import uniffi.till_app.TillPreview
import uniffi.till_app.Update
import uniffi.till_app.previewClose
import uniffi.till_app.previewSale
import uniffi.till_app.previewShift
import uniffi.till_screens.CloseView
import uniffi.till_screens.PosMark
import uniffi.till_screens.ShiftEntry
import uniffi.till_screens.ShiftFilter
import uniffi.till_screens.ShiftView

/*
 * The on-device tests' sales are the engine's own fixtures (`till_app::preview_sales`, docs/decisions/core.md 108):
 * a [PreviewSale] the engine turns into the saved shift's JSON, so they can't drift from `TillSale`'s schema.
 */
typealias TillSale = PreviewSale
typealias SalePhase = PreviewPhase

/** Whether [this] sale's code is up, as the engine's `TillSale::live` says. */
val PreviewSale.live: Boolean get() = previewSaleLive(this)

/** Which of the ring's four steps the phase is at, as the engine's `sale_text::step` counts: 0 once it ended unpaid. */
val PreviewPhase.step: Int get() = previewPhaseStep(this).toInt()

/** A sale of this till's for the screens under test, live until [expiresAt] unless [phase] says otherwise. */
fun testSale(
    id: String,
    phase: SalePhase = SalePhase.WAITING,
    expiresAt: Long = Long.MAX_VALUE / 2,
    paidAt: Long? = null,
) = PreviewSale(orderId = id, phase = phase, expiresAt = expiresAt, paidAt = paidAt)

/**
 * [content] in the till's theme with [motion] whatever the emulator's
 * animator scale is, so a timing test runs the presets it names; [dark]
 * picks the theme, else the emulator's.
 */
@Composable
fun Till(motion: Motion = Motion.Full, dark: Boolean? = null, content: @Composable () -> Unit) = TillTheme(dark ?: isSystemInDarkTheme()) {
    // A till on Square, as the activity and the overlay provide it (the sample sales are Square's), so a sale of
    // this till reads "Front · this till", not as one rung up at a point of sale the till has changed from.
    CompositionLocalProvider(LocalMotion provides motion, LocalPointOfSale provides PosMark.SQUARE, content = content)
}

/** Money written as the device writes it, for the engine's previews; it keeps nothing. */
object TestPlatform : Platform {
    override fun secretGet(key: String): ByteArray? = null
    override fun secretPut(key: String, value: ByteArray) = Unit
    override fun secretRemove(key: String) = Unit
    override fun formatMoney(minor: Long, currency: String) = Money.pattern(minor, currency)
    override fun currencySymbols(currency: String) = Money.symbols(currency)
}

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
    salesJson = previewSalesJson(sales),
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

/**
 * The shift as the engine keeps it, for screens under test: the chip and the tray's fold are the engine's
 * ([Action.ChooseFilter], [Action.FoldTray]), kept here as it keeps them (through a turn too), and a Back from a sale
 * whose row the folded tray hides opens the tray before the change starts, as the engine's `unfold_for` does.
 * Every other action goes to [actions].
 */
@Composable
fun EngineShift(
    sales: List<TillSale>,
    now: Long = 1_000,
    actions: TillActions = noActions,
    place: ShiftPlace = rememberShiftPlace(),
    close: CloseSheet? = null,
) {
    var filter by rememberSaveable { mutableStateOf<String?>(null) }
    var folded by rememberSaveable { mutableStateOf(true) }
    val chip = filter?.let { ShiftFilter.valueOf(it) }
    val leaving = com.monokulo.till.LocalStage.current?.leavingSale
    if (folded && leaving != null && hiddenByFold(sales, chip, leaving)) folded = false
    val view = remember(sales, now, chip, folded) { shiftView(sales, now, chip, folded) }
    val engine = remember(actions) {
        TillActions(
            send = { action ->
                when (action) {
                    is Action.ChooseFilter -> filter = action.filter.name
                    is Action.FoldTray -> folded = action.folded
                    else -> actions.send(action)
                }
            },
            onBackToPos = actions.onBackToPos,
        )
    }
    ShiftScreen(view, engine, place, close)
}

/** Whether [orderId]'s row is one the folded tray hides. */
private fun hiddenByFold(sales: List<TillSale>, filter: ShiftFilter?, orderId: String): Boolean {
    fun shows(folded: Boolean) = shiftView(sales, filter = filter, folded = folded).entries.any { it is ShiftEntry.Row && it.orderId == orderId }
    return !shows(true) && shows(false)
}
