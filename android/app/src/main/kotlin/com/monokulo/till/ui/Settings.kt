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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import uniffi.till_screens.WalletSection
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
import uniffi.till_app.Action
import uniffi.till_app.Pos
import uniffi.till_app.WakeUps
import uniffi.till_screens.Fact
import uniffi.till_screens.PosMark
import uniffi.till_screens.PosSection
import uniffi.till_screens.SettingsView
import uniffi.till_screens.StatusAction
import uniffi.till_screens.Tone
import uniffi.till_screens.WakeUpsPlace
import uniffi.till_screens.WakeUpsView

/** What Settings does on the device itself; everything else goes to the engine through [send]. */
class SettingsActions(
    val send: (Action) -> Unit,
    /** The alerts switch: Android's own page for Till's notifications. */
    val onAlerts: () -> Unit,
    /** The bubble switch: kept, and Android's page to allow it when it isn't. */
    val onBubble: (Boolean) -> Unit,
    /** Copies the store wallet's whole address. */
    val onCopyAddress: () -> Unit,
    /** "Get ntfy": a UnifiedPush app to install. */
    val onGetDistributor: () -> Unit,
    /** A UnifiedPush app's name, by its package, as Android names it. */
    val distributorName: (String) -> String,
)

/** The engine's point of sale for a [PosMark]. */
fun PosMark.pos(): Pos = when (this) {
    PosMark.SQUARE -> Pos.SQUARE
    PosMark.CLOVER -> Pos.CLOVER
    PosMark.TILL -> Pos.TILL
}

/** Settings (PhoneSettings), as the engine wrote it, with Android's shift and bubble in place of the Dynamic Island. */
@Composable
fun SettingsScreen(s: SettingsView, a: SettingsActions) = Page(
    top = {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            RoundIconButton(MkIcons.Back, "Back", { a.send(Action.Back) })
        }
    },
) {
    Heading("Settings", Modifier.padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp), size = 34)

    // The shop's own wallet in place of the store (core.md decision 234).
    s.wallet?.let { WalletSettings(it, a) }
    if (s.wallet == null) SectionLabel("Store")
    s.store?.takeIf { s.wallet == null }?.let { store ->
        Notice(store.problem, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
        Group(
            rows = listOf<@Composable () -> Unit>(
                {
                    Row(
                        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp).semantics(mergeDescendants = true) {},
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Logo(40.dp)
                        Column(Modifier.weight(1f)) {
                            Text(store.name, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                if (store.encrypted) {
                                    Icon(MkIcons.Lock, contentDescription = "Encrypted", tint = mk.paid, modifier = Modifier.size(11.dp))
                                }
                                Muted(store.host)
                            }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Box(Modifier.size(8.dp).clip(CircleShape).background(if (store.online) mk.paid else mk.outline))
                            Text(store.onlineLabel, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        }
                    }
                },
            ) + store.facts.map { fact ->
                {
                    val address = store.walletAddress
                    if (fact.mono && address != null) {
                        // The short form, with Copy for the whole address.
                        FactRow(
                            fact.label, fact.value, mono = true,
                            trailing = {
                                RoundIconButton(
                                    MkIcons.Copy, "Copy wallet address", a.onCopyAddress,
                                    Modifier.intoRowPadding(), iconSize = 20.dp,
                                )
                            },
                        )
                    } else {
                        FactRow(fact.label, fact.value, mono = fact.mono, valueColor = valueColor(fact))
                    }
                }
            },
        )
        Footnote("To change the wallet, open the store's settings in your Monokulo dashboard. Till can't change it.")
    }
    s.tor?.let { tor ->
        SectionLabel("Tor")
        if (tor.always) {
            Group(rows = listOf({ FactRow("Over Tor", "Always") }))
        } else {
            Group(rows = listOf({ SwitchRow("Reach the store over Tor", tor.on, { a.send(Action.SetTor(it)) }) }))
        }
        Footnote(tor.footnote)
    }

    PosSection(s.pointOfSale, a)

    // Counters and cash (docs/decisions/core.md 113 to 117), drawn plainly.
    s.counters?.let { c ->
        SectionLabel("Alert for")
        val rows = mutableListOf<@Composable () -> Unit>()
        c.thisDevice?.let { d -> rows += { ChoiceRow(d.label, d.selected, { a.send(Action.SetAlertFor(false)) }, d.detail) } }
        rows += { ChoiceRow(c.everyDevice.label, c.everyDevice.selected, { a.send(Action.SetAlertFor(true)) }, c.everyDevice.detail) }
        rows += { ActionRow(c.findAgain.label, { a.send(Action.FindDevice) }, mk.link) }
        Group(rows = rows)
        Footnote(c.footnote)
        SectionLabel("Other counters")
        Group(rows = listOf({ SwitchRow("Show their sales in the list", c.showOthers, { a.send(Action.SetShowOtherCounters(it)) }) }))
        Footnote(c.showOthersFootnote)
    }
    s.cash?.let { c ->
        // One switch, on by default (docs/decisions/core.md 182).
        SectionLabel(c.title)
        Group(rows = listOf({ SwitchRow(c.label, c.on, { a.send(Action.SetCashAsMonero(it)) }) }))
        Footnote(c.footnote)
    }

    s.shift?.let { shift ->
        SectionLabel("Shift")
        Group(rows = listOf({ SwitchRow("Watch for sales", shift.on, { on -> a.send(if (on) Action.StartShift else Action.StopShift) }) }))
        Footnote(shift.footnote)
    }

    // A keypad till shows no notifications (core.md decision 260).
    if (s.notifications) {
        SectionLabel("Alerts")
        Group(rows = listOf({ SwitchRow("New sale alerts", s.alerts.on, { a.onAlerts() }) }))
        Footnote(s.alerts.footnote)
    }

    s.bubble?.let { bubble ->
        SectionLabel("Bubble")
        Group(rows = listOf({ SwitchRow("Float over other apps", bubble.on, a.onBubble) }))
        Footnote(bubble.footnote)
    }

    // A till on its own wallet hears payments only while it's open: no Bell to wake it.
    if (s.wallet == null) (s.wakeUps as? WakeUpsPlace.Inline)?.let { WakeUpsSection(it.view, a) }

    Spacer(Modifier.height(22.dp))
    val wallet = s.wallet
    if (wallet != null) {
        WalletDisconnect(wallet, a)
    } else {
        Group(rows = listOf({ ActionRow("Disconnect this device", { a.send(Action.Disconnect) }, mk.danger) }))
        Footnote("Removes Till from the store's Apps: its key stops working at once. Payments already made aren't affected.")
    }
    Spacer(Modifier.height(32.dp))
}

