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
import com.monokulo.till.core.Amounts
import com.monokulo.till.core.ConnectCode
import com.monokulo.till.StoreNow
import com.monokulo.till.core.Connection
import com.monokulo.till.overlay.BubbleFace
import com.monokulo.till.sales.SquareLocation
import com.monokulo.till.sales.TillSale
import com.monokulo.till.BubbleLook
import com.monokulo.till.SaleText
import com.monokulo.till.PosText
import com.monokulo.till.core.PointOfSale

/** Text with `**bold**` runs, as the design bolds the names of things to tap. */
fun rich(text: String): AnnotatedString = buildAnnotatedString {
    text.split("**").forEachIndexed { i, part ->
        if (i % 2 == 1) withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(part) } else append(part)
    }
}

/** Step 1 (OnbWelcome). */
@Composable
fun WelcomeScreen(onStart: () -> Unit) = Page(
    bottom = {
        PrimaryButton("Get started", onStart, Modifier.fillMaxWidth())
        StepLabel(1)
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
        Feature(MkIcons.Bag, "Sees Monero sales", PosText.WELCOME_FEATURE)
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
fun ScanScreen(onCode: (String) -> Boolean, onBack: () -> Unit) {
    val context = LocalContext.current
    var allowed by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed = it }
    LaunchedEffect(Unit) { if (!allowed) ask.launch(Manifest.permission.CAMERA) }
    var wrong by remember { mutableStateOf<String?>(null) }
    Page(
        top = { BackBar(onBack) },
        bottom = {
            Text(
                rich("Dashboard open on this phone? Tap **Copy the connect link** there, then paste it here."),
                fontSize = 14.sp, lineHeight = 19.sp, color = mk.muted, textAlign = TextAlign.Center,
            )
            OutlineButton("Paste the link", onClick = {
                val clip = context.getSystemService(android.content.ClipboardManager::class.java).primaryClip
                val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty().trim()
                wrong = when {
                    text.isEmpty() -> "There's nothing to paste. Copy the connect link on the dashboard first."
                    !onCode(text) -> "That isn't a store's connect link."
                    else -> null
                }
            }, Modifier.fillMaxWidth(), icon = MkIcons.Copy)
            StepLabel(2)
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
                QrScanner(onText = { text -> if (!onCode(text)) wrong = "That code isn't a store's connect code." }, modifier = Modifier.fillMaxSize())
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
        Notice(wrong, Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp))
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

/** Step 3, before (OnbConfirm): Monokulo names the store only once the code is spent, so this shows the server (decision 22). */
@Composable
fun ConfirmScreen(code: ConnectCode, deviceName: String, busy: Boolean, problem: String?, onConnect: () -> Unit, onCancel: () -> Unit) = Page(
    bottom = {
        PrimaryButton(if (busy) "Connecting…" else "Connect", onConnect, Modifier.fillMaxWidth(), enabled = !busy)
        LinkButton("Cancel", onCancel, enabled = !busy)
        StepLabel(3)
    },
) {
    val secure = code.encrypted
    SheetTop("Connect to this store?", code.host, secure)
    SectionLabel("This device will take payments for")
    Group(rows = listOf({ FactRow("Server", code.host) }, { FactRow("Device name", deviceName) }))
    Footnote(CAN_DO)
    Notice(problem, Modifier.padding(16.dp))
}

/**
 * Step 3, after: the store, its wallet and this device, read from the store
 * just now (never saved: decision 50), to check before going on.
 */
@Composable
fun ConnectedScreen(connection: Connection, now: StoreNow, onNext: () -> Unit, onRetry: () -> Unit, onWrong: () -> Unit) = Page(
    bottom = {
        val store = now.store
        if (store == null && !now.reading) {
            PrimaryButton("Try again", onRetry, Modifier.fillMaxWidth())
        } else {
            PrimaryButton("Continue", onNext, Modifier.fillMaxWidth(), enabled = store != null)
        }
        LinkButton("Wrong store: disconnect", onWrong)
        StepLabel(3)
    },
) {
    val store = now.store
    SheetTop(store?.let { "Connected to ${it.name}" } ?: "Connected", connection.host, connection.encrypted)
    Notice(now.problem, Modifier.padding(16.dp))
    SectionLabel("This device takes payments for")
    val wallet = store?.wallet
    val reading = if (now.reading) "Reading…" else "—"
    Group(
        rows = listOfNotNull(
            { FactRow("Store", store?.name ?: reading) },
            wallet?.let { { FactRow("Payments go to", it.name) } },
            wallet?.let { { FactRow("Wallet address", it.addressShort, mono = true) } },
            wallet?.let { { FactRow("Network", it.network.replaceFirstChar(Char::uppercase)) } },
            { FactRow("Device name", now.device?.deviceName ?: reading) },
        ),
    )
    Footnote("Check this is the store you meant: payments go to its wallet. $CAN_DO")
}

/** Where Clover's Monero payment type stands, as step 4's Clover card shows it (decision 129). */
enum class CloverTender {
    /** Not known yet: signing in, or looking. */
    Looking,
    /** Found or added: the cashier rings sales up with it. */
    Ready,
    /** Clover didn't let the till add it: the cashier adds it in Clover, then Look again. */
    AddInClover,
}

/** Step 4's state: the point of sale chosen, and the chosen one's sign-in. */
data class PosSetup(
    /** Null until the cashier taps one (acceptance T1.1). */
    val chosen: PointOfSale?,
    /** Square: the location chosen, once signed in. */
    val location: SquareLocation?,
    /** Clover: the merchant's name, once signed in. */
    val merchant: String?,
    /** Signed in to [chosen]. */
    val signedIn: Boolean,
    val tender: CloverTender,
    val busy: Boolean,
    val problem: String?,
    val testSale: TillSale?,
    /** Onboarding's step 4; false when Settings' "Change the point of sale" opened it, with Back and no test sale. */
    val setup: Boolean = true,
) {
    /** Signed in and watching a Square location or a Clover merchant: the test sale can come in. */
    val connected: Boolean
        get() = signedIn && when (chosen) {
            PointOfSale.Square -> location != null
            PointOfSale.Clover -> merchant != null
            null -> false
        }
}

/** What step 4 can do. */
class PosSetupActions(
    val onChoose: (PointOfSale) -> Unit,
    val onSignIn: () -> Unit,
    val onChangeLocation: (() -> Unit)?,
    val onLookAgain: () -> Unit,
    val onSkip: () -> Unit,
    val onContinue: () -> Unit,
)

/**
 * Step 4 (OnbSquare, for every point of sale: acceptance T1.1, decision
 * 136): Square or Clover as one radio group, nothing chosen until a tap,
 * then the chosen one's card: its sign-in, its location or merchant, and what
 * to do at the point of sale itself.
 */
@Composable
fun PointOfSaleScreen(s: PosSetup, a: PosSetupActions) = Page(
    top = { BackBar(a.onContinue.takeUnless { s.setup }) },
    bottom = {
        val chosen = s.chosen
        when {
            s.connected && !s.setup -> PrimaryButton("Done", a.onContinue, Modifier.fillMaxWidth())
            s.connected -> {
                PrimaryButton("Continue", a.onContinue, Modifier.fillMaxWidth(), enabled = s.testSale != null)
                if (s.testSale == null) LinkButton("Skip the test", a.onContinue, size = 15)
            }
            chosen != null -> {
                PrimaryButton(if (s.busy) PosText.SIGNING_IN else PosText.signIn(chosen), a.onSignIn, Modifier.fillMaxWidth(), enabled = !s.busy)
                LinkButton("Not now: only show other tills' sales", a.onSkip, enabled = !s.busy, size = 15)
            }
            else -> LinkButton("Not now: only show other tills' sales", a.onSkip, enabled = !s.busy, size = 15)
        }
        if (s.setup) StepLabel(4)
    },
) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading(PosText.CONNECT_HEADING)
    }
    SectionLabel(PosText.POINT_OF_SALE)
    PosChoices(s.chosen, enabled = !s.busy, onChoose = a.onChoose)
    when (s.chosen) {
        PointOfSale.Square -> SquareCard(s, a)
        PointOfSale.Clover -> CloverCard(s, a)
        null -> Unit
    }
    Spacer(Modifier.height(16.dp))
}

/** Square and Clover as one radio group (T1.1): a row each, at least 48 dp, led by its mark on a tile; the mark and tick decorative. */
@Composable
fun PosChoices(chosen: PointOfSale?, enabled: Boolean = true, onChoose: (PointOfSale) -> Unit) = Group(
    Modifier.selectableGroup(),
    rows = PointOfSale.entries.map { pos ->
        { ChoiceRow(pos.displayName, chosen == pos, { if (enabled) onChoose(pos) }, PosText.choiceDetail(pos)) { PosTile(pos) } }
    },
)

/** The heading and lead of a point of sale's card, under Till's mark linked to the point of sale's. */
@Composable
private fun CardTop(pos: PointOfSale) = Column(
    Modifier.padding(start = 20.dp, end = 20.dp, top = 24.dp),
    verticalArrangement = Arrangement.spacedBy(10.dp),
) {
    LinkedMarks(pos)
    Heading(PosText.connect(pos), size = 24)
    Lead(PosText.lead(pos), size = 17)
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

/**
 * The card the steps at the point of sale go on, with the test sale's row under
 * them once connected, but not while Clover still needs the Monero payment type
 * added by hand: no sale can be paid in Monero until it is there.
 */
@Composable
private fun AtTheTill(pos: PointOfSale, s: PosSetup, steps: @Composable () -> Unit) {
    SectionLabel(PosText.atTheTill(pos))
    Column(
        Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(mk.card)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        steps()
        if (s.connected && s.setup && s.tender != CloverTender.AddInClover) {
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(mk.inset).padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                val testSale = s.testSale
                if (testSale == null) {
                    LoadingMark(26.dp, label = "Waiting for a test sale")
                    Text(PosText.testSaleWaiting(pos), fontSize = 14.sp)
                } else {
                    PaidTick(26.dp)
                    Text(PosText.testSaleFound(pos, SaleText.amount(testSale)), fontSize = 14.sp)
                }
            }
        }
    }
}

/** Numbered steps from [from], `**bold**` for what to tap. */
@Composable
private fun NumberedSteps(steps: List<String>, from: Int = 1) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    steps.forEachIndexed { i, step -> Text(rich("${i + from}.  $step"), fontSize = 15.sp, lineHeight = 22.sp) }
}

