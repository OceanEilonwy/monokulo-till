package com.monokulo.till.engine

import java.math.BigDecimal
import java.text.DecimalFormat
import java.text.NumberFormat
import java.util.Currency
import java.util.Locale
import uniffi.till_app.CurrencySymbols

/**
 * The platform's half of money (docs/decisions/core.md 9 and 99): the
 * device's locale data. The engine works out the amount in minor units and
 * chooses the symbol (`monokulo_client::amounts::written`, so iOS writes the
 * same); this writes the locale's pattern and says how it names symbols.
 */
object Money {
    /** Where the symbol goes in [pattern]'s answer, for the engine to fill in. */
    private const val SIGN = "¤"

    /** International English, whose symbols name the country: "US$", "A$", "CN¥". */
    private val INTERNATIONAL: Locale = Locale.forLanguageTag("en-001")

    /**
     * [minor] units of [currency] (ISO 4217, upper case) in [locale]'s money pattern, with `¤` where the symbol goes:
     * "¤24.50" in the US, "24,50 ¤" in Germany.
     */
    fun pattern(minor: Long, currency: String, locale: Locale = Locale.getDefault()): String = runCatching {
        val code = Currency.getInstance(currency.uppercase(Locale.ROOT))
        val digits = code.defaultFractionDigits.coerceAtLeast(0)
        val format = NumberFormat.getCurrencyInstance(locale).apply {
            this.currency = code
            minimumFractionDigits = digits
            maximumFractionDigits = digits
        }
        if (format is DecimalFormat) {
            format.decimalFormatSymbols = format.decimalFormatSymbols.apply { currencySymbol = SIGN }
        }
        format.format(BigDecimal.valueOf(minor, digits))
    }.getOrElse { "$minor $SIGN" }

    /** How [locale] and international English write [currency]'s symbol, and [locale]'s own currency. */
    fun symbols(currency: String, locale: Locale = Locale.getDefault()): CurrencySymbols {
        val code = runCatching { Currency.getInstance(currency.uppercase(Locale.ROOT)) }.getOrNull()
        val home = runCatching { Currency.getInstance(locale)?.currencyCode }.getOrNull()
        return CurrencySymbols(
            local = code?.getSymbol(locale) ?: currency,
            international = code?.getSymbol(INTERNATIONAL) ?: currency,
            home = home,
        )
    }
}
