package com.monokulo.till.ui

import android.Manifest
import android.content.pm.PackageManager
import android.text.InputType
import android.util.TypedValue
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading as headingRole
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.monokulo.till.R
import uniffi.till_screens.CheckMark
import uniffi.till_screens.Choice
import uniffi.till_screens.FobConnectedStep
import uniffi.till_screens.FobPattern
import uniffi.till_screens.FobRequestStep
import uniffi.till_screens.KeyField
import uniffi.till_screens.ModeChoice
import uniffi.till_screens.ModeStep
import uniffi.till_screens.NodeRow
import uniffi.till_screens.PaymentsMode
import uniffi.till_screens.Press
import uniffi.till_screens.SetupStepper
import uniffi.till_screens.ThisTillStep
import uniffi.till_screens.WalletCodeStep
import uniffi.till_screens.WalletEntryStep
import uniffi.till_screens.WalletKeysStep
import uniffi.till_screens.WalletNetwork
import uniffi.till_screens.WalletSeedStep
import uniffi.till_screens.WalletWay

// The wallet setup's steps (docs/view-key-mode.md §11.1; the canvas's Vk* boards), in round 4's one column: V1 how
// Till sees payments, V2 the wallet, C1 Connect to Fob, B4/C4 connected, D1 to D3 the wallet entered by hand, D4
// this till. Every word is the engine's; each screen takes its step and says what was pressed (decision 316).

/** A step's heading and lead, as setup's other stages draw theirs. */
@Composable
private fun Head(heading: String, lead: String, eyebrow: String? = null, done: Boolean = false) = Column(
    Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp),
    verticalArrangement = Arrangement.spacedBy(10.dp),
) {
    eyebrow?.let { Text(it.uppercase(), color = mk.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold) }
    if (done) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            PaidTick(32.dp)
            Heading(heading)
        }
    } else {
        Heading(heading)
    }
    if (lead.isNotEmpty()) Lead(lead)
}

/** The step's buttons at the foot: the main one, then the quiet one, then "Step 2 of 4". */
@Composable
private fun Foot(next: Press?, onNext: () -> Unit, stepLabel: String?, quiet: Press? = null, onQuiet: () -> Unit = {}) {
    next?.let { PrimaryButton(it.label, onNext, Modifier.fillMaxWidth(), enabled = it.enabled && !it.busy) }
    quiet?.let { LinkButton(it.label, onQuiet, enabled = it.enabled) }
    if (stepLabel != null) Muted(stepLabel)
}

/** V1, How will Till see payments: three cards, the chosen one ringed and ticked; one not built yet says so. */
@Composable
fun PaymentsModeScreen(view: ModeStep, heading: String, lead: String, stepLabel: String?, onChoose: (PaymentsMode) -> Unit, onNext: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = {
        Muted(view.footnote, align = TextAlign.Center)
        Foot(view.next, onNext, stepLabel)
    },
) {
    Head(heading, lead)
    Column(
        Modifier.padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 16.dp).selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) { view.choices.forEach { ModeCard(it) { onChoose(it.mode) } } }
}

@Composable
private fun ModeCard(choice: ModeChoice, onClick: () -> Unit) {
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier.fillMaxWidth().clip(shape).background(mk.card)
            .border(if (choice.selected) 2.dp else 1.dp, if (choice.selected) mk.ink else mk.edge, shape)
            .selectable(selected = choice.selected, enabled = choice.enabled, role = Role.RadioButton, onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 18.dp)
            .alpha(if (choice.enabled) 1f else 0.55f),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(
            Modifier.size(26.dp).clip(CircleShape).then(
                if (choice.selected) Modifier.background(mk.ink) else Modifier.border(1.6.dp, mk.outline, CircleShape),
            ),
            contentAlignment = Alignment.Center,
        ) { if (choice.selected) Icon(MkIcons.Tick, null, tint = mk.ground, modifier = Modifier.size(16.dp)) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(choice.title, fontSize = 19.sp, fontWeight = FontWeight.SemiBold)
            Text(choice.text, fontSize = 15.sp, lineHeight = 21.sp, color = mk.muted)
            choice.note?.let { Text(it, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = mk.muted) }
        }
    }
}

