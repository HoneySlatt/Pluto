package app.pluto.mobile

// Thread notifications (the core's `notices.rs`): while Pluto is in the
// background and the setting is on, a thread that finishes, fails, or needs
// the user posts a notification that opens it. `ConnectionService` keeps
// the process, and so the connection, alive meanwhile.

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.pluto.core.ThreadNotice

object Notifications {
    const val THREADS = "threads"
    const val CONNECTION = "connection"
    // The intent extra naming the thread a notification opens.
    const val THREAD = "app.pluto.mobile.thread"

    fun channels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(THREADS, "Threads", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "A thread finished, failed, or needs your answer"
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(CONNECTION, "Connection", NotificationManager.IMPORTANCE_MIN).apply {
                description = "Shown while Pluto stays connected for notifications"
                setShowBadge(false)
            },
        )
    }

    // Whether Pluto may post (Android 13 asks).
    fun allowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    // Opens Pluto, on `thread` when given.
    fun open(context: Context, thread: String?): PendingIntent {
        val intent = Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (thread != null) intent.putExtra(THREAD, thread)
        return PendingIntent.getActivity(
            context,
            thread?.hashCode() ?: 0,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    // One notification per thread: a newer event replaces the last.
    fun post(context: Context, notices: List<ThreadNotice>) {
        if (notices.isEmpty() || !allowed(context)) return
        val manager = NotificationManagerCompat.from(context)
        for (notice in notices) {
            val notification = NotificationCompat.Builder(context, THREADS)
                .setSmallIcon(R.drawable.ic_stat_pluto)
                .setContentTitle(notice.title)
                .setContentText(notice.body)
                .setCategory(if (notice.attention) NotificationCompat.CATEGORY_REMINDER else NotificationCompat.CATEGORY_STATUS)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(open(context, notice.threadId))
                .build()
            @Suppress("MissingPermission")
            manager.notify(notice.threadId.hashCode(), notification)
        }
    }

    // Back in Pluto: what it told about is on screen now.
    fun clear(context: Context) {
        NotificationManagerCompat.from(context).cancelAll()
    }

    // Whether the system lets Pluto run without battery limits, which keeps
    // the connection through the phone's sleep.
    fun unrestricted(context: Context): Boolean =
        context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

    fun askUnrestricted(context: Context) {
        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}"))
        runCatching { context.startActivity(intent) }
            .onFailure { context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }
    }
}
