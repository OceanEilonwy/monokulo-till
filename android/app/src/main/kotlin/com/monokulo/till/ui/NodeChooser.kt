package com.monokulo.till.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import uniffi.till_app.Action
import uniffi.till_screens.NodeCheckLine
import uniffi.till_screens.NodeChoiceRow
import uniffi.till_screens.NodeChooser
import uniffi.till_screens.NodeMark
import uniffi.till_screens.NodeReport

/**
 * The node's Change, as Fob's Node screen (owner, 2026-11-13: "Copy the node selection dialog from Fob wallet"),
 * over D4 or Settings: the node in use with Fob's checks, checked as it opens and by the refresh at the top,
 * "Changing node" while the wallet changes; then Change node, each other node checked with Use on the ones that
 * pass and Remove on this till's own; and Add your own node's sheet. Every word is the engine's.
 */
@Composable
fun NodeChooserDialog(view: NodeChooser, send: (Action) -> Unit, onDismiss: () -> Unit) {
    var adding by rememberSaveable { mutableStateOf(false) }
    // Checked as it opens, as Fob's Node screen is.
    LaunchedEffect(Unit) { send(Action.CheckNodes) }
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Page(
            top = {
                Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    RoundIconButton(MkIcons.Back, "Back", onDismiss)
                    Text(view.title, style = display(22), modifier = Modifier.weight(1f).padding(start = 4.dp).semantics { heading() })
                    RefreshButton(view.refresh.label, view.refresh.busy, view.refresh.enabled) { send(Action.CheckNodes) }
                }
            },
        ) {
            Column(
                Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                val changing = view.changing
                if (changing != null) ChangingCard(view.changingTitle, changing) else view.current?.let { WorkingCard(it) }
                view.problem?.let {
                    Text(it, color = mk.danger, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 4.dp).semantics { liveRegion = LiveRegionMode.Polite })
                }
                Column {
                    Text(
                        view.section.uppercase(), color = mk.muted, fontSize = 13.sp,
                        modifier = Modifier.padding(start = 16.dp, bottom = 6.dp).semantics { heading() },
                    )
                    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(mk.card)) {
                        view.choices.forEach { c ->
                            ChoiceLine(c, onUse = { send(Action.SetNode(c.url)) }, onRemove = { send(Action.RemoveNode(c.url)) })
                        }
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 56.dp)
                                .clickable(enabled = view.add.enabled, role = Role.Button) { adding = true }
                                .padding(horizontal = 20.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            val tint = if (view.add.enabled) mk.link else mk.muted
                            Icon(MkIcons.Add, contentDescription = null, tint = tint, modifier = Modifier.size(20.dp))
                            Text(view.add.label, color = tint, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                        }
                    }
                }
            }
        }
        if (adding) {
            AddNodeSheet(
                view,
                send,
                onUse = { url -> adding = false; send(Action.SetNode(url)) },
                onClose = { adding = false; send(Action.CloseNewNode) },
            )
        }
    }
}

/** The header's refresh: checks again, turning while the checks run. */
@Composable
private fun RefreshButton(label: String, turning: Boolean, enabled: Boolean, onClick: () -> Unit) {
    val spin = rememberInfiniteTransition(label = "refresh")
    val angle by spin.animateFloat(0f, 360f, infiniteRepeatable(tween(900, easing = LinearEasing), RepeatMode.Restart), label = "angle")
    Box(
        Modifier.size(48.dp).clip(CircleShape)
            .clickable(enabled = enabled, role = Role.Button, onClickLabel = label, onClick = onClick)
            .semantics { contentDescription = label; if (turning) stateDescription = "Checking" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(MkIcons.Refresh, contentDescription = null, tint = mk.ink, modifier = Modifier.size(22.dp).rotate(if (turning) angle else 0f))
    }
}

/** A check's mark: a filled circle with a tick, a "!", dots while running, or a dash when not run. */
@Composable
private fun MarkCircle(mark: NodeMark, size: Dp) {
    val (fill: Color, ink: Color, path: ImageVector) = when (mark) {
        NodeMark.PASS -> Triple(mk.paid, mk.ground, MkIcons.Check)
        NodeMark.FAIL -> Triple(mk.danger, mk.ground, MkIcons.Alert)
        NodeMark.CHECKING -> Triple(mk.inset, mk.muted, MkIcons.Dots)
        NodeMark.SKIPPED -> Triple(mk.inset, mk.muted, MkIcons.Dash)
    }
    Box(Modifier.size(size).clip(CircleShape).background(fill), contentAlignment = Alignment.Center) {
        Icon(path, contentDescription = null, tint = ink, modifier = Modifier.size(size * 0.6f))
    }
}

/** A report's checks: each mark and title, its result at the end (a second line under it), lines between. */
@Composable
private fun CheckLines(rows: List<NodeCheckLine>, minHeight: Dp, lines: Boolean) = Column {
    rows.forEach { row ->
        if (lines) Box(Modifier.fillMaxWidth().height(1.dp).background(mk.divider))
        Row(
            Modifier.fillMaxWidth().heightIn(min = minHeight).padding(vertical = 6.dp).clearAndSetSemantics { contentDescription = row.label },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            MarkCircle(row.mark, 20.dp)
            Text(row.title, fontSize = 14.sp, modifier = Modifier.weight(1f))
            val colour = if (row.mark == NodeMark.FAIL) mk.danger else mk.muted
            // The result keeps its words whole; the title wraps first.
            Column(horizontalAlignment = Alignment.End) {
                Text(row.result, fontSize = 13.sp, color = colour, textAlign = TextAlign.End)
                row.second?.let { Text(it, fontSize = 13.sp, color = colour, textAlign = TextAlign.End) }
            }
        }
    }
}

@Composable
private fun NodeCard(padding: PaddingValues, content: @Composable () -> Unit) = Column(
    Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(mk.card).padding(padding),
) { content() }

/** Changing to another node, until it's answered and the checks can start. */
@Composable
private fun ChangingCard(title: String, address: String) = NodeCard(PaddingValues(horizontal = 20.dp, vertical = 18.dp)) {
    Row(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) { heading(); liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        MarkCircle(NodeMark.CHECKING, 40.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = display(22))
            Text(address, fontSize = 13.sp, color = mk.muted)
        }
    }
}

