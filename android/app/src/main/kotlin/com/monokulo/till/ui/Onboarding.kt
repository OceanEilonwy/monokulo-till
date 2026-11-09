package com.monokulo.till.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.monokulo.till.overlay.BubbleFace
import uniffi.till_screens.Account
import uniffi.till_screens.CounterStep
import uniffi.till_screens.ShowSales
import uniffi.till_screens.ShowSalesStep
import uniffi.till_screens.Fact
import uniffi.till_screens.OnboardingPage
import uniffi.till_screens.PosCard
import uniffi.till_screens.PosChoice
import uniffi.till_screens.PosMark
import uniffi.till_screens.PosStep
import uniffi.till_screens.SampleAlert
import uniffi.till_screens.BubbleLook

/** Text with `**bold**` runs, as the design bolds the names of things to tap. */
fun rich(text: String): AnnotatedString = buildAnnotatedString {
    text.split("**").forEachIndexed { i, part ->
        if (i % 2 == 1) withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(part) } else append(part)
    }
}

/** "Step 4 of 6", as the engine numbers the step. */
@Composable
private fun StepLabel(label: String?) {
    if (label != null) Muted(label)
}

/** Step 1 (OnbWelcome). */
@Composable
fun WelcomeScreen(page: OnboardingPage.Welcome, stepLabel: String?, onStart: () -> Unit) = Page(
    bottom = {
        PrimaryButton("Get started", onStart, Modifier.fillMaxWidth())
        StepLabel(stepLabel)
    },
) {
    Column(
        Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        LoadingMark(148.dp, label = "Monokulo Till")
        Spacer(Modifier.height(26.dp))
        Heading("Monokulo Till", size = 34, align = TextAlign.Center)
        Spacer(Modifier.height(18.dp))
        Lead(
            "Take Monero at your till. Till sees each new sale and shows your customer a payment code.",
            Modifier.widthIn(max = 320.dp),
            align = TextAlign.Center,
            size = 17,
        )
    }
    Column(
        Modifier.padding(start = 24.dp, end = 24.dp, top = 40.dp, bottom = 16.dp),
        verticalArrangement = Arrangement.spacedBy(22.dp),
    ) {
        Feature(MkIcons.Bag, "Sees Monero sales", page.feature)
        Feature(MkIcons.Code, "Shows a payment code", "Turn the screen round and the customer scans with their wallet.")
        Feature(MkIcons.CheckCircle, "Confirms the money arrived", "Till tells you when the payment lands, and flags any that don't.")
    }
}

@Composable
private fun Feature(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, text: String) = Row(
    horizontalArrangement = Arrangement.spacedBy(16.dp),
    modifier = Modifier.semantics(mergeDescendants = true) {},
) {
    Icon(icon, contentDescription = null, tint = mk.link, modifier = Modifier.size(30.dp))
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(title, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        Text(text, fontSize = 15.sp, color = mk.muted, lineHeight = 20.sp)
    }
}

/** Numbered steps on a card (OnbStore). */
@Composable
private fun Steps(vararg steps: String) = Column(
    Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(mk.card).padding(vertical = 4.dp),
) {
    steps.forEachIndexed { i, step ->
        Row(Modifier.padding(horizontal = 16.dp, vertical = 9.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                Modifier.size(22.dp).clip(CircleShape).background(mk.ink),
                contentAlignment = Alignment.Center,
            ) { Text("${i + 1}", color = mk.ground, fontSize = 13.sp, fontWeight = FontWeight.Bold) }
            Text(rich(step), fontSize = 15.sp, lineHeight = 20.sp, modifier = Modifier.padding(top = 1.dp))
        }
    }
}

