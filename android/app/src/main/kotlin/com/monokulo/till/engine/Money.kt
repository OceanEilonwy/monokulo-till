package com.monokulo.till.engine

import java.math.BigDecimal
import java.text.DecimalFormat
import java.text.NumberFormat
import java.util.Currency
import java.util.Locale

/**
 * The platform's half of money (docs/decisions/core.md 9): the engine works
 * out the amount in minor units and the currency, and the device's locale
 * writes it. Ported from `core`'s `Amounts.fiat`, whose six tests that read
 * the JVM's locale data stayed with it (`MoneyTest`).
 */
object Money {
    /**
     * [minor] units of [currency] (ISO 4217, upper case) the way [locale]
     * writes money, with the currency's narrow symbol: 2450 USD is "$24.50"
     * in the US and in Britain alike, since a store takes one currency. The
     * symbol says which dollar only when the till's own currency would read
     * the same: USD on an Australian till is "US$24.50". A currency with no
     * symbol ("CHF") follows the number after a space.
     */
    fun format(minor: Long, currency: String, locale: Locale = Locale.getDefault()): String = runCatching {
        val code = Currency.getInstance(currency.uppercase(Locale.ROOT))
        val digits = code.defaultFractionDigits.coerceAtLeast(0)
        val number = BigDecimal.valueOf(minor, digits)
        val narrow = narrow(code, locale) ?: return@runCatching NumberFormat.getNumberInstance(locale).apply {
            minimumFractionDigits = digits
            maximumFractionDigits = digits
        }.format(number) + " " + code.currencyCode
        val format = NumberFormat.getCurrencyInstance(locale).apply {
            this.currency = code
            minimumFractionDigits = digits
            maximumFractionDigits = digits
        }
        if (format is DecimalFormat) {
            format.decimalFormatSymbols = format.decimalFormatSymbols.apply {
                currencySymbol = if (clashes(code, narrow, locale)) distinct(code, locale) else narrow
            }
        }
        format.format(number)
    }.getOrElse { "$minor ${currency.uppercase(Locale.ROOT)}" }

    /** International English, whose symbols name the country: "US$", "A$", "CN¥". */
    private val INTERNATIONAL: Locale = Locale.forLanguageTag("en-001")

    /** Symbols that put country letters before a shared sign: "US$" is "$", "CN¥" is "¥". */
    private val PREFIXED = setOf("US$", "A$", "AU$", "CA$", "HK$", "NZ$", "MX$", "NT$", "CN¥", "JP¥")

    /**
     * [code]'s narrow symbol: [locale]'s own ("R$" in Brazil, "kr" in
     * Sweden), or the international one where [locale] writes only the code,
     * with country letters taken off the shared signs ("US$" to "$"). Null
     * when neither has a symbol ("CHF").
     */
    private fun narrow(code: Currency, locale: Locale): String? {
        val symbol = code.getSymbol(locale).takeIf { it != code.currencyCode }
            ?: code.getSymbol(INTERNATIONAL).takeIf { it != code.currencyCode }
            ?: return null
        return if (symbol in PREFIXED) symbol.filterNot { it in 'A'..'Z' } else symbol
    }

    /** Whether the till's own currency is another one written with the same narrow symbol. */
    private fun clashes(code: Currency, narrow: String, locale: Locale): Boolean {
        val own = runCatching { Currency.getInstance(locale) }.getOrNull() ?: return false
        return own != code && narrow(own, locale) == narrow
    }

    /**
     * A symbol that tells [code] apart: [locale]'s own when it has one ("A$"
     * in the US, "AU$" in Germany), else the international one ("US$" in
     * Australia, which writes USD as just the code).
     */
    private fun distinct(code: Currency, locale: Locale): String {
        val local = code.getSymbol(locale)
        val letters = local.any { it in 'A'..'Z' || it in 'a'..'z' }
        val symbol = local.any { !(it in 'A'..'Z' || it in 'a'..'z') }
        return if (letters && symbol) local else code.getSymbol(INTERNATIONAL)
    }
}