/** The node in use: the verdict, its address, and the checks. */
@Composable
private fun WorkingCard(report: NodeReport) = NodeCard(PaddingValues(start = 20.dp, end = 20.dp, top = 18.dp, bottom = 12.dp)) {
    Row(
        Modifier.fillMaxWidth().padding(bottom = 12.dp).semantics(mergeDescendants = true) { heading(); liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        MarkCircle(report.mark, 40.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(report.verdict, style = display(22))
            // Whole, wrapping if it must: it's what's being checked.
            Text(report.address, fontSize = 13.sp, color = mk.muted)
        }
    }
    CheckLines(report.rows, minHeight = 40.dp, lines = true)
}

/** Another node: its dot, address and one line, Use when it passed, and Remove when it's this till's own. */
@Composable
private fun ChoiceLine(choice: NodeChoiceRow, onUse: () -> Unit, onRemove: () -> Unit) = Row(
    Modifier.fillMaxWidth().heightIn(min = 60.dp).padding(start = 20.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    val dot = when (choice.mark) {
        NodeMark.PASS -> mk.paid
        NodeMark.FAIL -> mk.danger
        else -> mk.outline
    }
    Box(Modifier.size(10.dp).clip(CircleShape).background(dot))
    Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(choice.address, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(choice.line, fontSize = 12.5.sp, color = mk.muted)
    }
    val changing = choice.changing
    val use = choice.useNode
    if (changing != null) {
        Text(changing, color = mk.muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 12.dp))
    } else if (use != null) {
        Box(
            Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp))
                .clickable(enabled = use.enabled, role = Role.Button, onClickLabel = choice.useSaid, onClick = onUse)
                .semantics { contentDescription = choice.useSaid }
                .padding(horizontal = 12.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(use.label, color = mk.link, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
        }
    }
    if (choice.remove) {
        RoundIconButton(MkIcons.Trash, choice.removeSaid, onRemove, iconSize = 20.dp)
    }
}

/**
 * Add your own node (Fob's board 12): an address and the same checks; Use, or Use it anyway when only Up to date
 * failed. The checks show while the field still says what was checked; closing keeps the node if it passed.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AddNodeSheet(view: NodeChooser, send: (Action) -> Unit, onUse: (String) -> Unit, onClose: () -> Unit) {
    val sheet = view.newNode
    var address by rememberSaveable { mutableStateOf("") }
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onClose,
        sheetState = state,
        shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
        containerColor = mk.ground,
        contentColor = mk.ink,
        scrimColor = Fixed.Scrim,
        dragHandle = {
            Box(Modifier.padding(top = 12.dp, bottom = 12.dp).size(width = 40.dp, height = 5.dp).clip(CircleShape).background(mk.edge))
        },
        modifier = Modifier.semantics { paneTitle = sheet.title },
    ) {
        Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 24.dp).imePadding()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(sheet.title, style = display(22), modifier = Modifier.weight(1f).semantics { heading() })
                RoundIconButton(MkIcons.Close, sheet.close, onClose)
            }
            // The checks and the keyboard can be more than the screen holds: the buttons stay reachable.
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    address, { address = it },
                    label = { Text(sheet.fieldLabel) },
                    supportingText = { Text(sheet.hint) },
                    singleLine = true,
                    textStyle = androidx.compose.ui.text.TextStyle(fontFamily = FontFamily.Monospace, fontSize = 15.sp),
                    keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
                    colors = fieldColours(),
                    modifier = Modifier.fillMaxWidth(),
                )
                // What was checked, while the field still says it.
                val report = sheet.report?.takeIf { sheet.checked.isNotEmpty() && sheet.checked == address.trim() }
                if (report != null) {
                    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(mk.card).padding(horizontal = 16.dp, vertical = 4.dp)) {
                        CheckLines(report.rows, minHeight = 44.dp, lines = false)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlineButton(
                            sheet.checkAgain.label, { send(Action.CheckNewNode(address)) }, Modifier.weight(1f),
                            enabled = sheet.checkAgain.enabled, height = 52.dp, oneLine = true,
                        )
                        PrimaryButton(
                            sheet.useNode.label, { onUse(address.trim()) }, Modifier.weight(1f),
                            enabled = sheet.useNode.enabled, height = 52.dp,
                        )
                    }
                } else {
                    PrimaryButton(sheet.check, { send(Action.CheckNewNode(address)) }, Modifier.fillMaxWidth(), enabled = address.isNotBlank(), height = 52.dp)
                }
                Spacer(Modifier.height(4.dp))
            }
        }
    }
}