/** V2, Connect your wallet: Fob first, on its own card, then the ways to enter it yourself. */
@Composable
fun WalletEntryScreen(
    view: WalletEntryStep, heading: String, lead: String, stepLabel: String?,
    onConnect: () -> Unit, onWay: (WalletWay) -> Unit, onGetFob: () -> Unit, onBack: () -> Unit,
) = Page(
    top = { BackBar(onBack) },
    bottom = { Foot(null, {}, stepLabel, quiet = view.getFob, onQuiet = onGetFob) },
) {
    Head(heading, lead)
    Column(
        Modifier.padding(start = 16.dp, end = 16.dp, top = 18.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(mk.card)
            .border(2.dp, mk.ink, RoundedCornerShape(16.dp)).padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Image(painterResource(R.drawable.ic_fob_mark), contentDescription = null, modifier = Modifier.size(56.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(view.fobTitle, fontSize = 19.sp, fontWeight = FontWeight.SemiBold)
                Text(view.fobText, fontSize = 15.sp, lineHeight = 21.sp, color = mk.muted)
            }
        }
        PrimaryButton(view.connect.label, onConnect, Modifier.fillMaxWidth(), enabled = view.connect.enabled)
    }
    SectionLabel(view.section)
    Group(
        rows = view.ways.map { way ->
            {
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable(role = Role.Button) { onWay(way.way) }
                        .padding(horizontal = 16.dp, vertical = 12.dp).semantics(mergeDescendants = true) {},
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    val icon = when (way.way) {
                        WalletWay.SCAN_CODE -> MkIcons.WalletCode
                        WalletWay.KEYS -> MkIcons.Key
                        WalletWay.SEED -> MkIcons.SeedWords
                    }
                    Icon(icon, null, tint = mk.ink, modifier = Modifier.size(26.dp))
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(way.title, fontSize = 17.sp)
                        Text(way.text, fontSize = 15.sp, color = mk.muted)
                    }
                    Icon(MkIcons.Chevron, null, tint = mk.outline, modifier = Modifier.size(14.dp))
                }
            }
        },
    )
    Spacer(Modifier.height(16.dp))
}

/**
 * The request's pattern (board VkIdenticon): 5 × 5 cells from fob-codes, ink on paper, the same pixels on both
 * devices and in both themes.
 */
@Composable
fun FobPatternImage(pattern: FobPattern, size: Dp, modifier: Modifier = Modifier) {
    val ink = colour(pattern.ink)
    val paper = colour(pattern.paper)
    val gap = max(2.dp, size / 40)
    val shape = RoundedCornerShape(size * 0.12f)
    Column(
        modifier.size(size).clip(shape).background(paper).border(1.dp, mk.edge, shape).padding(size * 0.1f)
            .semantics { contentDescription = "Connection pattern" },
        verticalArrangement = Arrangement.spacedBy(gap),
    ) {
        for (r in 0 until 5) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(gap)) {
                for (c in 0 until 5) {
                    val on = pattern.cells.getOrElse(r * 5 + c) { false }
                    Box(Modifier.weight(1f).aspectRatio(1f).clip(RoundedCornerShape(gap)).background(if (on) ink else Color.Transparent))
                }
            }
        }
    }
}

private fun colour(hex: String): Color = runCatching { Color(android.graphics.Color.parseColor(hex)) }.getOrDefault(Color.Black)

/**
 * C1, Connect to Fob. With Fob on this device ([fobHere]) MainActivity opens it at once with the request and its
 * answer comes back by itself; this page stays for Fob on another phone, under "Open Fob" to open it again. The
 * two-device way, as the board: the request's code and pattern on a white card, the numbered steps, and Scan Fob's
 * code, which turns the card into the camera.
 */
