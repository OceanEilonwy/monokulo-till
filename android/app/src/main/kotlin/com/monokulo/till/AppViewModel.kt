package com.monokulo.till

import uniffi.till_app.Screen
import android.app.Application
import androidx.lifecycle.AndroidViewModel
import com.monokulo.till.engine.EngineStore
import com.monokulo.till.ui.ReceiptFolds
import uniffi.till_app.Action

/** The sale a screen shows, if it shows one. */
fun Screen.orderIdOrNull(): String? = when (this) {
    is Screen.Sale -> orderId
    is Screen.Code -> orderId
    is Screen.Order -> orderId
    else -> null
}

/**
 * The activity's side of the engine, as Tern's is: the [EngineStore]'s flows
 * to draw, and [send] for what the cashier does. Everything else (where
 * Back goes, what each screen says, the network) is the engine's.
 */
class AppViewModel(app: Application) : AndroidViewModel(app) {
    val store: EngineStore = (app as TillApp).engineStore

    /** The sale pages' order card folds, kept while a sale's order or code is over its page and through a turn. */
    val receiptFolds = ReceiptFolds()

    fun send(action: Action) = store.send(action)
}
