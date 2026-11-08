import Foundation

/// Writes `minor` units of `currency` (ISO 4217) the way `locale` writes
/// money: 2450 and "USD" are "$24.50" in the US. The core asks for this
/// through `Platform.formatMoney` (docs/design.md §6), so amounts follow
/// the device's locale without the core carrying locale data.
public func formatMoney(minor: Int64, currency: String, locale: Locale = .current) -> String {
    let formatter = NumberFormatter()
    formatter.numberStyle = .currency
    formatter.locale = locale
    formatter.currencyCode = currency
    let digits = formatter.maximumFractionDigits
    var amount = Decimal(minor)
    for _ in 0..<digits {
        amount /= 10
    }
    return formatter.string(from: amount as NSDecimalNumber) ?? "\(amount) \(currency)"
}