/** Square's card: as it was before Clover joined, the payment type added by the cashier in Square. */
@Composable
private fun SquareCard(s: PosSetup, a: PosSetupActions) {
    val pos = PointOfSale.Square
    CardTop(pos)
    SectionLabel(PosText.account(pos))
    val location = s.location
    if (s.connected && location != null) {
        Group(
            rows = listOf(
                { ConnectedRow(location.businessName ?: location.name) },
                {
                    val change = a.onChangeLocation
                    val row = Modifier.let { if (change != null) it.clickable(onClickLabel = "Change the location", onClick = change) else it }
                    Box(row) { FactRow(PosText.placeRow(pos), location.name) }
                },
            ),
        )
    } else {
        Group(rows = listOf({ FactRow("Account", "Not signed in") }))
    }
    Footnote(PosText.permissions(pos))
    Notice(s.problem, Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp))
    AtTheTill(pos, s) {
        Text("Add \"Monero\" as a payment type", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        NumberedSteps(
            listOf(
                "Open **Settings › Checkout › Payment types**",
                "Under Other, add a type called **Monero**",
                "Ring up a sale and tap Monero to pay",
            ),
        )
    }
}

/**
 * Clover's card (acceptance T2.6, T3.2, T3.3), laid out as Square's: the
 * merchant once signed in; a titled card of numbered steps whose first is the
 * Monero payment type the till added, or adding it in Clover with Look again
 * when Clover didn't let it; the test sale; how a sale never paid is refunded.
 */