/** Step 2 (OnbStore): the camera looks for the store's connect code, or the link is pasted. */
@Composable
fun ScanScreen(page: OnboardingPage.Scan, stepLabel: String?, onPaste: (String?) -> Unit, onScan: (String) -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    var allowed by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed = it }
    LaunchedEffect(Unit) { if (!allowed) ask.launch(Manifest.permission.CAMERA) }
    Page(
        top = { BackBar(onBack) },
        bottom = {
            Text(
                rich("Dashboard open on this phone? Tap **Copy the connect link** there, then paste it here."),
                fontSize = 14.sp, lineHeight = 19.sp, color = mk.muted, textAlign = TextAlign.Center,
            )
            OutlineButton("Paste the link", onClick = {
                val clip = context.getSystemService(android.content.ClipboardManager::class.java).primaryClip
                val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()?.trim()
                // Nothing to paste, and text that isn't a connect link, are the engine's to say.
                onPaste(text?.ifEmpty { null })
            }, Modifier.fillMaxWidth(), icon = MkIcons.Copy)
            StepLabel(stepLabel)
        },
    ) {
        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Heading("Connect a store")
            Lead("Till connects from your Monokulo dashboard, wherever you host it.")
        }
        Spacer(Modifier.height(18.dp))
        Steps(
            "In your Monokulo dashboard, open the store's **Settings › Connections**",
            "Under **Apps**, choose **Connect an app**, then **Show the code**",
            "Point this camera at the code it shows",
        )
        Box(
            Modifier.padding(start = 16.dp, end = 16.dp, top = 18.dp).fillMaxWidth().height(300.dp)
                .clip(RoundedCornerShape(24.dp)).background(Color(0xFF2B2926)),
            contentAlignment = Alignment.Center,
        ) {
            if (allowed) {
                QrScanner(onText = onScan, modifier = Modifier.fillMaxSize())
                Viewfinder(Modifier.size(200.dp).offset(y = (-26).dp))
                Row(
                    Modifier.align(Alignment.BottomCenter).padding(12.dp).fillMaxWidth().height(40.dp)
                        .clip(CircleShape).background(Color(0x8C000000)),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    LoadingMark(22.dp, label = "Scanning", ring = Fixed.Ground)
                    Spacer(Modifier.width(8.dp))
                    Text("Looking for a connect code…", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                }
            } else {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.padding(24.dp)) {
                    Text("Allow the camera to scan the code, or paste the link below.", color = Color.White, fontSize = 15.sp, textAlign = TextAlign.Center)
                    PrimaryButton("Allow the camera", { ask.launch(Manifest.permission.CAMERA) }, height = 48.dp, size = 15)
                }
            }
        }
        Notice(page.problem, Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp))
    }
}

/** The four white corners of the camera's viewfinder. */
@Composable
private fun Viewfinder(modifier: Modifier) = Box(
    modifier.clearAndSetSemantics {}.drawBehind {
        val w = 4.dp.toPx()
        val arm = 34.dp.toPx()
        val r = 12.dp.toPx()
        val s = Stroke(w)
        val c = Color.White
        fun corner(x: Float, y: Float) = drawRoundRect(c, Offset(x, y), Size(arm * 2, arm * 2), CornerRadius(r), style = s)
        // Each corner is the corner of a rounded square, clipped to its arm.
        clipCorner(0f, 0f, arm) { corner(w / 2, w / 2) }
        clipCorner(size.width - arm, 0f, arm) { corner(size.width - arm * 2 - w / 2, w / 2) }
        clipCorner(0f, size.height - arm, arm) { corner(w / 2, size.height - arm * 2 - w / 2) }
        clipCorner(size.width - arm, size.height - arm, arm) { corner(size.width - arm * 2 - w / 2, size.height - arm * 2 - w / 2) }
    },
)

private fun DrawScope.clipCorner(x: Float, y: Float, arm: Float, block: DrawScope.() -> Unit) =
    clipRect(x, y, x + arm, y + arm, block = block)

