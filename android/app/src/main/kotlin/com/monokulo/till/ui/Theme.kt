package com.monokulo.till.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.monokulo.till.R

/**
 * The design's colours (the mockups' inline styles), by role. Light is the
 * design; dark keeps the same roles on a warm dark ground, each text pair
 * at least 4.5:1.
 */
@Immutable
data class Tokens(
    /** The page. */
    val ground: Color,
    /** Text and icons. */
    val ink: Color,
    /** Secondary text. */
    val muted: Color,
    /** Cards and grouped rows. */
    val card: Color,
    /** Lines between rows inside a card. */
    val divider: Color,
    /** A card's or a quiet button's edge. */
    val edge: Color,
    /** Round icon buttons, the empty part of progress. */
    val chip: Color,
    /** The status pill, inset panels. */
    val inset: Color,
    /** Outlined buttons. */
    val outline: Color,
    /** Text links and small accents. */
    val link: Color,
    /** The one primary action, with [onAccent] on it. */
    val accent: Color,
    val onAccent: Color,
    /** Paid, as text and ticks. */
    val paid: Color,
    /** "No payment yet": the clock and its circle. */
    val warn: Color,
    val warnGround: Color,
    /** Disconnect; a reversed payment; the tinted X that closes a sale. */
    val danger: Color,
    /** Behind [danger]: the X's circle, a reversed sale's icon and note. */
    val dangerGround: Color,
    /** Text on [dangerGround]: the reversed sale's note. */
    val onDangerGround: Color,
    /** Behind [paid]: the tick's circle on a paid row or page. */
    val paidGround: Color,
    /** A switch that's off. */
    val switchOff: Color,
    val dark: Boolean,
)

/** Colours that don't change with the theme (the QR card, the bubble, the accent). */
object Fixed {
    val Accent = Color(0xFFFF6600)
    val OnAccent = Color(0xFF111111)
    val Ink = Color(0xFF1A1917)
    val Ground = Color(0xFFF4F1EA)
    val Muted = Color(0xFF5A564C)
    val Paid = Color(0xFF1A7F37)
    /** The paid bubble's lens: its lightest facets, [PaidMid] its middle ones, [Paid] its core (decision 267). */
    val PaidLight = Color(0xFF2EA052)
    val PaidMid = Color(0xFF238A43)
    /** The waiting bubble's border: the pale track an [Ink] segment chases round. */
    val ChaseTrack = Color(0xFFD8D2C7)
    /** The idle bubble's border, round its grey mark. */
    val IdleBorder = Color(0xFF565656)
    /** The bubble's disc in dark mode, so it doesn't glare over a dark app: the dark theme's card grey. */
    val BubbleDark = Color(0xFF2A2825)
    /** On [BubbleDark]: the waiting border's track, and the light segment ([BubbleDarkInk]) chasing round it. */
    val BubbleDarkTrack = Color(0xFF4A4743)
    /** On [BubbleDark], where [Ink] is on white: the chasing segment and the whole border, waiting still or paid. */
    val BubbleDarkInk = Color(0xFFECE8DF)
    /** The idle bubble's border on [BubbleDark]. */
    val BubbleDarkIdleBorder = Color(0xFF8C8577)
    val Track = Color(0xFFE4DFD5)
    val Outline = Color(0xFF8C8577)
    /** The QR card and the enlarged code's screen; a glyph on [Ink]. */
    val White = Color.White
    /** Over the sale, behind the close sheet: the design's 62% near-black. */
    val Scrim = Color(0x9E0A0908)
}

val LightTokens = Tokens(
    ground = Fixed.Ground,
    ink = Fixed.Ink,
    muted = Fixed.Muted,
    card = Color.White,
    divider = Color(0xFFECE6DA),
    edge = Color(0xFFDCD5C7),
    chip = Fixed.Track,
    inset = Color(0xFFEFEAE0),
    outline = Fixed.Outline,
    link = Color(0xFFB34700),
    accent = Fixed.Accent,
    onAccent = Fixed.OnAccent,
    paid = Fixed.Paid,
    warn = Color(0xFF7D5E00),
    warnGround = Color(0xFFF1E3CF),
    danger = Color(0xFFB00020),
    dangerGround = Color(0xFFF6DCD9),
    onDangerGround = Color(0xFF6E1A1A),
    paidGround = Color(0xFFDCEFE2),
    switchOff = Color(0xFFC9C3B8),
    dark = false,
)