@Composable
fun FobRequestScreen(
    view: FobRequestStep, heading: String, lead: String, stepLabel: String?, fobHere: Boolean,
    onOpenFob: () -> Unit, onAnswer: (String) -> Unit, onBack: () -> Unit, scanningAtFirst: Boolean = false,
) {
    var scanning by remember { mutableStateOf(scanningAtFirst) }
    Page(
        top = { BackBar(onBack) },
        bottom = {
            if (fobHere) {
                PrimaryButton(OPEN_FOB, onOpenFob, Modifier.fillMaxWidth())
                if (!scanning) OutlineButton(view.scan.label, { scanning = true }, Modifier.fillMaxWidth(), enabled = view.scan.enabled)
            } else if (!scanning) {
                PrimaryButton(view.scan.label, { scanning = true }, Modifier.fillMaxWidth(), enabled = view.scan.enabled, icon = MkIcons.Code)
            }
            if (stepLabel != null) Muted(stepLabel)
        },
    ) {
        Head(heading, lead)
        Box(Modifier.padding(start = 16.dp, end = 16.dp, top = 18.dp).fillMaxWidth(), contentAlignment = Alignment.Center) {
            if (scanning) {
                CameraBox(view.looking, onAnswer, Modifier.fillMaxWidth().height(340.dp))
            } else {
                Column(
                    Modifier.widthIn(max = 320.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Fixed.White).padding(20.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    Box(Modifier.fillMaxWidth().aspectRatio(1f)) {
                        Image(
                            remember(view.qr) { codeBitmap(view.qr) }, contentDescription = "Code for Fob",
                            modifier = Modifier.fillMaxSize(), filterQuality = androidx.compose.ui.graphics.FilterQuality.None,
                        )
                    }
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        FobPatternImage(view.pattern, 64.dp)
                        Text(view.patternCaption, fontSize = 15.sp, lineHeight = 21.sp, color = Fixed.Muted, modifier = Modifier.widthIn(max = 180.dp))
                    }
                }
            }
        }
        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 22.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            view.steps.forEachIndexed { i, step ->
                Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Box(Modifier.size(28.dp).clip(CircleShape).background(mk.ink), contentAlignment = Alignment.Center) {
                        Text("${i + 1}", color = mk.ground, fontSize = 15.sp, fontWeight = FontWeight.Bold)
                    }
                    Text(rich(step), fontSize = 17.sp, lineHeight = 24.sp, modifier = Modifier.padding(top = 2.dp))
                }
            }
        }
        Notice(view.problem, Modifier.padding(16.dp))
    }
}

/** C1's way to open Fob on this device: Fob's own word for it, Fob having no iOS app yet for the engine to name. */
private const val OPEN_FOB = "Open Fob"

/** The camera, asked for first, reading QR codes into [onText], with [looking] along its foot. */
@Composable
private fun CameraBox(looking: String, onText: (String) -> Unit, modifier: Modifier, ask: String? = null, allow: String? = null) {
    val context = LocalContext.current
    var allowed by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val asking = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed = it }
    LaunchedEffect(Unit) { if (!allowed) asking.launch(Manifest.permission.CAMERA) }
    Box(modifier.clip(RoundedCornerShape(16.dp)).background(Fixed.Ink), contentAlignment = Alignment.Center) {
        if (allowed) {
            QrScanner(onText = onText, modifier = Modifier.fillMaxSize())
            Viewfinder(Modifier.size(220.dp).offset(y = (-14).dp))
            Text(
                looking, color = Color(0xFFECE8DF), fontSize = 15.sp, textAlign = TextAlign.Center,
                modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp),
            )
        } else {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.padding(24.dp)) {
                Text(ask ?: looking, color = Color.White, fontSize = 15.sp, textAlign = TextAlign.Center)
                PrimaryButton(allow ?: ALLOW_CAMERA, { asking.launch(Manifest.permission.CAMERA) }, height = 48.dp, size = 15)
            }
        }
    }
}

private const val ALLOW_CAMERA = "Allow the camera"

/**
 * A value with − and + (B4, C4, D4). With a [SetupStepper.detail] the value sits between the buttons (C4, D4's
 * "10 confirmations", "1"); without, as B4's phone row, the value is the line under the label.
 */
