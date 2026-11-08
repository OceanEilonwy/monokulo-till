package com.monokulo.till.notify

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import com.monokulo.till.MainActivity
import com.monokulo.till.TillApp
import com.monokulo.till.TillService

/**
 * A sale notification's "Show code": opens the bubble's panel over the
 * app in front, or Till itself when there's no bubble. An activity,
 * not the service, so that tapping it closes the notification shade (and
 * so Android 12+'s ban on notification trampolines never applies). It
 * shows nothing and finishes at once.
 */
class ShowCodeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val bubble = (application as TillApp).engineStore.bubble.value
        val order = intent.getStringExtra(MainActivity.EXTRA_ORDER)
        // The bubble is up over the app in front (the engine's word) and Android still lets it draw.
        if (bubble?.shown == true && Settings.canDrawOverlays(this)) {
            startService(Intent(this, TillService::class.java).setAction(TillService.ACTION_SHOW_CODE).putExtra(MainActivity.EXTRA_ORDER, order))
        } else {
            startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP).putExtra(MainActivity.EXTRA_ORDER, order))
        }
        finish()
    }
}
