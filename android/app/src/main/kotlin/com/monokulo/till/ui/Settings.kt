package com.monokulo.till.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.monokulo.till.PosText
import com.monokulo.till.StoreNow
import com.monokulo.till.WakeUpsText
import com.monokulo.till.core.Connection
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.Tor
import com.monokulo.till.push.BellAddress
import com.monokulo.till.push.WakeUpChoice
import com.monokulo.till.push.WakeUpStatus
import com.monokulo.till.sales.SquareLocation

/** Settings' state. */
data class SettingsState(
    val connection: Connection?,
    /** The store as last read: shown while Settings reads it again. */
    val store: StoreNow,
    val pos: PosState,
    val live: Boolean,
    val shiftOn: Boolean,
    val alerts: Boolean,
    val bubbleWanted: Boolean,
    val bubbleAllowed: Boolean,
    val debug: Boolean,
    /** Debug builds: the stand-ins' addresses, by point of sale (acceptance T6.5). */
    val baseUrls: Map<PointOfSale, String?>,
    /** Orbot's proxy answers; null while that's being checked. */
    val torRunning: Boolean? = null,
    val wakeUps: WakeUpsState,
    /** What went wrong on the way back here, such as the point of sale's words on a failed sign-in through a Bell (decision 105). */
    val problem: String? = null,
)

/** Settings › Point of sale's state (acceptance T1.2). */
data class PosState(
    /** Null while none is chosen (or the till runs without one). */
    val chosen: PointOfSale?,
    val signedIn: Boolean,
    /** Square's location, while signed in to Square. */
    val location: SquareLocation?,
    /** Clover's merchant name, while signed in to Clover. */
    val merchant: String?,
    val tenderName: String,
    /** Clover's Monero payment type, while signed in to Clover. */
    val tender: CloverTender = CloverTender.Ready,
) {
    /** Signed in and at a location or merchant: the facts show. */
    val connected: Boolean
        get() = signedIn && when (chosen) {
            PointOfSale.Square -> location != null
            PointOfSale.Clover -> merchant != null
            null -> false
        }
}

/** Settings › Wake-ups' state (acceptance A1). */
data class WakeUpsState(
    val choice: WakeUpChoice,
    val status: WakeUpStatus,
    /** The project's Bell: the Monokulo Bell row's detail is its address. */
    val monokulo: BellAddress,
    /** The last "My own Bell" address entered, kept through Timer only. */
    val ownBell: BellAddress?,
    /** The installed UnifiedPush apps' names, by package, for [WakeUpStatus.ChooseDistributor]'s rows. */
    val distributorNames: Map<String, String> = emptyMap(),
    /** A debug build: an http Bell is taken (decision 98). */
    val allowHttp: Boolean = false,
)

/** What settings can do. */
class SettingsActions(
    val onShift: (Boolean) -> Unit,
    val onAlerts: () -> Unit,
    val onBubble: (Boolean) -> Unit,
    /** One of the choice rows, while not signed in: saved at once. */
    val onChoosePos: (PointOfSale) -> Unit,
    /** Sign in to the point of sale chosen. */
    val onSignIn: () -> Unit,
    val onChangeLocation: () -> Unit,
    val onSignOut: () -> Unit,
    /** "Change the point of sale": step 4's choice (acceptance T1.4). */
    val onChangePos: () -> Unit,
    /** Clover's "Look again" for the Monero payment type. */
    val onLookAgain: () -> Unit,
    val onDisconnect: () -> Unit,
    val onBaseUrl: (PointOfSale, String) -> Unit,
    /** Copies the store wallet's whole address. */
    val onCopyAddress: (String) -> Unit,
    /** Reach the store over Tor, or not. */
    val onTor: (Boolean) -> Unit,
    /** Settings › Wake-ups' choice: Timer only, the Monokulo Bell, or a saved own Bell. */
    val onWakeUps: (WakeUpChoice) -> Unit,
    /** "Use this Bell": the address as typed. */
    val onOwnBell: (String) -> Unit,
    /** A UnifiedPush app picked, by package, from several. */
    val onDistributor: (String) -> Unit,
    /** "Get ntfy": a UnifiedPush app to install. */
    val onGetDistributor: () -> Unit,
    /** Sign in to the point of sale through the Bell's application, by its application id. */
    val onBellSignIn: (PointOfSale, String) -> Unit,
    val onBack: () -> Unit,
)

