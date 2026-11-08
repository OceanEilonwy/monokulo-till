package com.monokulo.till

import com.monokulo.till.core.PointOfSale
import com.monokulo.till.push.BellAddress
import com.monokulo.till.push.WakeUpStatus
import com.monokulo.till.push.interval
import java.time.ZoneId
import java.util.Locale

/**
 * Settings › Wake-ups in the cashier's words (acceptance A1.6–A1.8): every
 * status's word and footnote from one exhaustive `when` each, so a status
 * added later doesn't compile until it has words (D15), naming the till's
 * point of sale [PosText]'s way (acceptance T6.1; null: none chosen, "your
 * point of sale"). Pure: tested string by string, for each point of sale, in
 * `WakeUpsTextTest`.
 */
object WakeUpsText {
    /** The Timer only row's detail. */
    fun timerDetail(pos: PointOfSale?): String = "Checks ${PosText.name(pos)} every 2 seconds"

    /** The My own Bell row's detail when no address is saved. */
    const val NO_OWN_BELL = "Enter its address"

    /** Under the choices while My own Bell is picked and no address is saved yet: nothing has changed until one is used. */
    const val NO_OWN_BELL_YET = "Enter the address of the Bell you run, then press Use this Bell. Until then Wake-ups stay as they were."

    /** Under every Bell's status: what the Bell learns, and what it doesn't. */
    fun privacy(pos: PointOfSale?): String =
        "The Bell learns which ${PosText.placeAt(pos)} this till is at and when it takes payments, not what was sold. " +
            "Your ${PosText.signInOf(pos)} is shown to it once, to prove the ${PosText.place(pos)}, and never kept."

    /** Under the choices while Timer only is chosen. */
    fun timerOnly(pos: PointOfSale?): String = "Till checks ${PosText.name(pos)} every 2 seconds during a shift. " +
        "A Bell lets ${PosText.name(pos)} wake the till within a second or two of a sale, so it can check every 30 seconds instead."

    /**
     * A Bell's row detail: its address without its scheme, with the scheme
     * only when that isn't https (a debug build's http Bell), so two Bells
     * that differ in nothing else can be told apart (decision 124).
     */
    fun bell(address: BellAddress): String = if (address.url == "https://${address.display}") address.display else address.url

    /** The My own Bell row's detail: the saved address, or how to start. */
    fun ownDetail(saved: BellAddress?): String = saved?.let(::bell) ?: NO_OWN_BELL

    /** What the address field starts with: the saved address as its row shows it, so it reads back as the same address. */
    fun ownField(saved: BellAddress?): String = saved?.let(::bell) ?: ""

    /** The status card's Checking row ([PosText.checking]): the pace the shift's timer runs at, on one line at any font scale. */
    fun pace(status: WakeUpStatus): String = "Every ${status.interval.inWholeSeconds} s"

    /** The status card's Bell row. */
    fun word(status: WakeUpStatus): String = when (status) {
        WakeUpStatus.Off -> "Off"
        WakeUpStatus.NoPos -> "Sign in needed"
        WakeUpStatus.NoDistributor -> "Needs a UnifiedPush app"
        is WakeUpStatus.ChooseDistributor -> "Choose a UnifiedPush app"
        WakeUpStatus.Connecting -> "Connecting…"
        is WakeUpStatus.Connected -> "Connected"
        is WakeUpStatus.MissingWakeups -> "Wake-ups not arriving"
        is WakeUpStatus.Unreachable -> "Not connected"
        is WakeUpStatus.Refused -> "Not connected"
        is WakeUpStatus.SignInNeeded -> "Sign in needed"
    }

    /**
     * The status card's Last wake-up row, written as the shift writes
     * times; null while the status has no ding clock (the row is left out).
     */
    fun lastWakeUp(status: WakeUpStatus, locale: Locale = Locale.getDefault(), zone: ZoneId = ZoneId.systemDefault()): String? {
        val at = when (status) {
            is WakeUpStatus.Connected -> status.lastDingAt
            is WakeUpStatus.MissingWakeups -> status.lastDingAt
            else -> return null
        }
        return at?.let { SaleText.time(it, locale, zone) } ?: "None yet"
    }

    /** The footnote under the cards, for a till at [pos]. */
    fun footnote(status: WakeUpStatus, pos: PointOfSale?): String = when (status) {
        WakeUpStatus.Off -> timerOnly(pos)
        WakeUpStatus.NoPos -> "${PosText.signIn(pos)} to use the Bell. Until then Till shows other tills' sales only."
        WakeUpStatus.NoDistributor -> "Wake-ups need a UnifiedPush app such as ntfy to reach this till. Install one, then come back here. " +
            "Until then Till checks every 2 seconds."
        is WakeUpStatus.ChooseDistributor -> "Deliver wake-ups through one of the UnifiedPush apps above. " +
            "Until you choose, Till checks every 2 seconds."
        WakeUpStatus.Connecting -> "Connecting to the Bell…"
        is WakeUpStatus.Connected -> "${PosText.nameFirst(pos)} wakes this till through the Bell within a second or two of a sale. " +
            "Till still checks every 30 seconds in case a wake-up is lost."
        is WakeUpStatus.MissingWakeups -> "A sale came in without a wake-up from the Bell. " +
            "Till is checking ${PosText.name(pos)} every 2 seconds until the Bell's wake-ups come back."
        is WakeUpStatus.Unreachable -> "Till couldn't reach the Bell (${status.reason}). " +
            "It's checking ${PosText.name(pos)} every 2 seconds and will try the Bell again."
        is WakeUpStatus.Refused -> "The Bell refused this till: ${sentence(status.reason)} Till is checking ${PosText.name(pos)} every 2 seconds."
        is WakeUpStatus.SignInNeeded -> "This Bell uses a different ${status.pointOfSale.displayName} application. " +
            "${PosText.signIn(status.pointOfSale)} through it to use the Bell."
    }

    /** The Shift section's footnote, by the pace, for a till at [pos]. */
    fun shift(status: WakeUpStatus, pos: PointOfSale?): String = when (status) {
        is WakeUpStatus.Connected -> "While it's on, the Bell wakes Till when a sale comes in and it checks ${PosText.name(pos)} " +
            "every 30 seconds between wake-ups. Turn it off at the end of the day."
        WakeUpStatus.Off, WakeUpStatus.NoPos, WakeUpStatus.NoDistributor, is WakeUpStatus.ChooseDistributor,
        WakeUpStatus.Connecting, is WakeUpStatus.MissingWakeups, is WakeUpStatus.Unreachable, is WakeUpStatus.Refused,
        is WakeUpStatus.SignInNeeded,
        -> "While it's on, Till checks ${PosText.name(pos)} every 2 seconds and keeps a notification. Turn it off at the end of the day."
    }

    /** [reason] ending in one full stop: some refusals are sentences of their own ("… Sign in to Clover again."). */
    private fun sentence(reason: String): String = if (reason.endsWith('.')) reason else "$reason."
}
