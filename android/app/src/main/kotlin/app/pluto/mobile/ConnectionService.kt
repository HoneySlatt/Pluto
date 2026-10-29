package app.pluto.mobile

// Keeps Pluto running while it is in the background, so the connection to
// the server stays open and thread notifications arrive (GrapheneOS has no
// push service). Runs while the setting is on; its quiet notification is
// the one Android requires.

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

class ConnectionService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Notifications.channels(this)
        val notification = NotificationCompat.Builder(this, Notifications.CONNECTION)
            .setSmallIcon(R.drawable.ic_stat_pluto)
            .setContentTitle("Pluto watches your threads")
            .setContentText("You will hear when one finishes or needs you")
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setOngoing(true)
            .setContentIntent(Notifications.open(this, null))
            .build()
        ServiceCompat.startForeground(
            this,
            ID,
            notification,
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0,
        )
        // Started again by the system after it stopped Pluto: no screen
        // connects, so the service does, with the remembered server.
        if (intent == null) {
            val model = (application as PlutoApplication).model
            val saved = model.secret.load()
            if (saved == null) stopSelf() else model.core.connect(saved.first, saved.second, true)
        }
        return START_STICKY
    }

    companion object {
        private const val ID = 1

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, ConnectionService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ConnectionService::class.java))
        }
    }
}
