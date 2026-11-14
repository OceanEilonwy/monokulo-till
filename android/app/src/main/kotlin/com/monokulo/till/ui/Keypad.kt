package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.till_app.Action
import uniffi.till_screens.KeypadButton
import uniffi.till_screens.KeypadKey
import uniffi.till_screens.KeypadView

/**
 * Till's keypad (docs/view-key-mode.md §7), modelled on Monokulo's web POS: from the top, an optional reference, the
 * amount, the keys and Charge, so the keys sit between the field and Charge and a tap meant for the field can't charge
 * (core.md decision 258). It opens from the sales' New sale (decision 257); [closable], its X goes back to them. Beside
 * the sales on a wide screen there's nothing to close. A hardware keyboard types too: digits, the point, Backspace,
 * Escape to clear and Enter to charge; Enter in the reference only hands the typing back to the keys.
 *
 * The keys keep the iPhone's shape, [KEY_RATIO] tall for their width, and never stretch wide: on a bigger screen the
 * column narrows so the keys keep that shape and everything fits the height; on a phone they stay between 56 and 112 dp
 * tall, following the shape where the width allows.
 */
@Composable
fun KeypadScreen(view: KeypadView, closable: Boolean = true, send: (Action) -> Unit) {
    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    var typing by remember { mutableStateOf(false) }
    // Done or Enter in the reference: the keyboard goes and the keys have the typing again. It never charges.
    val backToKeys = {
        keyboard?.hide()
        runCatching { focus.requestFocus() }
        Unit
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Surface(Modifier.fillMaxSize(), color = mk.ground, contentColor = mk.ink) {
        Column(
            Modifier.fillMaxSize().safeDrawingPadding().focusRequester(focus).focusable()
                .onKeyEvent { event -> !typing && keypadKey(event)?.let { send(it); true } == true },
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(
                Modifier.fillMaxWidth().height(64.dp).padding(start = if (closable) 8.dp else 24.dp, end = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (closable) CircleButton(MkIcons.Close, view.close, { send(Action.Back) }, ground = mk.chip, tint = mk.ink, iconSize = 14.dp)
                Text(view.title, style = display(22, -0.3), maxLines = 1, modifier = Modifier.semantics { heading() })
            }
            BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
                val fromWidth = (maxWidth - SIDES * 2 - GAP * 2) / 3 * KEY_RATIO
                val fromHeight = (maxHeight - reserved(view) - GAP * 3) / 4
                val keyHeight = minOf(fromWidth, fromHeight).coerceIn(56.dp, 112.dp)
                val width = minOf(maxWidth, keyHeight / KEY_RATIO * 3 + GAP * 2 + SIDES * 2)
                Column(
                    Modifier.align(Alignment.TopCenter).width(width).fillMaxHeight().padding(horizontal = SIDES),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Reference(view, onTyping = { typing = it }, onDone = backToKeys, send = send)
                    Spacer(Modifier.weight(1f))
                    Amount(view, send)
                    Keys(view.keys, keyHeight, send)
                    view.problem?.let { Text(it, color = mk.danger, fontSize = 15.sp, modifier = Modifier.fillMaxWidth()) }
                    PrimaryButton(view.charge.label, { send(Action.Charge) }, Modifier.fillMaxWidth(), enabled = view.charge.enabled, height = 60.dp, size = 19)
                    Spacer(Modifier.height(4.dp))
                }
            }
        }
    }
}

/** A key's height for its width: the iPhone keypad's 113 × 84. */
const val KEY_RATIO = 84f / 113f

/** The space between keys. */
private val GAP = 10.dp

/** The keypad column's side margins. */
private val SIDES = 16.dp

/** The height the keys leave to the rest of the column: the reference, the amount, Charge, the problem and the gaps. */
private fun reserved(view: KeypadView): Dp = 262.dp + if (view.problem != null) 34.dp else 0.dp

/** What a hardware key does on the keypad: digits and the point type, Backspace deletes, Escape clears, Enter charges. */
fun keypadKey(event: KeyEvent): Action? {
    if (event.type != KeyEventType.KeyDown) return null
    val digit = when (event.key) {
        Key.Zero, Key.NumPad0 -> 0
        Key.One, Key.NumPad1 -> 1
        Key.Two, Key.NumPad2 -> 2
        Key.Three, Key.NumPad3 -> 3
        Key.Four, Key.NumPad4 -> 4
        Key.Five, Key.NumPad5 -> 5
        Key.Six, Key.NumPad6 -> 6
        Key.Seven, Key.NumPad7 -> 7
        Key.Eight, Key.NumPad8 -> 8
        Key.Nine, Key.NumPad9 -> 9
        else -> null
    }
    if (digit != null) return Action.KeypadPress(KeypadKey.Digit(digit.toUByte()))
    return when (event.key) {
        Key.Backspace, Key.Delete -> Action.KeypadPress(KeypadKey.Backspace)
        Key.Escape -> Action.KeypadPress(KeypadKey.Clear)
        Key.Period, Key.Comma, Key.NumPadDot -> Action.KeypadPress(KeypadKey.Point)
        Key.Enter, Key.NumPadEnter -> Action.Charge
        else -> null
    }
}

/** The amount, large, shrinking to fit; XMR's Clear beside it, since the point takes its key. */
@Composable
private fun Amount(view: KeypadView, send: (Action) -> Unit) = Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
    Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
        val size = when {
            view.amount.length > 16 -> 32
            view.amount.length > 11 -> 42
            else -> 56
        }
        Text(
            view.amount, style = display(size, -1.0), maxLines = 1, textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = view.amountSaid; liveRegion = LiveRegionMode.Polite },
        )
    }
    view.clear?.let { clear -> LinkButton(clear.label, { send(Action.KeypadPress(clear.key)) }, Modifier.semantics { contentDescription = clear.said }, size = 20) }
}