@Composable
internal fun StepperRow(s: SetupStepper, onStep: (up: Boolean) -> Unit) = Row(
    Modifier.fillMaxWidth().heightIn(min = 60.dp).padding(start = 16.dp, end = 10.dp, top = 8.dp, bottom = 8.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(10.dp),
) {
    val inside = s.detail != null
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(s.label, fontSize = 17.sp)
        Text(if (inside) s.detail!! else s.value, fontSize = 15.sp, color = mk.muted)
    }
    Row(Modifier.clip(RoundedCornerShape(10.dp)).background(mk.chip), verticalAlignment = Alignment.CenterVertically) {
        StepButton(s.minus, "${s.label}: less") { onStep(false) }
        if (inside) {
            Text(
                s.value, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, style = tabular, textAlign = TextAlign.Center,
                maxLines = 1, softWrap = false, modifier = Modifier.widthIn(min = 28.dp).clearAndSetSemantics {},
            )
        }
        StepButton(s.plus, "${s.label}: more") { onStep(true) }
    }
}

@Composable
private fun StepButton(press: Press, label: String, onClick: () -> Unit) = Box(
    Modifier.size(42.dp, 44.dp).clickable(enabled = press.enabled, role = Role.Button, onClick = onClick).semantics { contentDescription = label },
    contentAlignment = Alignment.Center,
) { Text(press.label, fontSize = 22.sp, color = if (press.enabled) mk.ink else mk.outline) }

/** What B4, C4 and D4 can change. */
class StepperActions(val onPaidAfter: (Boolean) -> Unit, val onExpiry: (Boolean) -> Unit)

/** B4 (phone) and C4 (tablet), Connected to Fob: what Fob handed over, then Till's own payment settings. */
@Composable
fun FobConnectedScreen(
    view: FobConnectedStep, heading: String, lead: String, stepLabel: String?,
    steps: StepperActions, onNext: () -> Unit, onOtherWallet: () -> Unit,
) = Page(
    bottom = { Foot(view.next, onNext, stepLabel, quiet = view.otherWallet, onQuiet = onOtherWallet) },
) {
    Head(heading, lead, eyebrow = view.eyebrow, done = true)
    Spacer(Modifier.height(12.dp))
    Group(rows = view.facts.map { fact -> { FactRow(fact.label, fact.value, mono = fact.mono) } })
    SectionLabel(view.section)
    Group(rows = listOf({ StepperRow(view.paidAfter, steps.onPaidAfter) }, { StepperRow(view.expiry, steps.onExpiry) }))
    Footnote(view.footnote)
    Notice(view.problem, Modifier.padding(16.dp))
    Spacer(Modifier.height(16.dp))
}

/** D1, Scan a wallet code: the camera, Paste a code instead, and what codes it takes. */
@Composable
fun WalletCodeScreen(view: WalletCodeStep, heading: String, lead: String, stepLabel: String?, onScan: (String) -> Unit, onPaste: (String?) -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    Page(top = { BackBar(onBack) }, bottom = { if (stepLabel != null) Muted(stepLabel) }) {
        Head(heading, lead)
        CameraBox(view.looking, onScan, Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp).fillMaxWidth().height(340.dp), view.cameraAsk, view.allowCamera)
        Box(Modifier.padding(start = 8.dp, top = 4.dp)) {
            LinkButton(view.paste.label, {
                val clip = context.getSystemService(android.content.ClipboardManager::class.java).primaryClip
                // Nothing to paste, and text that isn't a wallet code, are the engine's to say.
                onPaste(clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()?.trim()?.ifEmpty { null })
            }, enabled = view.paste.enabled)
        }
        Muted(view.footnote, Modifier.padding(horizontal = 20.dp))
        Notice(view.problem, Modifier.padding(16.dp))
    }
}

