package com.monokulo.till.notify

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.os.Build
import android.widget.Toast
import androidx.annotation.ColorInt
import androidx.annotation.DrawableRes
import androidx.compose.ui.graphics.toArgb
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.createBitmap
import com.monokulo.till.MainActivity
import com.monokulo.till.R
import com.monokulo.till.PosText
import com.monokulo.till.SaleText
import com.monokulo.till.core.PointOfSale
import com.monokulo.till.core.SalePhase
import com.monokulo.till.sales.TillSale
import com.monokulo.till.sales.live
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.qrBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * What the shift's ongoing notification says about a sale (the design's
 * Shade and Chips boards), worked out from the sale alone. Its [icon] is
 * the status bar chip's: the ring filling a quarter a step, or the tick
 * once paid, and nothing else, no amount and no words.
 */
data class Face(
    @param:DrawableRes val icon: Int,
    @param:ColorInt val color: Int,
    val title: String,
    /** "$13.88 · 3 items · waiting for payment". */
    val text: String,
    /** What was bought ("2 × Flat white, Almond croissant"): the private version only, null without a receipt. */
    val detail: String?,
    /** The lock screen's text: the amount and the phase, never an item. */
    val publicText: String,
    /** How many of the four steps are filled. */
    val step: Int,
    /** The customer still has to scan: "Full screen" beside "Show code", else "Copy address". */
    val fullScreen: Boolean,
) {
    companion object {
        private val accent = Fixed.Accent.toArgb()
        private val paidGreen = Fixed.Paid.toArgb()

        /** A sale whose code is up, in [location] (the till's Square location or Clover merchant). */
        fun of(sale: TillSale, location: String?): Face {
            val (icon, title, phrase) = when (sale.phase) {
                SalePhase.Seen -> Triple(R.drawable.ic_mark_q2, "Seen on the network", "waiting for confirmation")
                SalePhase.Confirming -> Triple(R.drawable.ic_mark_q3, "Confirming", "nearly there")
                SalePhase.Partial -> Triple(R.drawable.ic_mark_q1, "Part paid · ${SaleText.place(sale, location)}", "ask the customer to pay the rest")
                else -> Triple(R.drawable.ic_mark_q1, "New Monero sale · " + (location ?: SaleText.place(sale, location)), "waiting for payment")
            }
            val amount = SaleText.amount(sale)
            val receipt = sale.receipt
            return Face(
                icon = icon,
                color = accent,
                title = title,
                text = listOfNotNull(amount, receipt?.let(SaleText::items), phrase).joinToString(" · "),
                // Every line: Android ellipsises what doesn't fit.
                detail = receipt?.takeIf { it.lineItems.isNotEmpty() }?.let { SaleText.firstItems(it, show = it.lineItems.size) },
                publicText = "$amount · $phrase",
                step = SaleText.step(sale.phase),
                fullScreen = sale.phase == SalePhase.Waiting || sale.phase == SalePhase.Partial,
            )
        }

        /** The few seconds after [sale] is paid: a green tick in the chip. */
        fun paid(sale: TillSale): Face {
            val text = "${SaleText.amount(sale)} · ${SaleText.xmr(sale)} received"
            return Face(
                icon = R.drawable.ic_tick,
                color = paidGreen,
                title = "Paid · ${SaleText.title(sale)}",
                text = text,
                detail = null,
                publicText = text,
                step = 4,
                fullScreen = false,
            )
        }
    }
}

/** What the shift's one ongoing notification is about. */
sealed interface Ongoing {
    /** No sale's code is up: what's being watched. */
    data object Watching : Ongoing

    /** The newest sale whose code is up. */
    data class Sale(val sale: TillSale) : Ongoing

    /** A sale just paid, for [com.monokulo.till.ui.Motion.Dwell.PAID_MS]. */
    data class Paid(val sale: TillSale) : Ongoing