/** The host as a chip, with a green lock when the connection is encrypted. */
@Composable
private fun HostChip(host: String, secure: Boolean) = Row(
    Modifier.clip(CircleShape).background(mk.card).padding(horizontal = 12.dp, vertical = 6.dp)
        .semantics(mergeDescendants = true) { contentDescription = if (secure) "$host, encrypted" else "$host, not encrypted" },
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(6.dp),
) {
    if (secure) Icon(MkIcons.Lock, contentDescription = null, tint = mk.paid, modifier = Modifier.size(14.dp))
    Text(host, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
}

@Composable
private fun SheetTop(title: String, host: String, secure: Boolean) = Column(
    Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 32.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
    verticalArrangement = Arrangement.spacedBy(10.dp),
) {
    Logo(64.dp)
    Heading(title, size = 26, align = TextAlign.Center, modifier = Modifier.padding(top = 6.dp))
    HostChip(host, secure)
}

private const val CAN_DO =
    "Till can see this store's orders and create payment codes. It can't spend from or change the wallet. Only connect a server you run or trust."

/** A step's facts on a card: the server, the store, its wallet, this device. */
@Composable
private fun Facts(facts: List<Fact>) = Group(rows = facts.map { fact -> { FactRow(fact.label, fact.value, mono = fact.mono) } })

/** Step 3, before (OnbConfirm): Monokulo names the store only once the code is spent, so this shows the server (decision 22). */
@Composable
fun ConfirmScreen(page: OnboardingPage.Confirm, stepLabel: String?, onConnect: () -> Unit, onCancel: () -> Unit) = Page(
    bottom = {
        PrimaryButton(page.connect.label, onConnect, Modifier.fillMaxWidth(), enabled = page.connect.enabled)
        LinkButton(page.cancel.label, onCancel, enabled = page.cancel.enabled)
        StepLabel(stepLabel)
    },
) {
    SheetTop("Connect to this store?", page.host, page.encrypted)
    SectionLabel("This device will take payments for")
    Facts(page.facts)
    Footnote(CAN_DO)
    Notice(page.problem, Modifier.padding(16.dp))
}

/**
 * Step 3, after: the store, its wallet and this device, read from the store
 * just now (never saved: decision 50), to check before going on.
 */
@Composable
fun ConnectedScreen(page: OnboardingPage.Connected, stepLabel: String?, onNext: () -> Unit, onRetry: () -> Unit, onWrong: () -> Unit) = Page(
    bottom = {
        PrimaryButton(page.next.label, if (page.retry) onRetry else onNext, Modifier.fillMaxWidth(), enabled = page.next.enabled)
        LinkButton("Wrong store: disconnect", onWrong)
        StepLabel(stepLabel)
    },
) {
    SheetTop(page.title, page.host, page.encrypted)
    Notice(page.problem, Modifier.padding(16.dp))
    SectionLabel("This device takes payments for")
    Facts(page.facts)
    Footnote(page.footnote)
}

/** What step 4 can do. */
class PosSetupActions(
    val onChoose: (PosMark) -> Unit,
    /** The step's button: the engine signs in, or goes on (Continue, Done) once connected. */
    val onNext: () -> Unit,
    val onChangeLocation: () -> Unit,
    val onLookAgain: () -> Unit,
    /** The way past: "Not now: only show other tills' sales", or "Skip the test" once connected, as the engine decides. */
    val onSkip: () -> Unit,
    val onBack: () -> Unit,
)

/**
 * Step 4 (OnbSquare, for every point of sale: acceptance T1.1, decision
 * 136), as the engine wrote it: Square or Clover as one radio group,
 * nothing chosen until a tap, then the chosen one's card: its sign-in, its
 * location or merchant, and what to do at the point of sale itself.
 */
@Composable
fun PointOfSaleScreen(step: PosStep, stepLabel: String?, a: PosSetupActions) = Page(
    top = { BackBar(a.onBack.takeIf { step.back }) },
    bottom = {
        // The test sale's status (docs/decisions/core.md 121), drawn plainly.
        step.status?.let { status ->
            Text(status.title, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Muted(status.detail)
        }
        step.next?.let { next -> PrimaryButton(next.label, a.onNext, Modifier.fillMaxWidth(), enabled = next.enabled) }
        step.skip?.let { skip -> LinkButton(skip.label, a.onSkip, enabled = skip.enabled, size = 15) }
        step.skipFootnote?.let { Muted(it, align = TextAlign.Center) }
        StepLabel(stepLabel)
    },
) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading("Connect your point of sale")
    }
    SectionLabel("Point of sale")
    PosChoices(step.choices, onChoose = a.onChoose)
    step.card?.let { PosCardView(it, a) }
    Spacer(Modifier.height(16.dp))
}

/** Square and Clover as one radio group (T1.1): a row each, at least 48 dp, led by its mark on a tile; the mark and tick decorative. */
@Composable
fun PosChoices(choices: List<PosChoice>, onChoose: (PosMark) -> Unit) = Group(
    Modifier.selectableGroup(),
    rows = choices.map { pos ->
        {
            val choice = pos.choice
            ChoiceRow(choice.label, choice.selected, { if (choice.enabled) onChoose(pos.mark) }, choice.detail) { PosTile(pos.mark) }
        }
    },
)

