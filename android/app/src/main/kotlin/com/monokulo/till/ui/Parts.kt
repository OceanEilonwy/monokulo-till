package com.monokulo.till.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * A screen's frame: the ground colour, clear of the system bars, at most
 * [maxWidth] wide and centred on a tablet or in a wide window. [content]
 * scrolls; [bottom] stays at the foot (the design's button stacks).
 */
@Composable
fun Page(
    maxWidth: Dp = 560.dp,
    bottom: (@Composable ColumnScope.() -> Unit)? = null,
    top: (@Composable () -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(Modifier.fillMaxSize(), color = mk.ground, contentColor = mk.ink) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = maxWidth).fillMaxWidth().fillMaxSize()) {
                top?.invoke()
                Column(
                    Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()),
                    content = content,
                )
                if (bottom != null) {
                    Column(
                        Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 16.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        content = bottom,
                    )
                }
            }
        }
    }
}

/** A screen's heading (Manrope, as the design's h1). */
@Composable
fun Heading(text: String, modifier: Modifier = Modifier, size: Int = 30, align: TextAlign = TextAlign.Start) = Text(
    text,
    style = display(size),
    textAlign = align,
    modifier = modifier.semantics { heading() },
)

@Composable
fun Lead(text: String, modifier: Modifier = Modifier, align: TextAlign = TextAlign.Start, size: Int = 16) =
    Text(text, color = mk.muted, fontSize = size.sp, lineHeight = (size + 7).sp, textAlign = align, modifier = modifier)

@Composable
fun Muted(text: String, modifier: Modifier = Modifier, size: Int = 13, align: TextAlign = TextAlign.Start) =
    Text(text, color = mk.muted, fontSize = size.sp, lineHeight = (size + 5).sp, textAlign = align, modifier = modifier)

/** "Step 2 of 6". */
@Composable
fun StepLabel(step: Int, of: Int = 6) = Muted("Step $step of $of")

/** The orange button: one per screen, led by [icon] when it has one (what it does, owner's round 4). */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 52.dp, size: Int = 17, icon: ImageVector? = null) =
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = height),
        shape = CircleShape,
        colors = ButtonDefaults.buttonColors(
            containerColor = mk.accent, contentColor = mk.onAccent,
            disabledContainerColor = mk.chip, disabledContentColor = mk.muted,
        ),
        contentPadding = PaddingValues(horizontal = 20.dp),
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.size(8.dp))
        }
        Text(text, fontSize = size.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center)
    }

/**
 * Back to the point of sale, as iOS draws it (owner, round 4): silver, led
 * by the point of sale's mark, its words on one line however narrow.
 */
@Composable
fun BackToPosButton(text: String, mark: uniffi.till_screens.PosMark, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 56.dp) =
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = height),
        shape = CircleShape,
        colors = ButtonDefaults.buttonColors(
            containerColor = Silver, contentColor = Fixed.Ink,
            disabledContainerColor = mk.chip, disabledContentColor = mk.muted,
        ),
        contentPadding = PaddingValues(horizontal = 12.dp),
    ) {
        PosMark(mark, 18.dp, tint = if (mark == uniffi.till_screens.PosMark.SQUARE) Fixed.Ink else null)
        Spacer(Modifier.size(8.dp))
        Text(text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, softWrap = false)
    }

/** The point-of-sale button's silver, the same in light and dark (iOS's). */
private val Silver = Color(0xFFD6D9DE)

/** The quiet button beside it: an outline in the design's outline colour. */
@Composable
fun OutlineButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    height: Dp = 48.dp,
    size: Int = 16,
    icon: ImageVector? = null,
    /** Its words on one line, beside another button (owner, round 4). */
    oneLine: Boolean = false,
) = OutlinedButton(
    onClick = onClick,
    enabled = enabled,
    modifier = modifier.heightIn(min = height),
    shape = CircleShape,
    border = BorderStroke(1.dp, mk.outline),
    colors = ButtonDefaults.outlinedButtonColors(contentColor = mk.ink),
    contentPadding = PaddingValues(horizontal = if (oneLine) 12.dp else 18.dp),
) {
    if (icon != null) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.size(8.dp))
    }
    Text(
        text, fontSize = size.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center,
        maxLines = if (oneLine) 1 else Int.MAX_VALUE, softWrap = !oneLine,
    )
}

/** A text button in the link colour ("Not now", "Use notifications only"). */
@Composable
fun LinkButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, size: Int = 17, color: Color = mk.link) =
    TextButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.heightIn(min = 48.dp),
        colors = ButtonDefaults.textButtonColors(contentColor = color),
    ) { Text(text, fontSize = size.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center) }

/** A round icon button, 48 dp to touch: the design's back, close and overflow buttons. */
@Composable
fun RoundIconButton(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    filled: Boolean = false,
    iconSize: Dp = 18.dp,
) = IconButton(
    onClick = onClick,
    modifier = modifier.size(48.dp),
    colors = IconButtonDefaults.iconButtonColors(
        containerColor = if (filled) mk.chip else Color.Transparent,
        contentColor = mk.ink,
    ),
) { Icon(icon, contentDescription = label, modifier = Modifier.size(iconSize)) }

/**
 * A round button drawn [visual] wide on [ground] (the sale's 44 dp cog and
 * tinted X), still 48 dp to touch.
 */
@Composable
fun CircleButton(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit,
    ground: Color,
    tint: Color,
    iconSize: Dp,
    modifier: Modifier = Modifier,
    visual: Dp = 44.dp,
) = IconButton(onClick = onClick, modifier = modifier.size(48.dp)) {
    Box(Modifier.size(visual).clip(CircleShape).background(ground), contentAlignment = Alignment.Center) {
        Icon(icon, contentDescription = label, tint = tint, modifier = Modifier.size(iconSize))
    }
}

