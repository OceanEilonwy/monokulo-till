package com.monokulo.till.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.view.WindowManager
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import com.monokulo.till.Move
import com.monokulo.till.LocalStage
import com.monokulo.till.Nav
import androidx.compose.runtime.getValue
import androidx.compose.runtime.derivedStateOf
import com.monokulo.till.SaleText
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.up
import com.monokulo.till.GroundLook
import androidx.compose.runtime.remember
import com.monokulo.till.words
import com.monokulo.till.sharedGround
import com.monokulo.till.rememberGroundTrack
import com.monokulo.till.sales.TillSale

/** The line under a code's amount: what's left of it, or how far the payment has got. */
object CodeText {
    /** "14:52 left" while the code counts down (waiting, or part paid); null once a payment is on its way. */
    fun left(sale: TillSale, secondsLeft: Long): String? =
        if (sale.phase.countsDown) "${SaleText.clock(secondsLeft)} left" else null

    /** "0.05552 XMR · 14:52 left", or "0.05552 XMR · Seen on the network" once seen, at a till on [till]. */
    fun under(sale: TillSale, till: PointOfSale?, secondsLeft: Long): String =
        "${SaleText.xmr(sale)} · ${left(sale, secondsLeft) ?: SaleText.status(sale, till)}"
}

/**
 * The code enlarged (the design's CodeB, note s3), as the engine wrote it
 * ([SaleFace.code]): the whole screen white whatever the theme, at full
 * brightness and kept on, the code as large as the screen allows. A tap
 * anywhere or the shrink button goes back ([onBack]; Android's back is the
 * activity's, which previews it); the engine goes back too once the sale
 * stops being live (paid, ended, closed), where the sale's own page says
 * what happened.
 */
@Composable
fun CodeScreen(sale: SaleFace, onBack: () -> Unit) {
    val code = sale.code
    KeepScreenOn()
    // Once the white has spread to the screen (a brightness jump mid-grow would flash the whole display, M8), and
    // through a back gesture until it commits: a cancelled one springs back to the code at the brightness it had.
    if (up()) FullBrightness()
    // The bars' icons turn dark as the white passes half the screen change, and back as it does on the way out (M8):
    // read from the stage's line, so this recomposes twice a move, not per frame.
    val stage = LocalStage.current
    val white by remember(stage) { derivedStateOf { stage == null || stage.shown() >= Nav.HALF_WAY } }
    if (white) DarkBarIcons()
    // The design's light roles, fixed: this screen is white in both themes.
    val ink = LightTokens.ink
    val muted = LightTokens.muted
    CompositionLocalProvider(LocalContentColor provides ink) {
        Box(
            Modifier.fillMaxSize()
                .clickable(onClickLabel = "Shrink the code", indication = null, interactionSource = null, onClick = onBack),
        ) {
            // The sale's white code card, spread to the screen (M8): a ground with nothing in it, the code its own layer over it.
            val ground = rememberGroundTrack(codeGround(sale.orderId))
            Box(
                Modifier.fillMaxSize()
                    .sharedGround(codeGround(sale.orderId), Move.CodeGrow, GroundLook(Fixed.White, CodeScreenCorner), ground),
            )
            Column(
                Modifier.fillMaxSize().safeDrawingPadding().padding(start = 6.dp, end = 6.dp, top = 20.dp, bottom = 28.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                // The heading and the foot come in over the white once the sale's words have gone, at their own size.
                Row(Modifier.fillMaxWidth().words(over = Move.CodeGrow, on = ground).padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(code.amount, style = display(30, -0.5), color = ink)
                        // The engine writes the time left and draws it again each second (docs/decisions/core.md 37).
                        Text(code.under, fontSize = 15.sp, color = muted, style = LocalTextStyle.current.merge(tabular))
                    }
                    Box(
                        Modifier.size(48.dp).clip(CircleShape).background(LightTokens.inset).clickable(role = Role.Button, onClick = onBack),
                        contentAlignment = Alignment.Center,
                    ) { Icon(MkIcons.Shrink, contentDescription = "Shrink the code", tint = ink, modifier = Modifier.size(20.dp)) }
                }
                BoxWithConstraints(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    QrCode(
                        code.code,
                        "Payment code, tap to shrink",
                        Modifier.size(min(maxWidth, maxHeight)),
                        padding = 12.dp,
                        radius = 0.dp,
                        shared = sale.orderId,
                        sharedCard = false,
                    )
                }
                Column(Modifier.words(over = Move.CodeGrow, on = ground), horizontalAlignment = Alignment.CenterHorizontally) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        PhaseRing(code.ring, 26.dp, track = Fixed.Track, fill = Fixed.Accent, paid = Fixed.Paid)
                        Text("Scan with any Monero wallet", fontSize = 16.sp, fontWeight = FontWeight.Bold, color = ink)
                    }
                    Text("Tap anywhere to shrink", fontSize = 13.sp, color = muted, modifier = Modifier.padding(top = 6.dp))
                }
            }
        }
    }
}

/** Keeps the screen on while it's shown: a code a customer is about to scan mustn't go dark. */
@Composable
fun KeepScreenOn() {
    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
}

/** The window at full brightness while shown, as it was after. */
@Composable
private fun FullBrightness() {
    val view = LocalView.current
    DisposableEffect(view) {
        val window = view.context.activity()?.window ?: return@DisposableEffect onDispose { }
        val brightness = window.attributes.screenBrightness
        window.attributes = window.attributes.apply { screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_FULL }
        onDispose { window.attributes = window.attributes.apply { screenBrightness = brightness } }
    }
}

/** Dark status and navigation bar icons, over the white, while composed; as they were after. */
@Composable
private fun DarkBarIcons() {
    val view = LocalView.current
    DisposableEffect(view) {
        val window = view.context.activity()?.window ?: return@DisposableEffect onDispose { }
        val bars = WindowCompat.getInsetsController(window, view)
        val lightStatus = bars.isAppearanceLightStatusBars
        val lightNavigation = bars.isAppearanceLightNavigationBars
        bars.isAppearanceLightStatusBars = true
        bars.isAppearanceLightNavigationBars = true
        onDispose {
            bars.isAppearanceLightStatusBars = lightStatus
            bars.isAppearanceLightNavigationBars = lightNavigation
        }
    }
}

private tailrec fun Context.activity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}
