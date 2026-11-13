package com.monokulo.till

import android.app.Activity
import android.content.res.Configuration
import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.core.view.WindowCompat
import androidx.test.platform.app.InstrumentationRegistry
import com.monokulo.till.ui.AlertsScreen
import com.monokulo.till.ui.CodeScreen
import com.monokulo.till.ui.ConfirmScreen
import com.monokulo.till.ui.ConnectedScreen
import com.monokulo.till.ui.CounterScreen
import com.monokulo.till.ui.FloatScreen
import com.monokulo.till.ui.LocalMotion
import com.monokulo.till.ui.LocalPointOfSale
import com.monokulo.till.ui.LocationsScreen
import com.monokulo.till.ui.Motion
import com.monokulo.till.ui.OrderPage
import com.monokulo.till.ui.PointOfSaleScreen
import com.monokulo.till.ui.PosSetupActions
import com.monokulo.till.ui.ReadyScreen
import com.monokulo.till.ui.SaleFace
import com.monokulo.till.ui.SaleScreen
import com.monokulo.till.ui.ScanScreen
import com.monokulo.till.ui.SettingsActions
import com.monokulo.till.ui.SettingsScreen
import com.monokulo.till.ui.ShiftScreen
import com.monokulo.till.ui.ShowSalesScreen
import com.monokulo.till.ui.TillTheme
import com.monokulo.till.ui.WelcomeScreen
import com.monokulo.till.ui.FobConnectedScreen
import com.monokulo.till.ui.FobRequestScreen
import com.monokulo.till.ui.PaymentsModeScreen
import com.monokulo.till.ui.StepperActions
import com.monokulo.till.ui.LightServerScreen
import com.monokulo.till.ui.ThisTillActions
import com.monokulo.till.ui.ThisTillScreen
import com.monokulo.till.ui.PricesScreen
import com.monokulo.till.ui.KeypadScreen
import com.monokulo.till.ui.WalletCodeScreen
import com.monokulo.till.ui.WalletEntryScreen
import com.monokulo.till.ui.WalletKeysScreen
import com.monokulo.till.ui.WalletSeedScreen
import com.monokulo.till.ui.mk
import com.monokulo.till.ui.rememberShiftPlace
import java.io.File
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import uniffi.till_app.Pos
import uniffi.till_app.PreviewPhase
import uniffi.till_app.PreviewSale
import uniffi.till_app.PreviewStep
import uniffi.till_app.TillPreview
import uniffi.till_app.Update
import uniffi.till_app.previewClose
import uniffi.till_app.previewOnboarding
import uniffi.till_app.previewSale
import uniffi.till_app.previewKeypad
import uniffi.till_app.previewSalesJson
import uniffi.till_app.previewSettings
import uniffi.till_app.previewSettingsOf
import uniffi.till_app.PreviewSettings
import uniffi.till_app.previewShift
import uniffi.till_screens.CloseView
import uniffi.till_screens.Device
import uniffi.till_screens.OnboardingPage
import uniffi.till_screens.OnboardingView
import uniffi.till_screens.PaymentsMode
import uniffi.till_screens.PosMark
import uniffi.till_screens.SettingsView
import uniffi.till_screens.ShiftView

/**
 * Screenshots of the key screens and states, light and dark, for a reviewer without a device: screenshots.yml sets
 * them beside the iPhone's and the iPad's (tools/screenshot-compare). The screens are the app's own, drawn from the
 * engine's previews of the same sample till as the iOS tests' ScreenshotTests.swift, under the same names.
 *
 * Runs only when asked (`-e screenshots on`, android/scripts/screenshots.sh): each screen is written as
 * `<name>-light.png` and `<name>-dark.png` under the app's files, in `screenshots/android-phone` or
 * `…/android-tablet`, with `shots.txt` (or, turned on its side, `shots-landscape.txt`) naming them in order and
 * `device.txt` the device. A tablet turned on its side draws only the landscape screens, named `landscape-…`.
 */
class ScreenshotTest {
    @get:Rule
    val rule = createComposeRule()

    private class Shot(val name: String, val content: @Composable () -> Unit)

    private var shown by mutableStateOf<Shot?>(null)
    private var dark by mutableStateOf(false)