/**
 * Settings › Payments for a till on the shop's own wallet (core.md decisions 234 to 236): how Till sees payments,
 * the wallet with Copy, this till's number or Fob's account, the node, Paid after and the code's expiry, the price
 * source and what the wallet warns of. The steppers, node row and its Change are D4's own, so both send the same.
 */
@Composable
private fun WalletSettings(w: WalletSection, a: SettingsActions) {
    var changingNode by remember { mutableStateOf(false) }
    SectionLabel(w.title)
    Notice(w.problem, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
    Group(
        rows = buildList<@Composable () -> Unit> {
            add { FactRow(w.mode.label, w.mode.value, valueColor = valueColor(w.mode)) }
            w.fob?.let { fob -> add { FactRow(fob.label, fob.value, valueColor = valueColor(fob)) } }
            w.facts.forEach { fact ->
                add {
                    if (fact.mono && w.walletAddress.isNotEmpty()) {
                        FactRow(
                            fact.label, fact.value, mono = true,
                            trailing = { RoundIconButton(MkIcons.Copy, "Copy wallet address", a.onCopyAddress, Modifier.intoRowPadding(), iconSize = 20.dp) },
                        )
                    } else {
                        FactRow(fact.label, fact.value, mono = fact.mono, valueColor = valueColor(fact))
                    }
                }
            }
            w.account?.let { account -> add { FactRow(account.label, account.value, valueColor = valueColor(account)) } }
            // Change the wallet, here with what it changes (docs/decisions/core.md 261): off while a sale waits.
            w.change?.let { change ->
                add { ActionRow(change.label, { if (change.enabled) a.send(Action.ChangeWallet) }, if (change.enabled) mk.link else mk.muted) }
            }
        },
    )
    if (w.change != null) Footnote(w.changeFootnote)
    w.tillNumber?.let { number ->
        Spacer(Modifier.height(16.dp))
        Group(rows = listOf({ StepperRow(number) { up -> a.send(Action.StepTillNumber(up)) } }))
    }
    w.lookahead?.let { WarningLine(it) }
    w.node?.let { node ->
        Spacer(Modifier.height(16.dp))
        NodeCard(node, { changingNode = true }, { a.send(Action.SetLightServer(node.url, it)) })
        val chooser = node.chooser
        // A node's Change is Fob's Node screen; a light wallet server's, its address and switch.
        if (changingNode && chooser != null) {
            NodeChooserDialog(chooser, a.send, onDismiss = { changingNode = false })
        } else if (changingNode) {
            NodeDialog(node) { url, selfSigned ->
                changingNode = false
                when {
                    url == null -> {}
                    selfSigned != null -> a.send(Action.SetLightServer(url, selfSigned))
                    else -> a.send(Action.SetNode(url))
                }
            }
        }
    }
    Spacer(Modifier.height(16.dp))
    Group(
        rows = listOf(
            { StepperRow(w.paidAfter) { up -> a.send(Action.StepPaidAfter(up)) } },
            { StepperRow(w.expiry) { up -> a.send(Action.StepCodeExpiry(up)) } },
        ),
    )
    w.warnings.forEach { WarningLine(it) }
    Footnote(w.footnote)

    SectionLabel(w.prices.label)
    ProviderGroup(w.prices.providers, w.prices.asking, "") { a.send(Action.ChoosePriceProvider(it)) }
    Footnote(w.prices.footnote)
}

/** A line the wallet warns of, in the warning tone, under the card it's about. */
@Composable
private fun WarningLine(text: String) = Row(
    Modifier.padding(start = 32.dp, end = 32.dp, top = 8.dp),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    Icon(MkIcons.Warning, contentDescription = null, tint = mk.warn, modifier = Modifier.size(16.dp).padding(top = 2.dp))
    Text(text, color = mk.warn, fontSize = 13.sp, lineHeight = 18.sp)
}

/** Erase this till, asked first: a view key can't be taken back. */
@Composable
private fun WalletDisconnect(w: WalletSection, a: SettingsActions) {
    var asking by remember { mutableStateOf(false) }
    Group(rows = listOf({ ActionRow(w.disconnect.label, { if (w.disconnect.enabled) asking = true }, mk.danger) }))
    Footnote(w.disconnectFootnote)
    if (asking) {
        AlertDialog(
            onDismissRequest = { asking = false },
            title = { Text(w.disconnectAsk) },
            text = { Text(w.disconnectAskDetail) },
            confirmButton = { LinkButton(w.disconnectConfirm, { asking = false; a.send(Action.Disconnect) }, color = mk.danger) },
            dismissButton = { LinkButton("Cancel", { asking = false }, color = mk.ink) },
            containerColor = mk.ground,
        )
    }
}

/** A fact's value colour: its tone, the muted secondary ink when plain. */
@Composable
private fun valueColor(fact: Fact) = if (fact.tone == Tone.PLAIN) mk.muted else toneColor(fact.tone)

/**
 * Settings › Point of sale (acceptance T1.2): signed in, which one and its
 * facts, Sign out and Change the point of sale; not signed in, the choice
 * rows and the chosen one's sign-in.
 */
@Composable
private fun PosSection(p: PosSection, a: SettingsActions) {
    SectionLabel("Point of sale")
    when (p) {
        is PosSection.SignedIn -> {
            Group(
                rows = buildList<@Composable () -> Unit> {
                    add { PosRow(p.mark, p.name) }
                    p.account?.let { add { FactRow(it.label, it.value) } }
                    add {
                        val row = if (p.changeLocation) Modifier.clickable(onClickLabel = "Change the location") { a.send(Action.ChangeLocation) } else Modifier
                        Box(row) { FactRow(p.place.label, p.place.value) }
                    }
                    add { FactRow(p.paymentType.label, p.paymentType.value, valueColor = valueColor(p.paymentType)) }
                    p.lookAgain?.let { add { ActionRow(it, { a.send(Action.LookAgain) }, mk.link) } }
                    add { ActionRow(p.signOut, { a.send(Action.SignOutOfPos) }, mk.link) }
                    add { ActionRow(p.change, { a.send(Action.OpenPointOfSale) }, mk.link) }
                },
            )
            p.footnotes.forEach { Footnote(it) }
        }
        is PosSection.Keypad -> {
            Group(
                rows = buildList<@Composable () -> Unit> {
                    add { PosRow(p.mark, p.name) }
                    p.facts.forEach { fact -> add { FactRow(fact.label, fact.value) } }
                    add { ActionRow(p.change, { a.send(Action.OpenPointOfSale) }, mk.link) }
                },
            )
            Footnote(p.footnote)
        }
        is PosSection.Choosing -> {
            PosChoices(p.choices, onChoose = { a.send(Action.ChoosePointOfSale(it.pos())) })
            p.signIn?.let { signIn ->
                Spacer(Modifier.height(12.dp))
                Group(rows = listOf({ ActionRow(signIn, { a.send(Action.SignIn(null)) }, mk.link) }))
            }
            Footnote(p.footnote)
        }
    }
}

/**
 * The point of sale signed in to, led by its mark on a tile (the Logos board, A). The board's place under the name is
 * left out: the location or merchant row just below says it, and is where the location is changed.
 */
@Composable
private fun PosRow(mark: PosMark, name: String) = Row(
    Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 16.dp, vertical = 12.dp).semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    PosTile(mark)
    Text(name, fontSize = 17.sp)
}