    companion object {
        /**
         * The sale [justPaid] while it shows as paid, else the newest live
         * sale, else watching. A sale closed, expired or reversed is no
         * longer live, so the chip goes with it at once.
         */
        fun of(sales: List<TillSale>, justPaid: String?): Ongoing {
            sales.firstOrNull { it.orderId == justPaid && it.phase == SalePhase.Paid }?.let { return Paid(it) }
            return sales.firstOrNull { it.live }?.let(::Sale) ?: Watching
        }
    }
}

/**
 * The till's notifications (`docs/design.md` §9 "notifications"; the
 * design's Shade and Chips boards). One ongoing notification for the
 * shift, which becomes the current sale with its progress (Waiting, Seen,
 * Confirming) and is promoted to a Live Update on Android 16+, its status
 * bar chip the small icon alone; a green tick for a few seconds once the
 * sale is paid; then a short "Paid" or "No payment yet" one of its own,
 * not promoted, for each sale that ends.
 */
class SaleNotifications(private val context: Context) {
    private val accent = Fixed.Accent.toArgb()
    private val paidGreen = Fixed.Paid.toArgb()

    fun channels() {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_SHIFT, context.getString(R.string.channel_shift), NotificationManager.IMPORTANCE_LOW),
        )
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_SALES, context.getString(R.string.channel_sales), NotificationManager.IMPORTANCE_HIGH),
        )
    }

    /** Whether the cashier lets Till notify (Android 13+ asks). */
    fun allowed(): Boolean =
        (
            Build.VERSION.SDK_INT < 33 ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
            ) && NotificationManagerCompat.from(context).areNotificationsEnabled()

    /** The shift's notification with no sale: what's being watched. */
    fun watching(store: String, location: String?): Notification = NotificationCompat.Builder(context, CHANNEL_SHIFT)
        .setSmallIcon(R.drawable.ic_mark)
        .setColor(accent)
        .setContentTitle(context.getString(R.string.watching_title))
        .setContentText(listOfNotNull(store.ifEmpty { null }, location).joinToString(" · "))
        .setOngoing(true)
        .setSilent(true)
        .setCategory(NotificationCompat.CATEGORY_SERVICE)
        .setContentIntent(openApp(null))
        .build()

    /**
     * The shift's notification for [sale], waiting for its money. [alert]
     * makes it a heads-up (a new sale). Its large icon is the code, encoded
     * off the main thread: with a receipt in it that's a version-25 code at
     * four pixels a module, about 880 kB of pixels, and the service builds
     * this on every phase change.
     */
    suspend fun sale(sale: TillSale, location: String?, alert: Boolean): Notification {
        val face = Face.of(sale, location)
        val showCode = showCode(sale.orderId)
        val icon = withContext(Dispatchers.Default) { runCatching { qrBitmap(sale.code.uri, scale = 4, margin = 2) }.getOrNull() }
        val builder = live(face, sale, if (alert || sale.phase == SalePhase.Waiting) CHANNEL_SALES else CHANNEL_SHIFT)
            .setOnlyAlertOnce(!alert)
            .setLargeIcon(icon)
            .setContentIntent(showCode)
            .addAction(0, "Show code", showCode)
        if (face.fullScreen) {
            builder.addAction(0, "Full screen", openApp(sale.orderId))
        } else {
            builder.addAction(0, "Copy address", copy(sale))
        }
        return builder.build()
    }

    /** The shift's notification for the few seconds after [sale] is paid: the chip's green tick. Silent: [paid] is the one that alerts. */
    fun justPaid(sale: TillSale): Notification = live(Face.paid(sale), sale, CHANNEL_SHIFT)
        .setOnlyAlertOnce(true)
        .setSilent(true)
        .setLargeIcon(tick())
        .setContentIntent(openApp(sale.orderId))
        .build()

    /**
     * A Live Update showing [face]: promoted on Android 16+, where its
     * status bar chip is the small icon alone. Nothing the chip would
     * print is set: no short critical text, and no time (Android shows a
     * chronometer or a future `when` as the chip's text), so the code's
     * countdown is on the panel and the sale screen, not here. The
     * private version names what was bought; the public one, on the lock
     * screen, never does.
     */
    private fun live(face: Face, sale: TillSale, channel: String): NotificationCompat.Builder {
        val style = NotificationCompat.ProgressStyle()
            .setProgressSegments(List(4) { NotificationCompat.ProgressStyle.Segment(1).setColor(face.color) })
            .setStyledByProgress(true)
            .setProgress(face.step)
        val public = NotificationCompat.Builder(context, channel)
            .setSmallIcon(face.icon)
            .setColor(face.color)
            .setContentTitle(face.title)
            .setContentText(face.publicText)
            .setShowWhen(false)
            .build()
        return NotificationCompat.Builder(context, channel)
            .setSmallIcon(face.icon)
            .setColor(face.color)
            .setContentTitle(face.title)
            // The items go on a line of their own: ProgressStyle takes the style a second line would need.
            .setContentText(listOfNotNull(face.text, face.detail).joinToString("\n"))
            .setSubText(SaleText.title(sale))
            .setShowWhen(false)
            .setStyle(style)
            .setOngoing(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(public)
            .setRequestPromotedOngoing(true)
    }

    /** Replaces the sale's progress once it's paid: short, and gone after a while; [quiet] (no banner) while Till is on screen saying so. */
    fun paid(sale: TillSale, quiet: Boolean) = post(
        sale,
        NotificationCompat.Builder(context, CHANNEL_SALES)
            .setSmallIcon(R.drawable.ic_mark)
            .setColor(paidGreen)
            .setLargeIcon(tick())
            .setContentTitle("Paid · ${SaleText.title(sale)}")
            .setContentText("${SaleText.amount(sale)} · ${SaleText.xmr(sale)} received")
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setGroup(GROUP_ENDED)
            .setAutoCancel(true)
            .setTimeoutAfter(ENDED_MS)
            .setSilent(quiet)
            .setContentIntent(openApp(sale.orderId))
            .build(),
    )

    /** A sale that ended without its money, at a till on [till]: open it to refund the point of sale's sale. */
    fun unpaid(sale: TillSale, till: PointOfSale?, quiet: Boolean) = post(
        sale,
        NotificationCompat.Builder(context, CHANNEL_SALES)
            .setSmallIcon(R.drawable.ic_mark)
            .setColor(accent)
            .setContentTitle("${SaleText.status(sale, till)} · ${SaleText.title(sale)}")
            .setContentText(
                "${SaleText.amount(sale)}. " + if (sale.found != null) {
                    PosText.refundItIn(till).replaceFirstChar(Char::uppercase) + " if the customer isn't paying."
                } else {
                    PosText.refundOnOtherTill(till, "the till that took it")
                },
            )
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setGroup(GROUP_ENDED)
            .setAutoCancel(true)
            .setSilent(quiet)
            .setContentIntent(openApp(sale.orderId))
            .build(),
    )

    /** Posts [notification] as the shift's one ongoing notification. */
    fun ongoing(notification: Notification) = notify(ONGOING_ID, notification)

    /**
     * Posts a sale's own "Paid" or "No payment yet" in the ended group,
     * and, once there are two, the group's summary: left to itself Android
     * bundles them under the launcher icon, a solid square in the status
     * bar. The summary goes when the Paid ones would, so it never outlives
     * them for long; an unpaid one left after that stands alone.
     */
    private fun post(sale: TillSale, notification: Notification) {
        val id = idOf(sale)
        notify(id, notification)
        // The system posts in its own time, so the one just sent may not be listed yet: count the others and add it.
        val others = runCatching {
            context.getSystemService(NotificationManager::class.java).activeNotifications
                .count { it.notification.group == GROUP_ENDED && it.id != SUMMARY_ID && it.id != id }
        }.getOrDefault(0)
        val ended = others + 1
        if (ended >= 2) notify(SUMMARY_ID, summary(ended))
    }

    private fun summary(ended: Int): Notification = NotificationCompat.Builder(context, CHANNEL_SALES)
        .setSmallIcon(R.drawable.ic_mark)
        .setColor(accent)
        .setContentTitle("$ended sales ended")
        .setGroup(GROUP_ENDED)
        .setGroupSummary(true)
        .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
        .setSilent(true)
        .setAutoCancel(true)
        .setTimeoutAfter(ENDED_MS)
        .setContentIntent(openShift())
        .build()

    /** Android 13+ asks the cashier first; without it the till runs silently. */
    private fun notify(id: Int, notification: Notification) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        runCatching { NotificationManagerCompat.from(context).notify(id, notification) }
    }

    /** Opens Till, on [orderId]'s sale when there is one. */
    fun openApp(orderId: String?): PendingIntent = PendingIntent.getActivity(
        context,
        orderId?.hashCode() ?: 0,
        Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .apply { if (orderId != null) putExtra(MainActivity.EXTRA_ORDER, orderId) },
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /** Opens Till on the shift, where every ended sale is listed. */
    private fun openShift(): PendingIntent = PendingIntent.getActivity(
        context,
        SUMMARY_ID,
        Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(MainActivity.EXTRA_SHIFT, true),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /** "Show code": the bubble's panel over whatever app is in front, or Till when there's no bubble. */
    fun showCode(orderId: String): PendingIntent = PendingIntent.getActivity(
        context,
        orderId.hashCode() + 1,
        Intent(context, ShowCodeActivity::class.java).putExtra(MainActivity.EXTRA_ORDER, orderId),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun copy(sale: TillSale): PendingIntent = PendingIntent.getBroadcast(
        context,
        sale.orderId.hashCode(),
        Intent(context, CopyReceiver::class.java).putExtra(CopyReceiver.EXTRA_ADDRESS, sale.address),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /** The design's paid mark: a white tick on a green circle. */
    private fun tick(): Bitmap {
        val size = (48 * context.resources.displayMetrics.density).toInt()
        val bitmap = createBitmap(size, size)
        val canvas = Canvas(bitmap)
        val unit = size / 24f
        canvas.drawCircle(12 * unit, 12 * unit, 11 * unit, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = paidGreen })
        val tick = Path().apply {
            moveTo(7 * unit, 12.5f * unit)
            lineTo(10.2f * unit, 15.7f * unit)
            lineTo(17 * unit, 8.8f * unit)
        }
        canvas.drawPath(
            tick,
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = 0xFFFFFFFF.toInt()
                style = Paint.Style.STROKE
                strokeWidth = 2.2f * unit
                strokeCap = Paint.Cap.ROUND
                strokeJoin = Paint.Join.ROUND
            },
        )
        return bitmap
    }

    companion object {
        const val ONGOING_ID = 1
        private const val SUMMARY_ID = 2
        private const val GROUP_ENDED = "ended"
        const val CHANNEL_SHIFT = "shift"
        const val CHANNEL_SALES = "sales"

        /** How long a "Paid" notification stays. */
        const val ENDED_MS = 5 * 60 * 1000L

        private fun idOf(sale: TillSale) = 1000 + (sale.orderId.hashCode() and 0x7FFFFF)

        /**
         * Copies [address] (a sale's, for the customer, or the store wallet's
         * from Settings), saying so where Android doesn't (before 13). Not
         * marked sensitive: a receiving address is for sharing, and Android's
         * preview lets the cashier check what was copied.
         */
        fun copyAddress(context: Context, address: String) {
            val clipboard = context.getSystemService(ClipboardManager::class.java)
            clipboard.setPrimaryClip(ClipData.newPlainText("Monero address", address))
            if (Build.VERSION.SDK_INT < 33) Toast.makeText(context, "Address copied", Toast.LENGTH_SHORT).show()
        }

        /** Copies a sale's exact XMR amount (the panel's XMR line), for a wallet that wants it typed. */
        fun copyXmr(context: Context, xmr: String) {
            val clipboard = context.getSystemService(ClipboardManager::class.java)
            clipboard.setPrimaryClip(ClipData.newPlainText("XMR amount", xmr))
            if (Build.VERSION.SDK_INT < 33) Toast.makeText(context, "Amount copied", Toast.LENGTH_SHORT).show()
        }
    }
}

/** The notification's "Copy address". */
class CopyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val address = intent.getStringExtra(EXTRA_ADDRESS) ?: return
        SaleNotifications.copyAddress(context, address)
    }

    companion object {
        const val EXTRA_ADDRESS = "address"
    }
}