    @Test
    fun everyKeyScreen() {
        assumeTrue("not asked for screenshots", InstrumentationRegistry.getArguments().getString("screenshots") != null)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val configuration = context.resources.configuration
        val tablet = configuration.smallestScreenWidthDp >= 600
        val landscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
        val folder = File(context.filesDir, "screenshots/${if (tablet) "android-tablet" else "android-phone"}").apply { mkdirs() }
        val shots = when {
            landscape && tablet -> landscapeShots()
            landscape -> emptyList()
            else -> shots()
        }
        rule.mainClock.autoAdvance = false
        rule.setContent {
            val shot = shown ?: return@setContent
            // The status bar's icons as MainActivity's enableEdgeToEdge sets them: dark on the light theme.
            val view = LocalView.current
            SideEffect {
                val window = (view.context as Activity).window
                WindowCompat.getInsetsController(window, view).run {
                    isAppearanceLightStatusBars = !dark
                    isAppearanceLightNavigationBars = !dark
                }
            }
            TillTheme(dark) {
                CompositionLocalProvider(LocalMotion provides Motion.Still, LocalPointOfSale provides PosMark.SQUARE) {
                    Box(Modifier.fillMaxSize().background(mk.ground)) { shot.content() }
                }
            }
        }
        for (shot in shots) {
            for (mode in listOf(false, true)) {
                rule.runOnUiThread {
                    shown = shot
                    dark = mode
                }
                // Sheets rise and codes draw; the clock only moves when told, so the spinning ring stops too. In steps:
                // a sheet's window is composed a frame or two in, and its rise runs on the same clock.
                repeat(25) {
                    rule.mainClock.advanceTimeBy(100)
                    rule.waitForIdle()
                }
                // The first screen also waits for the activity's first frame.
                Thread.sleep(if (shot === shots.first() && !mode) 2_500 else 600)
                val picture = instrumentation.uiAutomation.takeScreenshot() ?: error("no screenshot of ${shot.name}")
                File(folder, "${shot.name}-${if (mode) "dark" else "light"}.png").outputStream().use {
                    picture.compress(Bitmap.CompressFormat.PNG, 100, it)
                }
            }
        }
        File(folder, if (landscape) "shots-landscape.txt" else "shots.txt").writeText(shots.joinToString("") { it.name + "\n" })
        val name = InstrumentationRegistry.getArguments().getString("device")?.trim()?.takeIf { it.isNotEmpty() } ?: android.os.Build.MODEL
        File(folder, "device.txt").writeText("$name · Android ${android.os.Build.VERSION.RELEASE} (API ${android.os.Build.VERSION.SDK_INT})\n")
    }

    // The sample till, as ScreenshotTests.swift has it.

    private val now = 1_761_000_600L

    private val sales = listOf(
        PreviewSale(orderId = "w1", phase = PreviewPhase.WAITING, amount = "8.40", reference = "m2Qe", expiresAt = now + 840, receipt = true, customerName = "Sam", createdAt = now - 60),
        PreviewSale(orderId = "s1", phase = PreviewPhase.SEEN, amount = "12.00", reference = "k7Pa", expiresAt = now + 600, customerName = "Ana", createdAt = now - 120, seenAt = now - 30),
        PreviewSale(
            orderId = "c1", phase = PreviewPhase.CONFIRMING, amount = "23.75", reference = "r4Tn", expiresAt = now + 300, customerName = "Lee",
            createdAt = now - 300, seenAt = now - 240, confirmations = 4u, confirmationsRequired = 10u,
        ),
        PreviewSale(orderId = "p1", phase = PreviewPhase.PAID, amount = "4.50", reference = "b9Lw", expiresAt = now + 300, receipt = true, customerName = "Mia", createdAt = now - 900, seenAt = now - 840, paidAt = now - 600),
        PreviewSale(orderId = "e1", phase = PreviewPhase.EXPIRED, amount = "6.20", reference = "x3Hd", expiresAt = now - 1_200, customerName = "Joe", createdAt = now - 2_100, endedAt = now - 1_200),
        PreviewSale(orderId = "o1", phase = PreviewPhase.PAID, amount = "15.00", reference = "z8Qc", expiresAt = now - 1_500, found = false, deviceName = "Bar iPad", createdAt = now - 2_400, seenAt = now - 2_350, paidAt = now - 2_200),
    )