/**
 * Settings › Wake-ups (acceptance A1, decision 99), as the engine wrote it:
 * the three choices as one radio group, "My own Bell" with its address
 * field in the same card, then, with a Bell chosen, its status card, the
 * status's footnote and what the Bell learns.
 */
@Composable
private fun WakeUpsSection(w: WakeUpsView, a: SettingsActions) {
    SectionLabel("Wake-ups")
    Notice(w.problem, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp))
    Group(
        Modifier.selectableGroup(),
        rows = listOfNotNull(
            { ChoiceRow(w.timer.label, w.timer.selected, { a.send(Action.ChooseWakeUps(WakeUps.Timer)) }, w.timer.detail) },
            { ChoiceRow(w.monokuloBell.label, w.monokuloBell.selected, { a.send(Action.ChooseWakeUps(WakeUps.MonokuloBell)) }, w.monokuloBell.detail) },
            // With an address saved the engine picks it at once; with none, its field opens and the saved choice stays
            // ticked until "Use this Bell" (decisions 103, 124).
            { ChoiceRow(w.ownBell.label, w.ownBell.selected, { a.send(Action.EditOwnBell(true, null)) }, w.ownBell.detail) },
            w.ownField?.let { field ->
                {
                    OwnBellField(field.initial, field.problem, field.useThis.label, field.useThis.enabled, onType = { a.send(Action.EditOwnBell(!w.ownBell.selected, it)) }) {
                        a.send(Action.ChooseWakeUps(WakeUps.OwnBell(it)))
                    }
                }
            },
        ),
    )
    w.notes.forEach { Footnote(it) }
    val status = w.status ?: return
    Spacer(Modifier.height(12.dp))
    Group(
        rows = buildList<@Composable () -> Unit> {
            add { FactRow(status.checking.label, status.checking.value) }
            add { FactRow(status.bell.label, status.bell.value, valueColor = valueColor(status.bell)) }
            status.lastWakeUp?.let { add { FactRow(it.label, it.value) } }
            when (val action = status.action) {
                StatusAction.GetDistributor -> add { ActionRow("Get ntfy", a.onGetDistributor, mk.link) }
                is StatusAction.ChooseDistributor -> add {
                    Column(Modifier.selectableGroup()) {
                        action.apps.forEach { app ->
                            ChoiceRow(a.distributorName(app.`package`), selected = false, onClick = { a.send(Action.ChooseDistributor(app.`package`)) })
                        }
                    }
                }
                is StatusAction.SignIn -> add {
                    ActionRow(action.label, { a.send(Action.SignIn(action.applicationId)) }, mk.link)
                }
                StatusAction.OpenAlertSettings, null -> Unit
            }
        },
    )
    Footnote(status.footnote)
    Footnote(status.privacy)
}

/**
 * My own Bell's address, a fourth row in the choices' card: prefilled with
 * the saved one ([initial]); the engine says whether what's typed is a
 * Bell's ([problem], [usable]), and "Use this Bell" chooses it (acceptance
 * A1.4–A1.5). The keyboard and the field's text are the platform's.
 */
@Composable
private fun OwnBellField(initial: String, problem: String?, useThis: String, usable: Boolean, onType: (String) -> Unit, onUse: (String) -> Unit) {
    var text by rememberSaveable(initial) { mutableStateOf(initial) }
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            text,
            {
                text = it
                onType(it)
            },
            label = { Text("Bell address") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done, autoCorrectEnabled = false),
            keyboardActions = KeyboardActions(onDone = { if (usable) onUse(text) }),
            modifier = Modifier.fillMaxWidth(),
        )
        if (problem != null) Muted(problem)
        OutlineButton(useThis, { onUse(text) }, Modifier.fillMaxWidth(), enabled = usable)
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
