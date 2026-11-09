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
import com.monokulo.till.ui.Fixed
import com.monokulo.till.ui.qrBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import com.monokulo.till.engine.EngineStore
import uniffi.till_app.Action
import uniffi.till_app.CopyKind
import uniffi.till_app.endedSummary
import uniffi.till_screens.EndedSummary
import uniffi.till_screens.Face
import uniffi.till_screens.FaceIcon
import uniffi.till_screens.FaceTint
import uniffi.till_screens.Notice
import uniffi.till_screens.NoticeAction
import uniffi.till_screens.OngoingFace

/**
 * How Android lays out the engine's faces (`till_screens::notification`):
 * the drawable and colour for a face's icon and tint, the channel, and the
 * private text. Pure, so it's tested on the JVM.
 */
object Shade {
    /** The status bar chip: the ring filling a quarter a step, or the tick once paid; the mark with no sale. */
    @DrawableRes
    fun icon(icon: FaceIcon): Int = when (icon) {
        FaceIcon.MARK -> R.drawable.ic_mark
        FaceIcon.RING_QUARTER -> R.drawable.ic_mark_q1
        FaceIcon.RING_HALF -> R.drawable.ic_mark_q2
        FaceIcon.RING_THREE_QUARTERS -> R.drawable.ic_mark_q3
        FaceIcon.TICK -> R.drawable.ic_tick
    }

    @ColorInt
    fun color(tint: FaceTint): Int = when (tint) {
        FaceTint.ACCENT -> Fixed.Accent.toArgb()
        FaceTint.PAID -> Fixed.Paid.toArgb()
    }

    /** The items go on a line of their own: ProgressStyle takes the style a second line would need. */
    fun text(face: Face): String = listOfNotNull(face.text, face.detail).joinToString("\n")

    /**
     * A sale the customer still has to scan, or one alerting, is on the
     * high-importance channel; once the payment is seen it moves to the
     * quiet one, and only alerts once either way.
     */
    fun channel(face: Face, alert: Boolean): String =
        if (alert || face.fullScreen) SaleNotifications.CHANNEL_SALES else SaleNotifications.CHANNEL_SHIFT
}