    private fun preview(sales: List<PreviewSale>, live: Boolean = true, problem: String? = null) = TillPreview(
        salesJson = previewSalesJson(sales),
        now = now,
        device = Device.ANDROID,
        till = Pos.SQUARE,
        store = "The Bean Room",
        location = "Front Counter",
        live = live,
        problem = problem,
        startedAt = now - 3_600,
    )

    private val full by lazy { preview(sales) }
    private val shift: ShiftView by lazy { requireNotNull(previewShift(full, TestPlatform)) }
    private val empty: ShiftView by lazy { requireNotNull(previewShift(preview(emptyList()), TestPlatform)) }
    private val offline: ShiftView by lazy { requireNotNull(previewShift(preview(sales, live = false, problem = "Couldn't reach the store."), TestPlatform)) }
    private val settings: SettingsView by lazy { previewSettings(Device.ANDROID, now) }

    /** A sale the till's own wallet priced: its code names the provider and the price's time (docs/view-key-mode.md §8). */
    private val pricedFace by lazy {
        val priced = preview(listOf(sales[0].copy(priceProvider = "Kraken", priceAt = now - 60)))
        SaleFace.of(previewSale(priced, "w1", TestPlatform) as Update.Sale)
    }

    /** A sale whose code ran out on a till on the shop's own wallet: Till, not Monokulo, watches for a late payment (core.md decision 252). */
    private val walletExpiredFace by lazy {
        SaleFace.of(previewSale(full.copy(payments = PaymentsMode.WALLET), "e1", TestPlatform) as Update.Sale)
    }

    /** Sales charged on Till's keypad, two put aside for the next customer. */
    private val keypadSales = listOf(
        PreviewSale(orderId = "k1", phase = PreviewPhase.WAITING, amount = "8.40", reference = "Sam", expiresAt = now + 840, foundPos = Pos.TILL, createdAt = now - 60),
        PreviewSale(orderId = "k2", phase = PreviewPhase.SEEN, amount = "12.00", reference = "Table 4", expiresAt = now + 600, foundPos = Pos.TILL, createdAt = now - 120, seenAt = now - 30),
    )

    private fun keypadPreview(sales: List<PreviewSale>) = preview(sales).copy(till = Pos.TILL, location = null)

    private fun keypad(currency: String, typed: String, reference: String, aside: Boolean) =
        requireNotNull(previewKeypad(keypadPreview(if (aside) keypadSales else emptyList()), currency, typed, reference, TestPlatform))

    private val keypadFace by lazy { SaleFace.of(previewSale(keypadPreview(keypadSales), "k1", TestPlatform) as Update.Sale) }

    private fun face(orderId: String) = SaleFace.of(previewSale(full, orderId, TestPlatform) as Update.Sale)
    private fun close(orderId: String): CloseView? = previewClose(full, orderId, TestPlatform)
    private fun step(step: PreviewStep): OnboardingView = previewOnboarding(step, Device.ANDROID, TestPlatform)

    private fun setup(name: String, step: PreviewStep) = Shot(name) { Step(step(step)) }
    private fun shiftShot(name: String, view: () -> ShiftView) = Shot(name) { ShiftScreen(view(), noActions, rememberShiftPlace()) }
    private fun sale(name: String, orderId: String, sheet: Boolean = false) = Shot(name) {
        SaleScreen(face(orderId), noActions, close(orderId).takeIf { sheet })
    }
    private fun code(name: String) = Shot(name) { CodeScreen(face("w1")) {} }
    private fun settingsShot(name: String) = Shot(name) { SettingsScreen(settings, settingsActions) }