/** The top of an onboarding step: a back arrow. */
@Composable
fun BackBar(onBack: (() -> Unit)?) = Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
    if (onBack != null) RoundIconButton(MkIcons.Back, "Back", onBack)
}

/** A grouped list's label: "STORE". */
@Composable
fun SectionLabel(text: String) = Text(
    text.uppercase(),
    color = mk.muted,
    fontSize = 13.sp,
    modifier = Modifier.padding(start = 32.dp, end = 32.dp, top = 22.dp, bottom = 6.dp).semantics { heading() },
)

/** A grouped list: rows on a card, lines between them. */
@Composable
fun Group(modifier: Modifier = Modifier, rows: List<@Composable () -> Unit>) = Column(
    modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(mk.card),
) {
    rows.forEachIndexed { i, row ->
        if (i > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
        row()
    }
}

/** A name and its value on one row, at least 48 dp tall. */
@Composable
fun FactRow(name: String, value: String, mono: Boolean = false, valueColor: Color = mk.muted, trailing: (@Composable RowScope.() -> Unit)? = null) = Row(
    Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 10.dp)
        .semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    // The name keeps its line; a long value wraps under itself, at the end.
    Text(name, fontSize = 17.sp)
    Text(
        value,
        color = valueColor,
        fontSize = if (mono) 15.sp else 17.sp,
        fontFamily = if (mono) FontFamily.Monospace else null,
        textAlign = TextAlign.End,
        modifier = Modifier.weight(1f),
    )
    trailing?.invoke(this)
}

/** A footnote under a group. */
@Composable
fun Footnote(text: String) = Muted(text, Modifier.padding(start = 32.dp, end = 32.dp, top = 6.dp))

/** A switch row: the whole row toggles, and TalkBack hears one switch. */
@Composable
fun SwitchRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit, enabled: Boolean = true, detail: String? = null) = Row(
    Modifier.fillMaxWidth().heightIn(min = 48.dp)
        .toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onChange)
        .padding(horizontal = 16.dp, vertical = 8.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    Column(Modifier.weight(1f)) {
        Text(label, fontSize = 17.sp, color = if (enabled) mk.ink else mk.outline)
        if (detail != null) Muted(detail)
    }
    Switch(
        checked = checked,
        onCheckedChange = null,
        enabled = enabled,
        colors = SwitchDefaults.colors(
            checkedTrackColor = mk.accent, checkedThumbColor = Color.White, checkedBorderColor = Color.Transparent,
            uncheckedTrackColor = mk.switchOff, uncheckedThumbColor = Color.White, uncheckedBorderColor = Color.Transparent,
        ),
    )
}

/**
 * One of a group's choices (Settings › Wake-ups): the whole row picks it,
 * and the chosen one has a tick at its end. A radio button to TalkBack,
 * read with its [detail]; [leading], decoration, goes before the label
 * (a point of sale's mark). Put the rows in a `Group` with
 * `Modifier.selectableGroup()`.
 */
@Composable
fun ChoiceRow(label: String, selected: Boolean, onClick: () -> Unit, detail: String? = null, leading: (@Composable () -> Unit)? = null) = Row(
    Modifier.fillMaxWidth().heightIn(min = 48.dp)
        .selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
        .padding(horizontal = 16.dp, vertical = 8.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    leading?.invoke()
    Column(Modifier.weight(1f)) {
        Text(label, fontSize = 17.sp)
        if (detail != null) Muted(detail)
    }
    // The tick is decoration: "selected" is said by the row's semantics.
    Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
        if (selected) Icon(MkIcons.Tick, contentDescription = null, tint = mk.ink, modifier = Modifier.size(20.dp))
    }
}

/** A green circle with a white tick: paid. */
@Composable
fun PaidTick(size: Dp) = Box(
    Modifier.size(size).clip(CircleShape).background(Fixed.Paid),
    contentAlignment = Alignment.Center,
) { Icon(MkIcons.Tick, contentDescription = null, tint = Fixed.White, modifier = Modifier.size(size * 0.55f)) }

/** A problem to show: on the inset colour, not shouted. */
@Composable
fun Notice(text: String?, modifier: Modifier = Modifier) {
    if (text == null) return
    Text(
        text,
        fontSize = 15.sp,
        lineHeight = 21.sp,
        modifier = modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(mk.inset)
            .border(1.dp, mk.edge, RoundedCornerShape(12.dp)).padding(14.dp),
    )
}

/**
 * What the engine is busy with and what went wrong, where the screen's own view doesn't say it
 * (`Update::Work`'s `shownBusy` and `shownProblem`, docs/decisions/core.md 109): a thin bar while [busy], and the
 * [problem] as a [Notice], over the top of the screen.
 */
@Composable
fun WorkBar(busy: Boolean, problem: String?) = Column(Modifier.fillMaxWidth().safeDrawingPadding()) {
    if (busy) LinearProgressIndicator(Modifier.fillMaxWidth().height(2.dp), color = mk.accent)
    Notice(problem, Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
}

/** The one confirming buzz a payment gets (`docs/motion.md` §6); the system's haptic settings apply. */
fun android.view.View.confirm() {
    performHapticFeedback(if (android.os.Build.VERSION.SDK_INT >= 30) android.view.HapticFeedbackConstants.CONFIRM else android.view.HapticFeedbackConstants.CONTEXT_CLICK)
}
