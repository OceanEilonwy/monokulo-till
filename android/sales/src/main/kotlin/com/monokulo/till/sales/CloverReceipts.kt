package com.monokulo.till.sales

import com.monokulo.till.receipt.Receipt
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.math.BigDecimal
import java.math.RoundingMode
import java.time.Instant

/**
 * A Clover order as an Open Receipt Format receipt (`docs/design.md` §4),
 * built on the till (decision 134). Clover repeats a line item once per
 * unit, so equal ones are counted onto one line; an item sold by measure is
 * one line of its `unitQty` thousandths. The order carries its total, tax
 * included, but its tax only per line behind another expand: the receipt
 * shows one tax line, the total less the lines, as Clover's paper receipt
 * does.
 *
 * Clover returns at most [EXPANDED_MAX] elements of an expanded collection,
 * so an order with that many line items may have lost some, and the total
 * less the lines would print the lost items' money as tax. Such an order
 * gets no receipt at all, the plain code, rather than one without its tax
 * line: a receipt that lists only some of what was bought, under a total
 * its lines don't add up to, is a wrong receipt, while the plain code still
 * takes the payment and the merchant's own receipt stays whole.
 */
object CloverReceipts {
    /**
     * Null when the merchant's country isn't known (a receipt must name it),
     * or when the order's line items may have been cut at [EXPANDED_MAX].
     */
    fun of(order: CloverOrder, merchant: CloverMerchant, sale: FoundSale): Receipt? {
        val country = merchant.country ?: return null
        if (order.lineItems.elements.size >= EXPANDED_MAX) return null
        val currency = order.currency ?: merchant.currency
        val digits = minorDigits(currency)
        val lines = lines(order.lineItems.elements)
        val sum = lines.sumOf { it.totalMinor }
        val rest = order.total - sum
        val tax = rest.takeIf { it > 0 && sum > 0 }?.let { taxMinor ->
            val rate = BigDecimal.valueOf(taxMinor).divide(BigDecimal.valueOf(sum), RATE_DIGITS, RoundingMode.HALF_UP)
            Receipt.Tax(listOf(Receipt.TaxLine(rate = rate.toDouble(), taxAmount = decimal(taxMinor, digits).toDouble())))
        }
        // Less than its lines: a discount the order took off.
        val discount = (-rest).takeIf { it > 0 }?.let {
            JsonObject(mapOf("discount" to JsonObject(mapOf("amount" to JsonPrimitive(decimal(it, digits).toDouble())))))
        }
        return Receipt(
            receipt = Receipt.Info(
                receiptId = sale.reference ?: order.id.takeLast(8),
                issueTimestamp = receiptTimestamp(Instant.ofEpochMilli(order.createdTime), merchant.timezone),
            ),
            merchant = Receipt.Merchant(legalName = oneLine(merchant.name), countryCode = country),
            lineItems = lines.map {
                Receipt.LineItem(
                    description = it.description,
                    quantity = it.quantity.toDouble(),
                    unitPrice = decimal(it.priceMinor, digits).toDouble(),
                    lineTotal = decimal(it.totalMinor, digits).toDouble(),
                )
            },
            tax = tax,
            payment = Receipt.Payment("monero"),
            total = decimal(order.total, digits).toDouble(),
            currency = currency,
            extensions = discount,
        )
    }

    /** One receipt line, in minor units. */
    internal data class Line(val description: String, val quantity: BigDecimal, val priceMinor: Long, val totalMinor: Long)

    /** Equal per-unit items counted onto one line, in the order they were rung up; a measured item a line of its own. */
    internal fun lines(items: List<CloverOrder.LineItem>): List<Line> {
        data class Key(val name: String?, val price: Long, val note: String?)
        val counted = LinkedHashMap<Any, Line>()
        for (item in items) {
            val description = describe(item)
            val unitQty = item.unitQty
            if (unitQty != null) {
                val quantity = BigDecimal.valueOf(unitQty).movePointLeft(3).stripTrailingZeros()
                val total = BigDecimal.valueOf(item.price).multiply(quantity).setScale(0, RoundingMode.HALF_UP).toLong()
                counted[item.id] = Line(description, quantity, item.price, total)
            } else {
                val key = Key(item.name, item.price, item.note)
                val line = counted[key]
                counted[key] = if (line == null) {
                    Line(description, BigDecimal.ONE, item.price, item.price)
                } else {
                    line.copy(quantity = line.quantity + BigDecimal.ONE, totalMinor = line.totalMinor + item.price)
                }
            }
        }
        return counted.values.toList()
    }

    /** "Latte, extra hot": the item and the cashier's note on it. */
    private fun describe(item: CloverOrder.LineItem): String {
        val name = item.name?.takeIf { it.isNotBlank() } ?: "Item"
        val note = item.note?.takeIf { it.isNotBlank() }
        return oneLine(if (note != null) "$name, $note" else name)
    }

    private fun decimal(minor: Long, digits: Int): BigDecimal = BigDecimal.valueOf(minor).movePointLeft(digits)

    /** The most elements Clover returns of an expanded collection, such as an order's `lineItems`. */
    internal const val EXPANDED_MAX = 100

    /** A tax rate to a thousandth of a percent: 8.875% is 0.08875. */
    private const val RATE_DIGITS = 5
}
