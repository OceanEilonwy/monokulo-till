import Foundation

/// The device's half of money (docs/decisions/core.md 9 and 99): the
/// locale's data. The core works out the amount in minor units and chooses
/// the symbol, so iOS and Android write the same one; these answer
/// `Platform.formatMoney` and `Platform.currencySymbols`.

/// `minor` units of `currency` (ISO 4217) in `locale`'s money pattern, with
/// `¤` where the symbol goes: 2450 and "USD" are "¤24.50" in the US and
/// "24,50 ¤" in Germany. The core puts the symbol in.
public func moneyPattern(minor: Int64, currency: String, locale: Locale = .current) -> String {
    let formatter = NumberFormatter()
    formatter.numberStyle = .currency
    formatter.locale = locale
    formatter.currencyCode = currency
    let digits = formatter.maximumFractionDigits
    formatter.currencySymbol = "\u{a4}"
    var amount = Decimal(minor)
    for _ in 0..<digits {
        amount /= 10
    }
    return formatter.string(from: amount as NSDecimalNumber) ?? "\(amount) \u{a4}"
}

/// How `locale` and international English (`en_001`) write `currency`'s
/// symbol (the code itself where they have none), and `locale`'s own
/// currency.
public func currencySymbols(currency: String, locale: Locale = .current) -> CurrencySymbols {
    func symbol(in locale: Locale) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.locale = locale
        formatter.currencyCode = currency
        return formatter.currencySymbol ?? currency
    }
    return CurrencySymbols(
        local: symbol(in: locale),
        international: symbol(in: Locale(identifier: "en_001")),
        home: locale.currency?.identifier
    )
}