val DarkTokens = Tokens(
    ground = Color(0xFF1C1B19),
    ink = Color(0xFFECE8DF),
    muted = Color(0xFFA8A29A),
    card = Color(0xFF2A2825),
    divider = Color(0xFF3A3733),
    edge = Color(0xFF4A4743),
    chip = Color(0xFF3A3733),
    inset = Color(0xFF2A2825),
    outline = Fixed.Outline,
    link = Color(0xFFFF8A3D),
    accent = Fixed.Accent,
    onAccent = Fixed.OnAccent,
    paid = Color(0xFF4CC27A),
    warn = Color(0xFFE8C267),
    warnGround = Color(0xFF3D3320),
    danger = Color(0xFFFF6B6B),
    dangerGround = Color(0xFF3D2422),
    onDangerGround = Color(0xFFFFD2CE),
    paidGround = Color(0xFF1E3A28),
    switchOff = Color(0xFF5A564C),
    dark = true,
)

val LocalTokens = staticCompositionLocalOf { LightTokens }

/** The current theme's colours. */
val mk: Tokens @Composable get() = LocalTokens.current

/** Manrope (SIL OFL 1.1, `assets/licenses/manrope-OFL.txt`), bundled: its variable font at the weights the design uses. */
val Manrope = FontFamily(
    Font(R.font.manrope, FontWeight.Medium, variationSettings = FontVariation.Settings(FontVariation.weight(500))),
    Font(R.font.manrope, FontWeight.Bold, variationSettings = FontVariation.Settings(FontVariation.weight(700))),
    Font(R.font.manrope, FontWeight.ExtraBold, variationSettings = FontVariation.Settings(FontVariation.weight(800))),
)

/** Display type: Manrope ExtraBold at [size] sp, tracked in by [tracking] sp as in the design. */
fun display(size: Int, tracking: Double = -0.4) = TextStyle(
    fontFamily = Manrope,
    fontWeight = FontWeight.ExtraBold,
    fontSize = size.sp,
    letterSpacing = tracking.sp,
    lineHeight = (size * 1.15).sp,
)

/**
 * Figures that all take the same width ("tnum"): set wherever a countdown
 * (the engine's countdown) ticks, so its digits change without the
 * words around them shifting.
 */
val tabular = TextStyle(fontFeatureSettings = "tnum")

/** Monokulo's palette: warm paper, ink, one orange primary; Manrope for display, Roboto for the rest. */
@Composable
fun TillTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val t = if (dark) DarkTokens else LightTokens
    val colors = if (dark) darkColorScheme() else lightColorScheme()
    val scheme = colors.copy(
        primary = t.accent, onPrimary = t.onAccent,
        background = t.ground, onBackground = t.ink,
        surface = t.ground, onSurface = t.ink,
        surfaceVariant = t.card, onSurfaceVariant = t.muted,
        surfaceContainer = t.card, surfaceContainerHigh = t.card, surfaceContainerLow = t.card,
        outline = t.outline, outlineVariant = t.edge,
        error = t.danger,
    )
    val base = Typography()
    val type = base.copy(
        displayLarge = base.displayLarge.copy(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold),
        displayMedium = base.displayMedium.copy(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold),
        displaySmall = base.displaySmall.copy(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold),
        headlineLarge = base.headlineLarge.copy(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold),
        headlineMedium = base.headlineMedium.copy(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold),
        headlineSmall = base.headlineSmall.copy(fontFamily = Manrope, fontWeight = FontWeight.ExtraBold),
    )
    CompositionLocalProvider(LocalTokens provides t, LocalMotion provides rememberMotion()) {
        MaterialTheme(colorScheme = scheme, typography = type, content = content)
    }
}