/** The heading and lead of a point of sale's card, under Till's mark linked to the point of sale's. */
@Composable
private fun CardTop(card: PosCard) = Column(
    Modifier.padding(start = 20.dp, end = 20.dp, top = 24.dp),
    verticalArrangement = Arrangement.spacedBy(10.dp),
) {
    LinkedMarks(card.mark)
    Heading(card.heading, size = 24)
    Lead(card.lead, size = 17)
}

/** Signed in: a tick, "Connected" and who. */
@Composable
private fun ConnectedRow(who: String) = Row(
    Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = 16.dp, vertical = 8.dp).semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    PaidTick(24.dp)
    Column {
        Text("Connected", fontSize = 17.sp)
        Muted(who)
    }
}

/** Numbered steps from [from], `**bold**` for what to tap. */
@Composable
private fun NumberedSteps(steps: List<String>, from: Int = 1) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    steps.forEachIndexed { i, step -> Text(rich("${i + from}.  $step"), fontSize = 15.sp, lineHeight = 22.sp) }
}

/**
 * The chosen point of sale's card (acceptance T2.6, T3.2, T3.3): its
 * account, its location or merchant once signed in; a titled card of
 * numbered steps at the point of sale, with Clover's Look again after the
 * first when Clover didn't let the till add the payment type; the test
 * sale; and, for Clover, how a sale never paid is refunded.
 */
@Composable
private fun PosCardView(card: PosCard, a: PosSetupActions) {
    CardTop(card)
    SectionLabel(card.accountTitle)
    when (val account = card.account) {
        is Account.Connected -> Group(
            rows = listOfNotNull(
                { ConnectedRow(account.who) },
                card.place?.let { place ->
                    {
                        val row = if (card.changeLocation) Modifier.clickable(onClickLabel = "Change the location", onClick = a.onChangeLocation) else Modifier
                        Box(row) { FactRow(place.label, place.value) }
                    }
                },
            ),
        )
        is Account.NotSignedIn -> Group(rows = listOf({ FactRow(account.row.label, account.row.value) }))
    }
    Footnote(card.permissions)
    Notice(card.problem, Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp))
    SectionLabel(card.atTheTill)
    Column(
        Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(mk.card)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(card.stepsTitle, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        val lookAgain = card.lookAgain
        if (lookAgain != null) {
            NumberedSteps(card.steps.take(1))
            OutlineButton(lookAgain.label, a.onLookAgain, Modifier.fillMaxWidth(), enabled = lookAgain.enabled)
            NumberedSteps(card.steps.drop(1), from = 2)
        } else {
            NumberedSteps(card.steps)
        }
        card.testSale?.let { test ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(mk.inset).padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (test.found) PaidTick(26.dp) else LoadingMark(26.dp, label = "Waiting for a test sale")
                Text(test.text, fontSize = 14.sp)
            }
        }
    }
    card.footnote?.let { Footnote(it) }
}

/** Step 4, when Square has more than one location. */
@Composable
fun LocationsScreen(page: OnboardingPage.Locations, stepLabel: String?, onPick: (String) -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = { StepLabel(stepLabel) },
) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading("Which location is this till at?")
        Lead(page.lead)
    }
    SectionLabel("Locations")
    val none = page.none
    if (none != null) {
        Group(rows = listOf({ FactRow(none.label, none.value) }))
    } else {
        Group(
            rows = page.locations.map { location ->
                {
                    Box(Modifier.clickable(onClickLabel = location.choose) { onPick(location.id) }) {
                        FactRow(location.name, location.currency)
                    }
                }
            },
        )
    }
}

/** A notification as it will look, in the illustrations. */
@Composable
private fun NotificationMock(alert: SampleAlert, faint: Boolean) = NotificationMock(alert.title, alert.text, alert.time, faint)