/**
 * The till's notifications (`docs/design.md` §9 "notifications"; the
 * design's Shade and Chips boards), drawn from the engine's faces: their
 * words and grouping are the engine's, the channels, intents and styles
 * Android's. One ongoing notification for the shift ([OngoingFace]), which
 * becomes the current sale with its progress and is promoted to a Live
 * Update on Android 16+, its status bar chip the small icon alone; a green
 * tick for a few seconds once the sale is paid; then a short notice of its
 * own ([NotificationFace]), not promoted, for each sale that ends.
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

    /** The shift's notification with no sale: what's being watched ([OngoingFace.Watching]'s words). */
    fun watching(title: String = context.getString(R.string.watching_title), text: String = ""): Notification =
        NotificationCompat.Builder(context, CHANNEL_SHIFT)
            .setSmallIcon(R.drawable.ic_mark)
            .setColor(accent)
            .setContentTitle(title)
            .setContentText(text)
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setContentIntent(openApp(null))
            .build()

    /**
     * The shift's notification as [face] says: a sale's payment code is
     * drawn as the large icon, and its address is what "Copy address"
     * copies. A sale's `alert`, set in
     * the one update as it arrives while Till is away, makes it a heads-up;
     * a post without it only alerts once. The code is encoded off the main thread: with
     * a receipt in it that's a version-25 code at four pixels a module,
     * about 880 kB of pixels.
     */
    suspend fun ongoing(face: OngoingFace): Notification = when (face) {
        is OngoingFace.Watching -> watching(face.title, face.text)
        is OngoingFace.Paid -> live(face.face, face.subText, CHANNEL_SHIFT)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setLargeIcon(tick())
            .setContentIntent(openApp(face.orderId))
            .build()
        is OngoingFace.Sale -> {
            val heads = face.alert
            val showCode = showCode(face.orderId)
            val icon = withContext(Dispatchers.Default) { runCatching { qrBitmap(face.code, scale = 4, margin = 2) }.getOrNull() }
            val builder = live(face.face, face.subText, Shade.channel(face.face, heads))
                .setOnlyAlertOnce(!heads)
                .setLargeIcon(icon)
                .setContentIntent(showCode)
            for (button in face.buttons) {
                val intent = when (button.action) {
                    NoticeAction.SHOW_CODE -> showCode
                    NoticeAction.FULL_SCREEN -> openApp(face.orderId)
                    NoticeAction.COPY_ADDRESS -> copy(face.orderId)
                }
                builder.addAction(0, button.label, intent)
            }
            builder.build()
        }
    }

    /**
     * A Live Update showing [face]: promoted on Android 16+, where its
     * status bar chip is the small icon alone. Nothing the chip would
     * print is set: no short critical text, and no time (Android shows a
     * chronometer or a future `when` as the chip's text), so the code's
     * countdown is on the panel and the sale screen, not here. The
     * private version names what was bought; the public one, on the lock
     * screen, never does.
     */
    private fun live(face: Face, subText: String, channel: String): NotificationCompat.Builder {
        val color = Shade.color(face.tint)
        val icon = Shade.icon(face.icon)
        val style = NotificationCompat.ProgressStyle()
            .setProgressSegments(List(4) { NotificationCompat.ProgressStyle.Segment(1).setColor(color) })
            .setStyledByProgress(true)
            .setProgress(face.step.toInt())
        val public = NotificationCompat.Builder(context, channel)
            .setSmallIcon(icon)
            .setColor(color)
            .setContentTitle(face.title)
            .setContentText(face.publicText)
            .setShowWhen(false)
            .build()
        return NotificationCompat.Builder(context, channel)
            .setSmallIcon(icon)
            .setColor(color)
            .setContentTitle(face.title)
            .setContentText(Shade.text(face))
            .setSubText(subText)
            .setShowWhen(false)
            .setStyle(style)
            .setOngoing(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(public)
            .setRequestPromotedOngoing(true)
    }

    /** Posts [notification] as the shift's one ongoing notification. */
    fun ongoing(notification: Notification) = notify(ONGOING_ID, notification)

    /**
     * Posts [orderId]'s own [notice] ([uniffi.till_app.Effect.Notify]) in the
     * ended group: a [paid] one with the green tick; gone after [timeoutMs]
     * when the engine sets one; [quiet] (no banner) while Till is on screen
     * saying so. The group's summary goes up when the engine says, for the
     * count showing ([endedSummary], docs/decisions/core.md 106).
     */
    fun notice(orderId: String, notice: Notice, paid: Boolean, timeoutMs: ULong?, quiet: Boolean) {
        val builder = NotificationCompat.Builder(context, CHANNEL_SALES)
            .setSmallIcon(R.drawable.ic_mark)
            .setColor(if (paid) paidGreen else accent)
            .setContentTitle(notice.title)
            .setContentText(notice.text)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setGroup(GROUP_ENDED)
            .setAutoCancel(true)
            .setSilent(quiet)
            .setContentIntent(openApp(orderId))
        if (paid) builder.setLargeIcon(tick())
        timeoutMs?.let { builder.setTimeoutAfter(it.toLong()) }
        val id = idOf(orderId)
        notify(id, builder.build())
        // The system posts in its own time, so the one just sent may not be listed yet: count the others and add it.
        val others = runCatching {
            context.getSystemService(NotificationManager::class.java).activeNotifications
                .count { it.notification.group == GROUP_ENDED && it.id != SUMMARY_ID && it.id != id }
        }.getOrDefault(0)
        endedSummary((others + 1).toUInt())?.let { notify(SUMMARY_ID, summary(it)) }
    }

    /** The sale [orderId] is gone from the shift: its notice goes with it. */
    fun clear(orderId: String) {
        runCatching { NotificationManagerCompat.from(context).cancel(idOf(orderId)) }
    }

    private fun summary(summary: EndedSummary): Notification = NotificationCompat.Builder(context, CHANNEL_SALES)
        .setSmallIcon(R.drawable.ic_mark)
        .setColor(accent)
        .setContentTitle(summary.notice.title)
        .setGroup(GROUP_ENDED)
        .setGroupSummary(true)
        .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
        .setSilent(true)
        .setAutoCancel(true)
        .setTimeoutAfter(summary.timeoutMs.toLong())
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

    /** "Copy address": the engine copies [orderId]'s address, as it does from the app and the panel. */
    private fun copy(orderId: String): PendingIntent = PendingIntent.getBroadcast(
        context,
        orderId.hashCode(),
        Intent(context, CopyReceiver::class.java)
            .putExtra(CopyReceiver.EXTRA_ORDER, orderId),
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

        private fun idOf(orderId: String) = 1000 + (orderId.hashCode() and 0x7FFFFF)

        /**
         * The engine's [uniffi.till_app.Effect.CopyText], named by what it
         * copies ([kind]): an address, an XMR amount (the panel's XMR line,
         * for a wallet that wants it typed), or anything else; said so where
         * Android doesn't (before 13). Not marked sensitive: a receiving
         * address is for sharing, and Android's preview lets the cashier
         * check what was copied.
         */
        fun copyText(context: Context, text: String, kind: CopyKind) {
            val (label, said) = when (kind) {
                CopyKind.ADDRESS -> "Monero address" to "Address copied"
                CopyKind.AMOUNT -> "XMR amount" to "Amount copied"
                CopyKind.OTHER -> "Till" to "Copied"
            }
            val clipboard = context.getSystemService(ClipboardManager::class.java)
            clipboard.setPrimaryClip(ClipData.newPlainText(label, text))
            // Android 13 and later show their own word for a copy.
            if (Build.VERSION.SDK_INT < 33) Toast.makeText(context, said, Toast.LENGTH_SHORT).show()
        }
    }
}

/**
 * The notification's "Copy address": the engine is asked ([Action.CopyAddress]) and answers with the copy
 * ([uniffi.till_app.Effect.CopyText]), which [com.monokulo.till.TillService.follow] puts on the clipboard
 * (docs/decisions/core.md 105).
 */
class CopyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val orderId = intent.getStringExtra(EXTRA_ORDER) ?: return
        EngineStore.send(Action.CopyAddress(orderId))
    }

    companion object {
        const val EXTRA_ORDER = "order"
    }
}