@Composable
private fun CloverCard(s: PosSetup, a: PosSetupActions) {
    val pos = PointOfSale.Clover
    CardTop(pos)
    SectionLabel(PosText.account(pos))
    val merchant = s.merchant
    if (s.connected && merchant != null) {
        Group(rows = listOf({ ConnectedRow(merchant) }))
    } else {
        Group(rows = listOf({ FactRow("Account", "Not signed in") }))
    }
    Footnote(PosText.permissions(pos))
    Notice(s.problem, Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp))
    AtTheTill(pos, s) {
        Text(PosText.CLOVER_STEPS_TITLE, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        if (s.connected && s.tender == CloverTender.AddInClover) {
            NumberedSteps(listOf(PosText.CLOVER_ADD_TENDER))
            OutlineButton(PosText.LOOK_AGAIN, a.onLookAgain, Modifier.fillMaxWidth(), enabled = !s.busy)
            NumberedSteps(PosText.CLOVER_STEPS, from = 2)
        } else {
            NumberedSteps(listOf(PosText.cloverTenderStep(added = s.connected && s.tender == CloverTender.Ready)) + PosText.CLOVER_STEPS)
        }
    }
    Footnote(PosText.CLOVER_REFUND_NOTE)
}

/** Step 4, when Square has more than one location. */
@Composable
fun LocationsScreen(locations: List<SquareLocation>, onPick: (SquareLocation) -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = { StepLabel(4) },
) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading("Which location is this till at?")
        Lead(PosText.LOCATIONS_LEAD)
    }
    SectionLabel("Locations")
    if (locations.isEmpty()) {
        Group(rows = listOf({ FactRow("None", PosText.NO_LOCATIONS) }))
    } else {
        Group(
            rows = locations.map { location ->
                {
                    Box(Modifier.clickable(onClickLabel = "Choose ${location.name}") { onPick(location) }) {
                        FactRow(location.name, location.currency)
                    }
                }
            },
        )
    }
}

