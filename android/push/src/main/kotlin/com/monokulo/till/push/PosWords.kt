package com.monokulo.till.push

import com.monokulo.till.core.PointOfSale

/**
 * The words Wake-ups' statuses give a point of sale, in the cashier's words
 * (acceptance T5.2): one exhaustive `when` each, so a point of sale added
 * later doesn't compile until it has them. The app's `PosText` holds the
 * screens' own words; these are the reasons a [WakeUpStatus] carries.
 */
object PosWords {
    /** The Bell runs no adapter for [pointOfSale]: refused for good. */
    fun notRung(pointOfSale: PointOfSale): String = "it doesn't ring ${pointOfSale.displayName} tills"

    /**
     * The Bell rings [pointOfSale], but not in the till's [environment]: refused for good
     * ("it doesn't ring Square sandbox tills", "it doesn't ring Clover tills in Europe").
     */
    fun notRung(pointOfSale: PointOfSale, environment: String): String = "it doesn't ring ${tills(pointOfSale, environment)}"

    /** [pointOfSale]'s tills in [environment]; one the point of sale doesn't have by its wire name. */
    private fun tills(pointOfSale: PointOfSale, environment: String): String = when (pointOfSale) {
        PointOfSale.Square -> when (environment) {
            "production" -> "Square production tills"
            "sandbox" -> "Square sandbox tills"
            else -> "Square $environment tills"
        }
        PointOfSale.Clover -> when (environment) {
            "north_america" -> "Clover tills in North America"
            "europe" -> "Clover tills in Europe"
            "latin_america" -> "Clover tills in Latin America"
            "sandbox" -> "Clover sandbox tills"
            else -> "Clover $environment tills"
        }
    }

    /** The point of sale didn't confirm the place the till registered for (`{pos}-refused`). */
    fun refused(pointOfSale: PointOfSale): String =
        "${pointOfSale.displayName} didn't confirm this ${place(pointOfSale)}. Sign in to ${pointOfSale.displayName} again."

    /** What a registration is for at [pointOfSale]: a Square location, a Clover merchant (its only location). */
    fun place(pointOfSale: PointOfSale?): String = when (pointOfSale) {
        PointOfSale.Square, null -> "location"
        PointOfSale.Clover -> "merchant"
    }

    /** Whose account another registration of this device is under. */
    fun account(pointOfSale: PointOfSale?): String = when (pointOfSale) {
        null -> "account"
        PointOfSale.Square, PointOfSale.Clover -> "${pointOfSale.displayName} account"
    }
}
