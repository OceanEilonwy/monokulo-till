package com.monokulo.till.engine

import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * The six of `core`'s AmountsTest that read the JVM's locale data, which
 * decision 9 left with the platform's `format_money`: amounts now arrive in
 * minor units, as the engine sends them (the other three are in
 * `monokulo-client`'s `amounts`).
 */
class MoneyTest {
    private val australia = Locale.forLanguageTag("en-AU")
    private val japan = Locale.JAPAN
    private val germany = Locale.GERMANY

    @Test
    fun `an amount is written the way the till's locale writes money`() {
        assertEquals("$24.50", Money.format(2450, "USD", Locale.US))
        assertEquals("€8.00", Money.format(800, "EUR", Locale.UK))
        assertEquals("¥1,200", Money.format(1200, "JPY", Locale.US))
        assertEquals("8,00 €", Money.format(800, "EUR", germany))
    }

    @Test
    fun `an amount carries the narrow symbol, never the currency's code`() {
        assertEquals("$4.50", Money.format(450, "USD", Locale.UK))
        assertEquals("¥1,200", Money.format(1200, "JPY", Locale.UK))
        assertEquals("¥1,200", Money.format(1200, "JPY", australia))
        assertEquals("€8.00", Money.format(800, "EUR", australia))
        assertEquals("$6.80", Money.format(680, "AUD", australia))
        // Japan writes its own yen full width.
        assertEquals("￥1,200", Money.format(1200, "JPY", japan))
        assertEquals("$4.50", Money.format(450, "USD", japan))
        assertEquals("€8.00", Money.format(800, "EUR", japan))
        assertEquals("4,50 $", Money.format(450, "USD", germany))
        assertEquals("1.200 ¥", Money.format(1200, "JPY", germany))
        assertEquals("£3.20", Money.format(320, "GBP", Locale.US))
    }

    @Test
    fun `a dollar the till's own dollar would be mistaken for says which it is`() {
        assertEquals("US$4.50", Money.format(450, "USD", australia))
        assertEquals("A$6.80", Money.format(680, "AUD", Locale.US))
        assertEquals("A$6.80", Money.format(680, "AUD", Locale.CANADA))
        assertEquals("CA$2.00", Money.format(200, "CAD", Locale.US))
        assertEquals("US$4.50", Money.format(450, "USD", Locale.CANADA))
    }

    @Test
    fun `a dollar no own dollar clashes with keeps the narrow symbol`() {
        assertEquals("$6.80", Money.format(680, "AUD", Locale.UK))
        assertEquals("6,80 $", Money.format(680, "AUD", germany))
        assertEquals("$6.80", Money.format(680, "AUD", japan))
        // A locale with no country has no currency of its own to clash with.
        assertEquals("$4.50", Money.format(450, "USD", Locale.ENGLISH))
    }

    @Test
    fun `a currency at home keeps the symbol its own locale writes`() {
        assertEquals("R$ 4,50", Money.format(450, "BRL", Locale.forLanguageTag("pt-BR")))
        assertEquals("R4,50", Money.format(450, "ZAR", Locale.forLanguageTag("en-ZA")))
        assertEquals("4,50 kr", Money.format(450, "SEK", Locale.forLanguageTag("sv-SE")))
        assertEquals("$4.50", Money.format(450, "SGD", Locale.forLanguageTag("en-SG")))
        assertEquals("4,50 zł", Money.format(450, "PLN", Locale.forLanguageTag("pl-PL")))
    }

    @Test
    fun `a symbol whose letters are the symbol keeps them abroad`() {
        assertEquals("R$4.50", Money.format(450, "BRL", Locale.US))
    }

    @Test
    fun `a currency with no symbol follows the number after a space`() {
        assertEquals("4.50 CHF", Money.format(450, "CHF", Locale.US))
        assertEquals("4,50 CHF", Money.format(450, "CHF", germany))
    }
}