/** Two or three choices side by side, the chosen one on a white tab: D2's network, D3's length. */
@Composable
private fun Segmented(choices: List<Choice>, onPick: (Int) -> Unit, modifier: Modifier = Modifier) = Row(
    modifier.clip(RoundedCornerShape(10.dp)).background(mk.chip).padding(3.dp).selectableGroup(),
) {
    choices.forEachIndexed { i, c ->
        Box(
            Modifier.weight(1f).heightIn(min = 40.dp).clip(RoundedCornerShape(8.dp)).background(if (c.selected) mk.card else Color.Transparent)
                .selectable(selected = c.selected, enabled = c.enabled, role = Role.RadioButton) { onPick(i) }
                .padding(horizontal = 8.dp),
            contentAlignment = Alignment.Center,
        ) { Text(c.label, fontSize = 15.sp, fontWeight = if (c.selected) FontWeight.SemiBold else FontWeight.Normal, textAlign = TextAlign.Center) }
    }
}

/** A field's note: a green tick and what's right, or what's wrong in red. */
@Composable
private fun FieldNote(field: KeyField) {
    val note = field.note ?: return
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (field.good) Icon(MkIcons.Tick, null, tint = mk.paid, modifier = Modifier.size(14.dp))
        Text(note, fontSize = 13.sp, lineHeight = 18.sp, color = if (field.good) mk.paid else mk.danger)
    }
}

/**
 * A secret typed in: a platform field, so it's a password field the keyboard neither suggests for nor learns from
 * (`IME_FLAG_NO_PERSONALIZED_LEARNING`, which Compose's fields can't set), kept out of autofill, and its text
 * cleared in place once it's been sent ([SecretFields.clear]).
 */
@Composable
private fun SecretField(holder: SecretFields, index: Int, description: String, onChange: () -> Unit, modifier: Modifier, size: Float = 17f) {
    val ink = mk.ink.toArgb()
    AndroidView(
        modifier = modifier,
        factory = { context ->
            EditText(context).apply {
                inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
                imeOptions = EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING or EditorInfo.IME_ACTION_NEXT
                importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS
                isSingleLine = true
                background = null
                setPadding(0, 0, 0, 0)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
                setTextColor(ink)
                contentDescription = description
                addTextChangedListener(object : android.text.TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit
                    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = Unit
                    override fun afterTextChanged(s: android.text.Editable?) = onChange()
                })
                holder.fields[index] = this
            }
        },
        update = { it.setTextColor(ink) },
    )
}

/** The secret fields on a screen, read when sent and cleared in place after. */
private class SecretFields(count: Int) {
    val fields = arrayOfNulls<EditText>(count)

    /** What's typed, each field trimmed, space separated. */
    fun joined(): String = fields.mapNotNull { it?.text?.toString()?.trim()?.takeIf(String::isNotEmpty) }.joinToString(" ")

    fun text(index: Int): String = fields[index]?.text?.toString().orEmpty()

    fun clear() = fields.forEach { it?.text?.clear() }
}

