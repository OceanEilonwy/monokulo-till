package com.monokulo.till.sales

import com.monokulo.till.receipt.Receipt
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.math.BigDecimal
import java.math.RoundingMode
import java.time.Instant

/**
 * A Square order as an Open Receipt Format receipt (`docs/design.md` §4),
 * built on the till: what was bought never leaves it except in the code.
 */
object SquareReceipts {
    fun of(order: SquareOrder, location: SquareLocation, sale: FoundSale): Receipt {
        val digits = minorDigits(order.totalMoney.currency)
        val lines = order.lineItems.map { line ->
            val quantity = line.quantity.toBigDecimalOrNull() ?: BigDecimal.ONE
            val gross = line.grossSalesMoney ?: line.basePriceMoney?.let { Money(it.amount * quantity.toLong(), it.currency) }
            val lineTotal = gross?.let { amount(it.amount, digits) } ?: BigDecimal.ZERO
            // The unit price that makes quantity × price the line's total,
            // when there is one in whole minor units; else the list price.
            val unit = lineTotal.divide(quantity, digits, RoundingMode.HALF_UP)
                .takeIf { it.multiply(quantity).compareTo(lineTotal) == 0 }
                ?: line.basePriceMoney?.let { amount(it.amount, digits) }
                ?: lineTotal
            Receipt.LineItem(
                description = describe(line),
                quantity = quantity.toDouble(),
                unitPrice = unit.toDouble(),
                lineTotal = lineTotal.toDouble(),
            )
        }
        val taxes = order.taxes.mapNotNull { tax ->
            val rate = tax.percentage?.toBigDecimalOrNull() ?: return@mapNotNull null
            val applied = tax.appliedMoney ?: return@mapNotNull null
            Receipt.TaxLine(rate = rate.movePointLeft(2).toDouble(), taxAmount = amount(applied.amount, digits).toDouble())
        }
        val extensions = buildMap {
            order.totalTipMoney?.takeIf { it.amount > 0 }?.let {
                put("tip", JsonObject(mapOf("amount" to JsonPrimitive(amount(it.amount, digits).toDouble()))))
            }
            order.totalDiscountMoney?.takeIf { it.amount > 0 }?.let {
                put("discount", JsonObject(mapOf("amount" to JsonPrimitive(amount(it.amount, digits).toDouble()))))
            }
        }
        return Receipt(
            receipt = Receipt.Info(
                receiptId = sale.reference ?: order.id.takeLast(8),
                issueTimestamp = receiptTimestamp(Instant.parse(order.createdAt), location.timezone),
            ),
            merchant = Receipt.Merchant(
                legalName = oneLine(location.businessName ?: location.name),
                countryCode = location.country,
                displayName = location.name.takeIf { it != location.businessName }?.let(::oneLine),
            ),
            lineItems = lines,
            tax = taxes.takeIf { it.isNotEmpty() }?.let { Receipt.Tax(it) },
            payment = Receipt.Payment("monero"),
            total = amount(order.totalMoney.amount, digits).toDouble(),
            currency = order.totalMoney.currency,
            extensions = extensions.takeIf { it.isNotEmpty() }?.let { JsonObject(it) },
        )
    }

    /** "Flat white (Large), oat milk": the item, its variation unless it's the default, and its modifiers. */
    internal fun describe(line: SquareOrder.LineItem): String {
        val parts = buildList {
            val name = line.name?.takeIf { it.isNotBlank() } ?: "Item"
            val variation = line.variationName?.takeIf { it.isNotBlank() && it != "Regular" }
            add(if (variation != null) "$name ($variation)" else name)
            line.modifiers.mapNotNull { it.name?.takeIf(String::isNotBlank) }.forEach(::add)
        }
        return oneLine(parts.joinToString(", "))
    }

    private fun amount(minor: Long, digits: Int): BigDecimal = BigDecimal.valueOf(minor).movePointLeft(digits)
}