@Composable
private fun NotificationMock(title: String, text: String?, time: String, faint: Boolean) = Row(
    Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(mk.card.copy(alpha = if (faint) 0.6f else 0.92f))
        .padding(horizontal = 14.dp, vertical = 12.dp),
    horizontalArrangement = Arrangement.spacedBy(12.dp),
    verticalAlignment = if (text == null) Alignment.CenterVertically else Alignment.Top,
) {
    Box(Modifier.size(38.dp).clip(RoundedCornerShape(9.dp)).background(mk.ground), contentAlignment = Alignment.Center) { Logo(28.dp) }
    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        if (text != null) {
            Row {
                Text("Till", fontSize = 13.sp, color = mk.muted, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                Text(time, fontSize = 13.sp, color = mk.muted)
            }
        }
        Text(title, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
        if (text != null) Text(text, fontSize = 15.sp)
    }
    if (text == null) Text(time, fontSize = 13.sp, color = mk.muted)
}

/** Step 5 (OnbAlerts): notifications. */
@Composable
fun AlertsScreen(page: OnboardingPage.Alerts, stepLabel: String?, onAllow: () -> Unit, onSkip: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = {
        PrimaryButton(page.next.label, onAllow, Modifier.fillMaxWidth(), enabled = page.next.enabled)
        page.notNow?.let { LinkButton(it.label, onSkip, enabled = it.enabled) }
        StepLabel(stepLabel)
    },
) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading("Hear every new sale")
        Lead(
            "When Till is in the background, an alert tells you a customer is ready to pay, " +
                "and its notification follows the payment until it's paid.",
            size = 17,
        )
    }
    Column(
        Modifier.padding(start = 16.dp, end = 16.dp, top = 36.dp).fillMaxWidth().clip(RoundedCornerShape(28.dp)).background(mk.inset)
            .padding(horizontal = 14.dp, vertical = 26.dp).clearAndSetSemantics {},
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        NotificationMock(page.sale, faint = false)
        NotificationMock(page.paid, faint = true)
    }
}

/** Step 5, on Android (AndPermission): the bubble over other apps. */
@Composable
fun FloatScreen(page: OnboardingPage.Float, onOpenSettings: () -> Unit, onNotificationsOnly: () -> Unit, onContinue: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = {
        PrimaryButton(page.next.label, if (page.allowed) onContinue else onOpenSettings, Modifier.fillMaxWidth(), height = 56.dp, size = 16, enabled = page.next.enabled)
        page.notificationsOnly?.let { LinkButton(it.label, onNotificationsOnly, size = 16, enabled = it.enabled) }
    },
) {
    val allowed = page.allowed
    Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading(page.heading)
        Lead(page.lead)
    }
    Row(
        Modifier.padding(start = 20.dp, end = 20.dp, top = 28.dp).fillMaxWidth().clip(RoundedCornerShape(28.dp))
            .background(Color(0xFFECECEC)).padding(22.dp).clearAndSetSemantics {},
        horizontalArrangement = Arrangement.SpaceAround,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            BubbleFace(BubbleLook.IDLE, pulse = false)
            Text("No sale", fontSize = 13.sp, color = Fixed.Muted)
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            BubbleFace(BubbleLook.LIT, pulse = true)
            Text("New sale", fontSize = 13.sp, color = Fixed.Muted)
        }
    }
    if (allowed) {
        Row(
            Modifier.padding(start = 20.dp, end = 20.dp, top = 26.dp).fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(mk.card)
                .padding(horizontal = 20.dp, vertical = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            PaidTick(24.dp)
            Text("Till can float over other apps.", fontSize = 16.sp)
        }
    } else {
        Text("ON THE NEXT SCREEN, CHOOSE MONOKULAR AND TURN THIS ON", fontSize = 13.sp, color = mk.muted, modifier = Modifier.padding(start = 40.dp, end = 40.dp, top = 26.dp, bottom = 8.dp))
        Row(
            Modifier.padding(horizontal = 20.dp).fillMaxWidth().heightIn(min = 64.dp).clip(RoundedCornerShape(20.dp)).background(mk.card)
                .padding(horizontal = 20.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text("Allow display over other apps", fontSize = 16.sp, modifier = Modifier.weight(1f))
            // A picture of the system switch, not a control.
            Box(Modifier.size(52.dp, 32.dp).clip(CircleShape).background(mk.accent).clearAndSetSemantics {}) {
                Box(Modifier.align(Alignment.CenterEnd).padding(4.dp).size(24.dp).clip(CircleShape).background(Color.White))
            }
        }
    }
    Spacer(Modifier.height(16.dp))
}

/**
 * After the test sale (C4, docs/decisions/core.md 113): which sales alert on
 * this phone, drawn plainly.
 */
@Composable
fun CounterScreen(view: CounterStep, onPick: (everyDevice: Boolean) -> Unit, onNext: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = { PrimaryButton(view.next.label, onNext, Modifier.fillMaxWidth(), enabled = view.next.enabled) },
) {
    Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Muted(view.found)
        Heading(view.heading)
        Lead(view.lead)
    }
    Spacer(Modifier.height(16.dp))
    Group(rows = view.choices.mapIndexed { index, choice -> { ChoiceRow(choice.label, choice.selected, { onPick(index == 1) }, choice.detail) } })
    Footnote(view.footnote)
}

