package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.input.key.type
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.min
import androidx.compose.ui.unit.sp
import uniffi.till_app.Action
import uniffi.till_screens.AsideSale
import uniffi.till_screens.KeypadButton
import uniffi.till_screens.KeypadKey
import uniffi.till_screens.KeypadView

/**
 * Till's keypad, the home of a till with no other point of sale (docs/view-key-mode.md §7), modelled on Monokulo's
 * web POS: the sales put aside for the next customer over the amount, the keys, an optional reference and Charge.
 * "Sales" opens the shift's list and Settings is the shift's round button. A hardware keyboard types too: digits, the
 * point, Backspace, Escape to clear and Enter to charge. The keys share the height left, between a phone held in one
 * hand's 60 dp and a counter tablet's 96, in a column no wider than a hand spans on a tablet.
 */
@Composable
fun KeypadScreen(view: KeypadView, send: (Action) -> Unit) {
    val focus = remember { FocusRequester() }
    var typing by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Surface(Modifier.fillMaxSize(), color = mk.ground, contentColor = mk.ink) {
        Column(
            Modifier.fillMaxSize().safeDrawingPadding().focusRequester(focus).focusable()
                .onKeyEvent { event -> !typing && keypadKey(event)?.let { send(it); true } == true },
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp).heightIn(min = 56.dp), verticalAlignment = Alignment.CenterVertically) {
                LinkButton(view.sales.label, { send(Action.OpenSales) })
                Spacer(Modifier.weight(1f))
                CircleButton(MkIcons.Settings, view.settings, { send(Action.OpenSettings) }, ground = mk.chip, tint = mk.ink, iconSize = 20.dp)
            }
            BoxWithConstraints(Modifier.weight(1f).widthIn(max = 520.dp).fillMaxWidth().padding(horizontal = 16.dp)) {
                val keyHeight = min(112.dp, maxOf(56.dp, (maxHeight - if (view.aside.isEmpty()) 250.dp else 330.dp) / 4 - 10.dp))
                Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    if (view.aside.isNotEmpty()) Aside(view.asideTitle, view.aside, send)
                    Spacer(Modifier.weight(1f))
                    Amount(view, send)
                    Keys(view.keys, keyHeight, send)
                    Reference(view, onTyping = { typing = it }, send = send)
                    view.problem?.let { Text(it, color = mk.danger, fontSize = 15.sp, modifier = Modifier.fillMaxWidth()) }
                    PrimaryButton(view.charge.label, { send(Action.Charge) }, Modifier.fillMaxWidth(), enabled = view.charge.enabled, height = 60.dp, size = 19)
                    Spacer(Modifier.height(4.dp))
                }
            }
        }
    }
}

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
private fun Keys(keys: List<KeypadButton>, height: Dp, send: (Action) -> Unit) = Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
    keys.chunked(3).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
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

/** The optional reference: whom it's for or a note, which names the sale; Enter charges. */
@Composable
private fun Reference(view: KeypadView, onTyping: (Boolean) -> Unit, send: (Action) -> Unit) {
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
        keyboardActions = KeyboardActions(onDone = { send(Action.Charge) }),
        colors = fieldColours(),
        modifier = Modifier.fillMaxWidth().onFocusChanged { onTyping(it.isFocused) },
    )
}

/** The sales put aside, still live: a chip each, sideways, opening the sale's code. */
@Composable
private fun Aside(title: String?, sales: List<AsideSale>, send: (Action) -> Unit) = Column(Modifier.padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
    title?.let { Muted(it.uppercase(), size = 12) }
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        sales.forEach { sale ->
            Column(
                Modifier.clip(ChipShape).background(mk.card).border(1.dp, mk.edge, ChipShape)
                    .clickable(role = Role.Button) { send(Action.OpenSale(sale.orderId)) }
                    .clearAndSetSemantics { contentDescription = sale.said }
                    .heightIn(min = 48.dp).padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(sale.label, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
                    Text(sale.amount, fontSize = 15.sp, maxLines = 1)
                }
                Text(sale.status, fontSize = 12.sp, color = mk.muted, maxLines = 1)
            }
        }
    }
}

private val KeyShape = RoundedCornerShape(16.dp)
private val ChipShape = RoundedCornerShape(12.dp)