/** Four rows of three keys. */
@Composable
private fun Keys(keys: List<KeypadButton>, height: Dp, send: (Action) -> Unit) = Column(verticalArrangement = Arrangement.spacedBy(GAP)) {
    keys.chunked(3).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(GAP)) {
            row.forEach { key -> Key(key, height, Modifier.weight(1f)) { send(Action.KeypadPress(key.key)) } }
        }
    }
}

/** One key: its label large on a card, ⌫ as the delete shape. */
@Composable
private fun Key(key: KeypadButton, height: Dp, modifier: Modifier, onPress: () -> Unit) = Box(
    modifier.height(height).clip(KeyShape).background(mk.card).border(1.dp, mk.edge, KeyShape)
        .clickable(role = Role.Button, onClick = onPress).clearAndSetSemantics { contentDescription = key.said },
    contentAlignment = Alignment.Center,
) {
    if (key.key == KeypadKey.Backspace) {
        Icon(MkIcons.Backspace, contentDescription = null, modifier = Modifier.size(28.dp))
    } else {
        Text(key.label, fontSize = minOf(40f, height.value * 0.45f).sp, fontWeight = FontWeight.Medium)
    }
}

/**
 * The optional reference: whom it's for or a note, which names the sale. Done, or Enter on a hardware keyboard, only
 * [onDone]: the keyboard goes and the keys have the typing back. Charging stays Charge's.
 */
@Composable
private fun Reference(view: KeypadView, onTyping: (Boolean) -> Unit, onDone: () -> Unit, send: (Action) -> Unit) {
    var text by remember { mutableStateOf(view.reference) }
    LaunchedEffect(view.reference) { if (view.reference != text) text = view.reference }
    OutlinedTextField(
        text,
        { typed ->
            text = typed.take(view.referenceMax.toInt())
            send(Action.KeypadReference(text))
        },
        label = { Text(view.referenceLabel) },
        placeholder = { Text(view.referencePlaceholder) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { onDone() }),
        colors = fieldColours(),
        modifier = Modifier.fillMaxWidth().padding(top = 4.dp).onFocusChanged { onTyping(it.isFocused) }
            .onPreviewKeyEvent { event ->
                // Enter is the field's own, down and up, so it never reaches the keys' Enter (which charges).
                val enter = event.key == Key.Enter || event.key == Key.NumPadEnter
                if (enter && event.type == KeyEventType.KeyDown) onDone()
                enter
            },
    )
}

private val KeyShape = RoundedCornerShape(16.dp)