/** How a new sale shows up (O4): an iPad step, drawn plainly should it come. */
@Composable
fun ShowSalesScreen(view: ShowSalesStep, onPick: (ShowSales) -> Unit, onNext: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = { PrimaryButton(view.next.label, onNext, Modifier.fillMaxWidth(), enabled = view.next.enabled) },
) {
    Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading(view.heading)
        Lead(view.lead)
    }
    Spacer(Modifier.height(16.dp))
    Group(rows = view.choices.map { c -> { ChoiceRow(c.choice.label, c.choice.selected, { onPick(c.mode) }, c.choice.detail) } })
    view.sideBySide?.let { Footnote(it.value) }
}

/** Step 6 (OnbReady): what's set up, and start the shift. */
@Composable
fun ReadyScreen(page: OnboardingPage.Ready, bubble: Boolean, onStart: () -> Unit) = Page(
    bottom = {
        PrimaryButton("Start Till", onStart, Modifier.fillMaxWidth())
        Muted(
            "Till watches for sales during your shift, with a notification, until you stop it in Settings.",
            Modifier.widthIn(max = 320.dp),
            align = TextAlign.Center,
        )
    },
) {
    Column(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 56.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Heading(page.heading, align = TextAlign.Center)
        Lead(page.lead, Modifier.widthIn(max = 330.dp), align = TextAlign.Center, size = 17)
    }
    PhoneTop(bubble, Modifier.padding(top = 40.dp).align(Alignment.CenterHorizontally))
    Column(
        Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).border(1.dp, mk.edge, RoundedCornerShape(12.dp))
            .background(mk.card),
    ) {
        page.checks.forEachIndexed { i, check ->
            if (i > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
            Check(check.name, check.value, check.done)
        }
    }
    Spacer(Modifier.height(16.dp))
}

@Composable
private fun Check(name: String, value: String, done: Boolean) = Row(
    Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 12.dp).semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    Icon(MkIcons.CheckCircle, contentDescription = null, tint = if (done) mk.paid else mk.outline, modifier = Modifier.size(22.dp).alpha(if (done) 1f else 0.6f))
    Text(name, fontSize = 15.sp, modifier = Modifier.weight(1f))
    Text(value, fontSize = 15.sp, color = mk.muted)
}

/** The top of a phone with the point-of-sale app on it and the faded bubble (or the notification icon) over it. */
@Composable
private fun PhoneTop(bubble: Boolean, modifier: Modifier) = Box(
    modifier.size(300.dp, 210.dp).clearAndSetSemantics {}
        .clip(RoundedCornerShape(topStart = 44.dp, topEnd = 44.dp))
        .background(Color.White)
        .border(1.dp, Color(0xFFDCD5C7), RoundedCornerShape(topStart = 44.dp, topEnd = 44.dp)),
) {
    Box(Modifier.align(Alignment.TopCenter).padding(top = 12.dp).size(18.dp).clip(CircleShape).background(Color.Black))
    if (!bubble) Logo(16.dp, Modifier.padding(start = 30.dp, top = 13.dp), ring = Fixed.Ink)
    Column(Modifier.padding(start = 22.dp, end = 22.dp, top = 56.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        repeat(2) {
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                repeat(3) { Box(Modifier.weight(1f).height(54.dp).clip(RoundedCornerShape(12.dp)).background(Color(0xFFECECEC))) }
            }
        }
    }
    if (bubble) Box(Modifier.align(Alignment.TopEnd).padding(top = 64.dp)) { BubbleFace(BubbleLook.IDLE, pulse = false) }
}
