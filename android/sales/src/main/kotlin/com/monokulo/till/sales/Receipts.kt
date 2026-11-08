package com.monokulo.till.sales

import java.math.BigDecimal
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import java.util.Currency

// What every point of sale's receipts are made with (SquareReceipts, CloverReceipts).

/** The codec refuses line breaks in text. */
internal fun oneLine(s: String): String = s.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")

/** Whole seconds, in the merchant's own time zone (UTC when it isn't known): the codec refuses fractions. */
internal fun receiptTimestamp(at: Instant, timezone: String?): String {
    val zone = timezone?.let { runCatching { ZoneId.of(it) }.getOrNull() } ?: ZoneId.of("UTC")
    return DateTimeFormatter.ISO_OFFSET_DATE_TIME.format(at.truncatedTo(ChronoUnit.SECONDS).atZone(zone))
}

/** How many minor-unit digits [currency] has: 2 for USD, 0 for JPY. */
internal fun minorDigits(currency: String): Int =
    runCatching { Currency.getInstance(currency).defaultFractionDigits }.getOrDefault(2).coerceAtLeast(0)

/** [minor] units of [currency] as a decimal string: 2450 USD is "24.50". */
fun decimalAmount(minor: Long, currency: String): String {
    val digits = minorDigits(currency)
    return BigDecimal.valueOf(minor).movePointLeft(digits).setScale(digits).toPlainString()
}
