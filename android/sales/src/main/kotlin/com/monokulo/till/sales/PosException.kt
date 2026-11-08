package com.monokulo.till.sales

/**
 * The point of sale answered with an error, or its sign-in is gone and must
 * be done again ([signInAgain]). Each point of sale's own exception extends
 * it, so the till and the app catch one type (decision 131).
 */
open class PosException(val status: Int, message: String, val signInAgain: Boolean = false) : Exception(message)