/** Settings (PhoneSettings), with Android's shift and bubble in place of the Dynamic Island. */
@Composable
fun SettingsScreen(s: SettingsState, a: SettingsActions) = Page(
    top = {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            RoundIconButton(MkIcons.Back, "Back", a.onBack)
        }
    },
) {
    Heading("Settings", Modifier.padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp), size = 34)

    val connection = s.connection
    SectionLabel("Store")
    if (connection != null) {
        val store = s.store.store
        val wallet = store?.wallet
        val unknown = if (s.store.reading) "Reading…" else "—"
        Notice(s.store.problem, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
        Group(
            rows = listOfNotNull(
                {
                    Row(
                        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp).semantics(mergeDescendants = true) {},
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Logo(40.dp)
                        Column(Modifier.weight(1f)) {
                            Text(store?.name ?: unknown, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                if (connection.encrypted) {
                                    Icon(MkIcons.Lock, contentDescription = "Encrypted", tint = mk.paid, modifier = Modifier.size(11.dp))
                                }
                                Muted(connection.host)
                            }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Box(Modifier.size(8.dp).clip(CircleShape).background(if (s.live) mk.paid else mk.outline))
                            Text(if (s.live) "Online" else "Offline", fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        }
                    }
                },
                wallet?.let { { FactRow("Payments go to", it.name) } },
                wallet?.let {
                    {
                        // The short form, with Copy for the whole address.
                        FactRow(
                            "Wallet address", it.addressShort, mono = true,
                            trailing = {
                                RoundIconButton(
                                    MkIcons.Copy, "Copy wallet address", { a.onCopyAddress(it.address) },
                                    Modifier.intoRowPadding(), iconSize = 20.dp,
                                )
                            },
                        )
                    }
                },
                wallet?.let { { FactRow("Network", it.network.replaceFirstChar(Char::uppercase)) } },
                { FactRow("Base currency", store?.baseCurrency ?: unknown) },
                { FactRow("This device", s.store.device?.deviceName ?: unknown) },
            ),
        )
        Footnote("To change the wallet, open the store's settings in your Monokulo dashboard. Till can't change it.")
        TorSection(connection, s.store.onionApi, s.torRunning, a.onTor)
    }

    PosSection(s.pos, a)

    SectionLabel("Shift")
    Group(rows = listOf({ SwitchRow("Watch for sales", s.shiftOn, a.onShift) }))
    Footnote(WakeUpsText.shift(s.wakeUps.status, s.pos.chosen))

    SectionLabel("Alerts")
    Group(rows = listOf({ SwitchRow("New sale alerts", s.alerts, { a.onAlerts() }) }))
    Footnote(
        if (s.alerts) {
            "A new sale makes a sound and shows a banner when you're in another app. Change the sound in Android's settings."
        } else {
            "Alerts are off. Till still shows new sales in its bubble and here, but won't make a sound or show a banner."
        },
    )

    SectionLabel("Bubble")
    Group(rows = listOf({ SwitchRow("Float over other apps", s.bubbleWanted && s.bubbleAllowed, a.onBubble) }))
    Footnote(
        when {
            !s.bubbleAllowed -> "Android hasn't allowed Till to display over other apps, so sales show in the notification only."
            s.bubbleWanted -> PosText.bubbleFootnote(s.pos.chosen)
            else -> "Sales show in the notification only."
        },
    )

    WakeUpsSection(s.wakeUps, s.pos.chosen, s.problem, a)

    if (s.debug) {
        SectionLabel("Developer (debug builds)")
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (pos in PointOfSale.entries) {
                var base by remember(pos) { mutableStateOf(s.baseUrls[pos] ?: "") }
                OutlinedTextField(base, { base = it }, label = { Text(PosText.standIn(pos)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlineButton("Use it", { a.onBaseUrl(pos, base) }, Modifier.fillMaxWidth())
            }
        }
    }

    Spacer(Modifier.height(22.dp))
    Group(rows = listOf({ ActionRow("Disconnect this device", a.onDisconnect, mk.danger) }))
    Footnote("Removes Till from the store's Apps: its key stops working at once. Payments already made aren't affected.")
    Spacer(Modifier.height(32.dp))
}

/**
 * Settings › Point of sale (acceptance T1.2): signed in, which one and its
 * facts, Sign out and Change the point of sale; not signed in, the choice
 * rows and the chosen one's sign-in.
 */
@Composable
private fun PosSection(p: PosState, a: SettingsActions) {
    SectionLabel(PosText.POINT_OF_SALE)
    val chosen = p.chosen
    if (p.connected && chosen != null) {
        Group(
            rows = buildList<@Composable () -> Unit> {
                add { PosRow(chosen) }
                when (chosen) {
                    PointOfSale.Square -> {
                        val location = p.location ?: return@buildList
                        add { FactRow("Account", location.businessName ?: "Signed in") }
                        add {
                            Box(Modifier.clickable(onClickLabel = "Change the location", onClick = a.onChangeLocation)) {
                                FactRow(PosText.placeRow(chosen), location.name)
                            }
                        }
                        add { FactRow("Payment type", p.tenderName) }
                    }
                    PointOfSale.Clover -> {
                        add { FactRow(PosText.placeRow(chosen), p.merchant.orEmpty()) }
                        if (p.tender == CloverTender.AddInClover) {
                            add { FactRow("Payment type", PosText.tenderMissing(chosen), valueColor = mk.warn) }
                            add { ActionRow(PosText.LOOK_AGAIN, a.onLookAgain, mk.link) }
                        } else {
                            add { FactRow("Payment type", p.tenderName) }
                        }
                    }
                }
                add { ActionRow(PosText.signOut(chosen), a.onSignOut, mk.link) }
                add { ActionRow(PosText.CHANGE, a.onChangePos, mk.link) }
            },
        )
        if (chosen == PointOfSale.Clover && p.tender == CloverTender.AddInClover) {
            Footnote(PosText.CLOVER_ADD_TENDER.replace("**", ""))
        }
        Footnote(PosText.signedInFootnote(chosen))
    } else {
        PosChoices(chosen, onChoose = a.onChoosePos)
        if (chosen != null) {
            Spacer(Modifier.height(12.dp))
            Group(rows = listOf({ ActionRow(PosText.signIn(chosen), a.onSignIn, mk.link) }))
        }
        Footnote(PosText.withoutFootnote(chosen))
    }
}

/**
 * The point of sale signed in to, led by its mark on a tile (the Logos board, A). The board's place under the name is
 * left out: the location or merchant row just below says it, and is where the location is changed.
 */
@Composable
private fun PosRow(pos: PointOfSale) = Row(
    Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 12.dp).semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    PosTile(pos)
    Text(pos.displayName, fontSize = 17.sp)
}

/**
 * Tor (decision 55): offered while the store has an onion address, through
 * Orbot. A till that connected at the onion address is on Tor already.
 */
@Composable
private fun TorSection(connection: Connection, onionApi: String?, running: Boolean?, onTor: (Boolean) -> Unit) {
    val atOnion = Tor.isOnion(connection.api)
    if (!atOnion && connection.onionApi == null && onionApi == null) return
    SectionLabel("Tor")
    if (atOnion) {
        Group(rows = listOf({ FactRow("Over Tor", "Always") }))
    } else {
        Group(rows = listOf({ SwitchRow("Reach the store over Tor", connection.tor, onTor) }))
    }
    Footnote(
        when {
            connection.overTor && running == false ->
                "Orbot isn't running, so Till can't reach the store. Open Orbot" + if (atOnion) "." else ", or turn this off."
            atOnion -> "Till connected at the store's onion address, so it reaches the store through Orbot."
            connection.tor -> "Till reaches the store at its onion address, through Orbot."
            else -> "The store has an onion address. Turn this on to reach it over Tor, through the Orbot app."
        },
    )
}

/**
 * Settings › Wake-ups (acceptance A1, decision 99): the three choices as
 * one radio group, "My own Bell" with its address field in the same card,
 * then, with a Bell chosen, its status card, the status's footnote and
 * what the Bell learns.
 */
@Composable
private fun WakeUpsSection(w: WakeUpsState, pos: PointOfSale?, problem: String?, a: SettingsActions) {
    // My own Bell tapped with no address saved yet: its field opens, but the tick and the status
    // card stay with the saved choice until "Use this Bell" is pressed (decisions 103, 124).
    var ownPicked by rememberSaveable { mutableStateOf(false) }
    val ownShown = ownPicked || w.choice is WakeUpChoice.OwnBell
    fun pick(choice: WakeUpChoice) {
        ownPicked = false
        a.onWakeUps(choice)
    }
    SectionLabel("Wake-ups")
    Notice(problem, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
    Group(
        Modifier.selectableGroup(),
        rows = listOfNotNull(
            { ChoiceRow("Timer only", w.choice == WakeUpChoice.TimerOnly, { pick(WakeUpChoice.TimerOnly) }, WakeUpsText.timerDetail(pos)) },
            { ChoiceRow("Monokulo Bell", w.choice == WakeUpChoice.MonokuloBell, { pick(WakeUpChoice.MonokuloBell) }, WakeUpsText.bell(w.monokulo)) },
            {
                ChoiceRow("My own Bell", w.choice is WakeUpChoice.OwnBell, {
                    val saved = w.ownBell
                    if (saved != null) pick(WakeUpChoice.OwnBell(saved)) else ownPicked = true
                }, WakeUpsText.ownDetail(w.ownBell))
            },
            if (ownShown) {
                {
                    OwnBellField(w) { text ->
                        // Chosen now: the status card shows this Bell's, as a pick does.
                        ownPicked = false
                        a.onOwnBell(text)
                    }
                }
            } else {
                null
            },
        ),
    )
    // The status is the saved choice's, and stays up while My own Bell waits for its address.
    if (ownPicked) Footnote(WakeUpsText.NO_OWN_BELL_YET)
    if (w.choice == WakeUpChoice.TimerOnly) {
        if (!ownPicked) Footnote(WakeUpsText.timerOnly(pos))
        return
    }
    val status = w.status
    Spacer(Modifier.height(12.dp))
    Group(
        rows = buildList<@Composable () -> Unit> {
            add { FactRow(PosText.checking(pos), WakeUpsText.pace(status)) }
            add { FactRow("Bell", WakeUpsText.word(status), valueColor = if (status is WakeUpStatus.Connected) mk.paid else mk.muted) }
            WakeUpsText.lastWakeUp(status)?.let { at -> add { FactRow("Last wake-up", at) } }
            when (status) {
                WakeUpStatus.NoDistributor -> add { ActionRow("Get ntfy", a.onGetDistributor, mk.link) }
                is WakeUpStatus.ChooseDistributor -> add {
                    Column(Modifier.selectableGroup()) {
                        status.packages.forEach { pkg ->
                            ChoiceRow(w.distributorNames[pkg] ?: pkg, selected = false, onClick = { a.onDistributor(pkg) })
                        }
                    }
                }
                is WakeUpStatus.SignInNeeded -> add {
                    ActionRow(PosText.signIn(status.pointOfSale), { a.onBellSignIn(status.pointOfSale, status.applicationId) }, mk.link)
                }
                else -> Unit
            }
        },
    )
    Footnote(WakeUpsText.footnote(status, pos))
    Footnote(WakeUpsText.privacy(pos))
}

/**
 * My own Bell's address, a fourth row in the choices' card: prefilled with
 * the saved one, "Use this Bell" only for an address that differs and
 * parses, and why one doesn't under the field (acceptance A1.4–A1.5).
 */
@Composable
private fun OwnBellField(w: WakeUpsState, onUse: (String) -> Unit) {
    var text by rememberSaveable(w.ownBell) { mutableStateOf(WakeUpsText.ownField(w.ownBell)) }
    val parsed = BellAddress.parse(text, w.allowHttp)
    val usable = parsed != null && parsed != w.ownBell
    val problem = if (text.isBlank()) null else BellAddress.problem(text, w.allowHttp)
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            text,
            { text = it },
            label = { Text("Bell address") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done, autoCorrectEnabled = false),
            keyboardActions = KeyboardActions(onDone = { if (usable) onUse(text) }),
            modifier = Modifier.fillMaxWidth(),
        )
        if (problem != null) Muted(problem)
        OutlineButton("Use this Bell", { onUse(text) }, Modifier.fillMaxWidth(), enabled = usable)
    }
}

/** A row that's a button: its label in [color]. */
@Composable
private fun ActionRow(label: String, onClick: () -> Unit, color: androidx.compose.ui.graphics.Color) = Box(
    Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 16.dp),
    contentAlignment = Alignment.CenterStart,
) { Text(label, fontSize = 17.sp, color = color) }

/**
 * Lets a 48 dp button reach into its row's padding (10 dp above and below,
 * 12 of the 16 dp at the end), so the row keeps the height of the rows
 * around it and the icon lines up with their values. It still takes taps
 * across all 48 dp.
 */
private fun Modifier.intoRowPadding() = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints)
    val vertical = 10.dp.roundToPx()
    layout(placeable.width - 12.dp.roundToPx(), placeable.height - 2 * vertical) { placeable.place(0, -vertical) }
}
