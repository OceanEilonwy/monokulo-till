package com.monokulo.till.ui

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/** The shared code's side, at least: sharp in a message or printed. */
private const val SHARE_SIDE = 1024

/** The shared code's quiet zone, in modules: the standard's four. */
private const val SHARE_QUIET = 4

/** Where shared codes are written, under the cache (res/xml/file_paths.xml). */
private const val SHARE_DIR = "shared"

/**
 * The system share sheet for a sale's payment: the whole code [code] as a
 * PNG at least [SHARE_SIDE] px with a white quiet zone, [code] (the payment
 * URI) as its text and [title] as its subject. The picture is drawn and
 * written off the main thread and shared through the app's FileProvider.
 */
suspend fun shareCode(context: Context, code: String, title: String) {
    val uri = withContext(Dispatchers.IO) {
        val module = qrBitmap(code, scale = 1, margin = SHARE_QUIET)
        val scale = (SHARE_SIDE + module.width - 1) / module.width
        val picture = qrBitmap(code, scale = scale, margin = SHARE_QUIET)
        module.recycle()
        val dir = File(context.cacheDir, SHARE_DIR).apply { mkdirs() }
        // One file at a time: the last shared code is the only one kept.
        dir.listFiles()?.forEach { it.delete() }
        val file = File(dir, "monero-payment.png")
        file.outputStream().use { picture.compress(Bitmap.CompressFormat.PNG, 100, it) }
        picture.recycle()
        FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    }
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "image/png"
        putExtra(Intent.EXTRA_STREAM, uri)
        putExtra(Intent.EXTRA_TEXT, code)
        if (title.isNotEmpty()) {
            putExtra(Intent.EXTRA_SUBJECT, title)
            putExtra(Intent.EXTRA_TITLE, title)
        }
        // The share sheet's preview reads the picture through the clip.
        clipData = ClipData.newUri(context.contentResolver, title.ifEmpty { "Monero payment" }, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(send, title.ifEmpty { null }))
}