/** D2, the address and view key: the network, both fields judged as they're typed, and where to find them. */
@Composable
fun WalletKeysScreen(
    view: WalletKeysStep, heading: String, lead: String, stepLabel: String?,
    onEdit: (network: WalletNetwork, address: String, viewKey: String) -> Unit, onNext: () -> Unit, onBack: () -> Unit,
) {
    val secret = remember { SecretFields(1) }
    var address by remember { mutableStateOf(view.address.value) }
    var help by remember { mutableStateOf(false) }
    val network = if (view.networks.getOrNull(1)?.selected == true) WalletNetwork.STAGENET else WalletNetwork.MAINNET
    fun edited(net: WalletNetwork = network) = onEdit(net, address, secret.text(0))
    Page(top = { BackBar(onBack) }, bottom = { Foot(view.next, onNext, stepLabel) }) {
        Head(heading, lead)
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(view.networkLabel, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.semantics { headingRole() })
                Segmented(view.networks, { i -> edited(if (i == 1) WalletNetwork.STAGENET else WalletNetwork.MAINNET) }, Modifier.widthIn(max = 320.dp).fillMaxWidth())
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(view.address.label, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                OutlinedTextField(
                    value = address, onValueChange = { address = it; edited() },
                    modifier = Modifier.fillMaxWidth().semantics { contentDescription = view.address.label },
                    minLines = 2, maxLines = 4,
                    textStyle = androidx.compose.ui.text.TextStyle(fontFamily = FontFamily.Monospace, fontSize = 15.sp, lineHeight = 22.sp),
                    keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Ascii),
                    shape = RoundedCornerShape(10.dp),
                    isError = view.address.note != null && !view.address.good,
                    colors = fieldColours(),
                )
                FieldNote(view.address)
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(view.viewKey.label, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                val wrong = view.viewKey.note != null && !view.viewKey.good
                Box(
                    Modifier.fillMaxWidth().height(52.dp).clip(RoundedCornerShape(10.dp)).background(mk.card)
                        .border(if (wrong) 2.dp else 1.dp, if (wrong) mk.danger else mk.edge, RoundedCornerShape(10.dp)).padding(horizontal = 14.dp),
                    contentAlignment = Alignment.CenterStart,
                ) { SecretField(secret, 0, view.viewKey.label, { edited() }, Modifier.fillMaxWidth()) }
                FieldNote(view.viewKey)
            }
            Column {
                LinkButton(view.helpTitle, { help = !help }, Modifier.offset(x = (-12).dp))
                if (help) Text(view.help, fontSize = 15.sp, lineHeight = 22.sp, color = mk.muted)
            }
        }
        Spacer(Modifier.height(16.dp))
    }
}

@Composable
private fun fieldColours() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = mk.ink, unfocusedBorderColor = mk.edge, errorBorderColor = mk.danger,
    focusedContainerColor = mk.card, unfocusedContainerColor = mk.card, errorContainerColor = mk.card, cursorColor = mk.ink,
)

/**
 * D3, a seed phrase: the warning with its way to a code instead, the length, a secret field per word, and Continue,
 * which sends the words, clears every field and keeps no copy.
 */
@Composable
fun WalletSeedScreen(
    view: WalletSeedStep, heading: String, lead: String, stepLabel: String?,
    onLength: (UByte) -> Unit, onScanInstead: () -> Unit, onEnter: (String) -> Unit, onBack: () -> Unit,
) {
    val count = view.words.toInt()
    val secret = remember(count) { SecretFields(count) }
    Page(
        top = { BackBar(onBack) },
        bottom = {
            Foot(view.next, {
                onEnter(secret.joined())
                secret.clear()
            }, stepLabel)
        },
    ) {
        Head(heading, lead)
        Row(
            Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(mk.warnGround)
                .padding(horizontal = 16.dp, vertical = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(MkIcons.Warning, null, tint = mk.link, modifier = Modifier.size(22.dp))
            val link = mk.link
            Text(
                buildAnnotatedString {
                    append(view.warning)
                    append(" ")
                    withLink(LinkAnnotation.Clickable("scan", TextLinkStyles(SpanStyle(color = link, fontWeight = FontWeight.SemiBold))) { onScanInstead() }) {
                        append(view.scanInstead.label)
                    }
                    append(".")
                },
                fontSize = 15.sp, lineHeight = 21.sp,
            )
        }
        Segmented(
            view.lengths, { i -> onLength(if (i == 0) 25u else 16u) },
            Modifier.padding(start = 16.dp, end = 16.dp, top = 18.dp).widthIn(max = 360.dp).fillMaxWidth(),
        )
        BoxWithConstraints(Modifier.padding(start = 16.dp, end = 16.dp, top = 14.dp).fillMaxWidth()) {
            val columns = if (maxWidth >= 520.dp) 5 else 3
            key(count) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    (0 until count).chunked(columns).forEach { row ->
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            row.forEach { i ->
                                Row(
                                    Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(8.dp)).background(mk.card)
                                        .border(1.dp, mk.edge, RoundedCornerShape(8.dp)).padding(horizontal = 8.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    Text("${i + 1}", fontSize = 12.sp, color = mk.muted, style = tabular, textAlign = TextAlign.End, modifier = Modifier.width(20.dp))
                                    SecretField(secret, i, "Word ${i + 1}", {}, Modifier.weight(1f), size = 15f)
                                }
                            }
                            repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
                        }
                    }
                }
            }
        }
        Muted(view.footnote, Modifier.padding(start = 20.dp, end = 20.dp, top = 12.dp))
        Notice(view.problem, Modifier.padding(16.dp))
        Spacer(Modifier.height(16.dp))
    }
}