/** A notification as it will look, in the illustrations. */
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
fun AlertsScreen(place: String?, currency: String, allowed: Boolean, onAllow: () -> Unit, onSkip: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = {
        PrimaryButton(if (allowed) "Continue" else "Turn on alerts", onAllow, Modifier.fillMaxWidth())
        if (!allowed) LinkButton("Not now", onSkip)
        StepLabel(5)
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
        NotificationMock(
            "New Monero sale · ${Amounts.fiat("24.50", currency)}",
            listOfNotNull(place, "Tap Show code for the customer.").joinToString(". "),
            "now",
            faint = false,
        )
        NotificationMock("Paid · ${Amounts.fiat("8.00", currency)}", null, "4m ago", faint = true)
    }
}

/** Step 5, on Android (AndPermission): the bubble over other apps. */
@Composable
fun FloatScreen(pos: PointOfSale?, allowed: Boolean, onOpenSettings: () -> Unit, onNotificationsOnly: () -> Unit, onContinue: () -> Unit, onBack: () -> Unit) = Page(
    top = { BackBar(onBack) },
    bottom = {
        if (allowed) {
            PrimaryButton("Continue", onContinue, Modifier.fillMaxWidth(), height = 56.dp, size = 16)
        } else {
            PrimaryButton("Open settings", onOpenSettings, Modifier.fillMaxWidth(), height = 56.dp, size = 16)
            LinkButton("Use notifications only", onNotificationsOnly, size = 16)
        }
    },
) {
    Column(Modifier.padding(start = 24.dp, end = 24.dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Heading(PosText.floatHeading(pos))
        Lead(PosText.floatLead(pos))
    }
    Row(
        Modifier.padding(start = 20.dp, end = 20.dp, top = 28.dp).fillMaxWidth().clip(RoundedCornerShape(28.dp))
            .background(Color(0xFFECECEC)).padding(22.dp).clearAndSetSemantics {},
        horizontalArrangement = Arrangement.SpaceAround,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            BubbleFace(BubbleLook.Idle, pulse = false)
            Text("No sale", fontSize = 13.sp, color = Fixed.Muted)
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            BubbleFace(BubbleLook.Lit, pulse = true)
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

/** Step 6 (OnbReady): what's set up, and start the shift. */
@Composable
fun ReadyScreen(
    pos: PointOfSale?,
    store: String,
    location: String?,
    alerts: Boolean,
    bubble: Boolean,
    onStart: () -> Unit,
) = Page(
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
        Heading("You're ready", align = TextAlign.Center)
        Lead(
            PosText.readyLead(pos, bubble),
            Modifier.widthIn(max = 330.dp),
            align = TextAlign.Center,
            size = 17,
        )
    }
    PhoneTop(bubble, Modifier.padding(top = 40.dp).align(Alignment.CenterHorizontally))
    Column(
        Modifier.padding(horizontal = 16.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).border(1.dp, mk.edge, RoundedCornerShape(12.dp))
            .background(mk.card),
    ) {
        Check("Store", store, true)
        Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
        Check(PosText.checkName(pos), location ?: "Not connected", location != null)
        Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
        Check("Alerts", if (alerts) "On" else "Off", alerts)
        Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
        Check("Bubble", if (bubble) "On" else "Notifications only", bubble)
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
    if (bubble) Box(Modifier.align(Alignment.TopEnd).padding(top = 64.dp)) { BubbleFace(BubbleLook.Idle, pulse = false) }
}