    private fun shots() = listOf(
        setup("setup-1-welcome", PreviewStep.WELCOME),
        setup("setup-2-scan", PreviewStep.SCAN),
        setup("setup-3-confirm", PreviewStep.CONFIRM),
        setup("setup-3-connected", PreviewStep.CONNECTED),
        setup("setup-4-point-of-sale", PreviewStep.POINT_OF_SALE),
        setup("setup-4-signed-in", PreviewStep.SIGNED_IN),
        setup("setup-4-test-sale", PreviewStep.TEST_SALE),
        setup("setup-4-locations", PreviewStep.LOCATIONS),
        setup("setup-4-counter", PreviewStep.COUNTER),
        setup("setup-5-alerts", PreviewStep.ALERTS),
        setup("setup-5-float", PreviewStep.FLOAT),
        setup("setup-6-ready", PreviewStep.READY),
        // View-key mode's setup, under the iOS tests' names: B4 on a phone, C4 on a tablet.
        setup("vk-mode", PreviewStep.VK_MODE),
        setup("vk-wallet", PreviewStep.VK_WALLET),
        setup("vk-pad-request", PreviewStep.VK_PAD_REQUEST),
        if (tablet) setup("vk-pad-connected", PreviewStep.VK_PAD_CONNECTED) else setup("vk-connected", PreviewStep.VK_CONNECTED),
        setup("vk-scan-code", PreviewStep.VK_SCAN_CODE),
        setup("vk-keys-error", PreviewStep.VK_KEYS_ERROR),
        setup("vk-seed", PreviewStep.VK_SEED),
        setup("vk-this-till", PreviewStep.VK_THIS_TILL),
        setup("vk-light-mode", PreviewStep.VK_LIGHT_MODE),
        setup("vk-light-server", PreviewStep.VK_LIGHT_SERVER),
        setup("vk-light-this-till", PreviewStep.VK_LIGHT_THIS_TILL),
        setup("vk-prices-step", PreviewStep.VK_PRICES),
        setup("vk-prices-fob", PreviewStep.VK_PRICES_FOB),
        Shot("vk-prices-code") { SaleScreen(pricedFace, noActions, null) },
        // Till's keypad as the point of sale (docs/view-key-mode.md §7).
        setup("pos-keypad-choose", PreviewStep.POS_KEYPAD_CHOOSE),
        setup("pos-keypad-currency", PreviewStep.POS_KEYPAD_CURRENCY),
        Shot("pos-keypad") { KeypadScreen(keypad("USD", "1250", "Sam", aside = true)) {} },
        Shot("pos-keypad-empty") { KeypadScreen(keypad("USD", "", "", aside = false)) {} },
        Shot("pos-keypad-xmr") { KeypadScreen(keypad("XMR", "0.05", "", aside = false)) {} },
        Shot("pos-keypad-sale") { SaleScreen(keypadFace, noActions, null) },
        Shot("pos-keypad-settings") { SettingsScreen(previewSettingsOf(PreviewSettings.POS_KEYPAD, Device.ANDROID, now), settingsActions) },
        shiftShot("till-empty") { empty },
        shiftShot("till-shift") { shift },
        shiftShot("till-shift-offline") { offline },
        sale("sale-waiting", "w1"),
        sale("sale-seen", "s1"),
        sale("sale-confirming", "c1"),
        sale("sale-paid", "p1"),
        sale("sale-expired", "e1"),
        sale("sale-other-till", "o1"),
        code("sale-code"),
        Shot("sale-order") { OrderPage(face("p1"), onBack = {}, onShowCode = {}) },
        sale("sale-close-sheet", "w1", sheet = true),
        settingsShot("settings"),
        Shot("vk-settings") { SettingsScreen(previewSettingsOf(PreviewSettings.VK_SETTINGS, Device.ANDROID, now), settingsActions) },
        Shot("vk-settings-warning") { SettingsScreen(previewSettingsOf(PreviewSettings.VK_SETTINGS_WARNING, Device.ANDROID, now), settingsActions) },
        Shot("vk-light-settings") { SettingsScreen(previewSettingsOf(PreviewSettings.VK_LIGHT_SETTINGS, Device.ANDROID, now), settingsActions) },
        Shot("vk-sale-expired") { SaleScreen(walletExpiredFace, noActions, null) },
    )

    private fun landscapeShots() = listOf(
        setup("landscape-setup-1-welcome", PreviewStep.WELCOME),
        setup("landscape-setup-4-point-of-sale", PreviewStep.POINT_OF_SALE),
        shiftShot("landscape-till-empty") { empty },
        shiftShot("landscape-till-shift") { shift },
        sale("landscape-sale-waiting", "w1"),
        sale("landscape-sale-paid", "p1"),
        code("landscape-sale-code"),
        settingsShot("landscape-settings"),
    )