/** What D4 can do besides B4's and C4's steppers. */
class ThisTillActions(val steps: StepperActions, val onNumber: (Boolean) -> Unit, val onNode: (String) -> Unit, val onNext: () -> Unit, val onBack: () -> Unit)

/** D4, This till: its number (its account), the node with Fob's checks and Change, Paid after and the code's time. */
@Composable
fun ThisTillScreen(view: ThisTillStep, heading: String, lead: String, stepLabel: String?, a: ThisTillActions) {
    var changing by remember { mutableStateOf(false) }
    Page(top = { BackBar(a.onBack) }, bottom = { Foot(view.next, a.onNext, stepLabel) }) {
        Head(heading, lead)
        Spacer(Modifier.height(16.dp))
        view.number?.let { n -> Group(rows = listOf({ StepperRow(n, a.onNumber) })); Spacer(Modifier.height(18.dp)) }
        view.node?.let { node -> NodeCard(node) { changing = true }; Spacer(Modifier.height(18.dp)) }
        Group(rows = listOf({ StepperRow(view.paidAfter, a.steps.onPaidAfter) }, { StepperRow(view.expiry, a.steps.onExpiry) }))
        Notice(view.problem, Modifier.padding(16.dp))
        Spacer(Modifier.height(16.dp))
    }
    val node = view.node
    if (changing && node != null) NodeDialog(node, onDone = { url -> changing = false; url?.let(a.onNode) })
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun NodeCard(node: NodeRow, onChange: () -> Unit) = Group(
    rows = listOfNotNull(
        {
            // The address on one line, cut short at its start rather than broken, with Change after it.
            Row(
                Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(start = 16.dp, end = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(node.label, fontSize = 17.sp)
                Text(
                    node.value, fontSize = 17.sp, color = mk.muted, maxLines = 1, softWrap = false, textAlign = TextAlign.End,
                    overflow = androidx.compose.ui.text.style.TextOverflow.StartEllipsis, modifier = Modifier.weight(1f),
                )
                LinkButton(node.change.label, onChange, enabled = node.change.enabled, size = 17)
            }
        },
        node.checks.takeIf { it.isNotEmpty() }?.let { checks ->
            {
                FlowRow(
                    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(18.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    checks.forEach { check ->
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            when (check.mark) {
                                CheckMark.PASSED -> Icon(MkIcons.Tick, "Passed", tint = mk.paid, modifier = Modifier.size(16.dp))
                                CheckMark.FAILED -> Icon(MkIcons.Warning, "Failed", tint = mk.danger, modifier = Modifier.size(16.dp))
                                CheckMark.CHECKING -> LoadingMark(16.dp, label = "Checking")
                            }
                            Text(check.text, fontSize = 15.sp)
                        }
                    }
                }
            }
        },
    ),
)

/** D4's Change: the node's address typed, or one of Fob's defaults. */
@Composable
internal fun NodeDialog(node: NodeRow, onDone: (String?) -> Unit) {
    var url by remember { mutableStateOf(node.url) }
    AlertDialog(
        onDismissRequest = { onDone(null) },
        title = { Text(node.label) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    url, { url = it }, singleLine = true, modifier = Modifier.fillMaxWidth(),
                    keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri), colors = fieldColours(),
                )
                node.choices.forEach { choice ->
                    Text(
                        choice, fontSize = 15.sp, color = mk.link,
                        modifier = Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable { onDone(choice) }.padding(vertical = 10.dp),
                    )
                }
            }
        },
        confirmButton = { LinkButton(node.change.label, { onDone(url.trim().ifEmpty { null }) }) },
        dismissButton = { LinkButton("Cancel", { onDone(null) }, color = mk.ink) },
        containerColor = mk.ground,
    )
}