    private val settingsActions = SettingsActions(
        send = {}, onAlerts = {}, onBubble = {}, onCopyAddress = {}, onGetDistributor = {}, distributorName = { it },
    )

    private val posActions = PosSetupActions(onChoose = {}, onNext = {}, onChangeLocation = {}, onLookAgain = {}, onSkip = {}, onBack = {})

    /** A step of setup, drawn as MainActivity's SetupScreens draws it. */
    @Composable
    private fun Step(view: OnboardingView) {
        val step = view.stepLabel
        when (val page = view.page) {
            is OnboardingPage.Welcome -> WelcomeScreen(page, step) {}
            is OnboardingPage.Scan -> ScanScreen(page, step, onPaste = {}, onScan = {}, onBack = {})
            is OnboardingPage.Confirm -> ConfirmScreen(page, step, onConnect = {}, onCancel = {})
            is OnboardingPage.Connected -> ConnectedScreen(page, step, onNext = {}, onRetry = {}, onWrong = {})
            is OnboardingPage.PointOfSale -> PointOfSaleScreen(page.view, step, posActions, heading = view.heading, lead = view.lead)
            is OnboardingPage.Locations -> LocationsScreen(page, step, onPick = {}, onBack = {})
            is OnboardingPage.Alerts -> AlertsScreen(page, step, onAllow = {}, onSkip = {}, onBack = {})
            is OnboardingPage.Float -> FloatScreen(page, onOpenSettings = {}, onNotificationsOnly = {}, onContinue = {}, onBack = {})
            is OnboardingPage.Ready -> ReadyScreen(page, bubble = false, onStart = {})
            is OnboardingPage.Counter -> CounterScreen(page.view, onPick = {}, onNext = {}, onBack = {})
            is OnboardingPage.ShowSales -> ShowSalesScreen(page.view, onPick = {}, onNext = {}, onBack = {})
            is OnboardingPage.PaymentsMode -> PaymentsModeScreen(page.view, view.heading, view.lead, step, onChoose = {}, onNext = {}, onBack = {})
            is OnboardingPage.WalletEntry -> WalletEntryScreen(page.view, view.heading, view.lead, step, onConnect = {}, onWay = {}, onGetFob = {}, onBack = {})
            // As on a device without Fob: the two-device way, as the board.
            is OnboardingPage.FobRequest -> FobRequestScreen(page.view, view.heading, view.lead, step, fobHere = false, onOpenFob = {}, onAnswer = {}, onBack = {})
            is OnboardingPage.FobConnected -> FobConnectedScreen(page.view, view.heading, view.lead, step, steppers, onNext = {}, onOtherWallet = {})
            is OnboardingPage.WalletCode -> WalletCodeScreen(page.view, view.heading, view.lead, step, onScan = {}, onPaste = {}, onBack = {})
            is OnboardingPage.WalletKeys -> WalletKeysScreen(page.view, view.heading, view.lead, step, onEdit = { _, _, _ -> }, onNext = {}, onBack = {})
            is OnboardingPage.WalletSeed -> WalletSeedScreen(page.view, view.heading, view.lead, step, onLength = {}, onScanInstead = {}, onEnter = {}, onBack = {})
            is OnboardingPage.LightServer -> LightServerScreen(page.view, view.heading, view.lead, step, onEdit = { _, _ -> }, onNext = {}, onBack = {})
            is OnboardingPage.ThisTill -> ThisTillScreen(
                page.view, view.heading, view.lead, step, ThisTillActions(steppers, onNumber = {}, onNode = {}, onNext = {}, onBack = {}),
            )
            is OnboardingPage.Prices -> PricesScreen(page.view, view.heading, view.lead, step, onChoose = {}, onNext = {}, onBack = {})
        }
    }

    private val steppers = StepperActions(onPaidAfter = {}, onExpiry = {})

    private val tablet: Boolean
        get() = InstrumentationRegistry.getInstrumentation().targetContext.resources.configuration.smallestScreenWidthDp >= 600
}
