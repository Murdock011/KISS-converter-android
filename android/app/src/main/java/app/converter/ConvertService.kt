package app.converter

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * A foreground service that keeps the app alive while it converts,
 * so the user can leave the app. The work itself runs in the ViewModel;
 * this service only owns the progress notification.
 */
class ConvertService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            onStopRequest?.invoke()
            return START_NOT_STICKY
        }
        val n = Work.progressNotification(this, intent?.getStringExtra(EXTRA_TITLE) ?: "Converting", 0)
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0
        ServiceCompat.startForeground(this, Work.PROGRESS_ID, n, type)
        return START_NOT_STICKY
    }

    companion object {
        const val ACTION_STOP = "app.converter.STOP"
        const val EXTRA_TITLE = "title"

        /** Set by the ViewModel: called when the user taps "Stop" in the notification. */
        @Volatile
        var onStopRequest: (() -> Unit)? = null
    }
}

/** Start, update and stop the progress notification. */
object Work {
    const val PROGRESS_ID = 1
    private const val DONE_ID = 2
    private const val CHANNEL_PROGRESS = "progress"
    private const val CHANNEL_DONE = "done"

    private fun channels(ctx: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_PROGRESS, "Conversion progress", NotificationManager.IMPORTANCE_LOW),
        )
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_DONE, "Conversion done", NotificationManager.IMPORTANCE_DEFAULT),
        )
    }

    private fun openApp(ctx: Context): PendingIntent = PendingIntent.getActivity(
        ctx, 0,
        Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    fun progressNotification(ctx: Context, title: String, percent: Int): android.app.Notification {
        channels(ctx)
        val stop = PendingIntent.getService(
            ctx, 1,
            Intent(ctx, ConvertService::class.java).setAction(ConvertService.ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(ctx, CHANNEL_PROGRESS)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(title)
            .setContentText("$percent%")
            .setProgress(100, percent, false)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(openApp(ctx))
            .addAction(0, "Stop", stop)
            .build()
    }

    fun start(ctx: Context, title: String) {
        runCatching {
            ContextCompat.startForegroundService(
                ctx,
                Intent(ctx, ConvertService::class.java).putExtra(ConvertService.EXTRA_TITLE, title),
            )
        }
    }

    @SuppressLint("MissingPermission")
    fun update(ctx: Context, title: String, percent: Int) {
        if (!canNotify(ctx)) return
        runCatching {
            NotificationManagerCompat.from(ctx).notify(PROGRESS_ID, progressNotification(ctx, title, percent))
        }
    }

    @SuppressLint("MissingPermission")
    /** Stop the service. When [message] is set, show it in a "done" notification. */
    fun stop(ctx: Context, message: String?) {
        runCatching { ctx.stopService(Intent(ctx, ConvertService::class.java)) }
        NotificationManagerCompat.from(ctx).cancel(PROGRESS_ID)
        if (message == null || !canNotify(ctx)) return
        channels(ctx)
        val n = NotificationCompat.Builder(ctx, CHANNEL_DONE)
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle(message)
            .setContentText("Tap to open, share or save.")
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx))
            .build()
        runCatching { NotificationManagerCompat.from(ctx).notify(DONE_ID, n) }
    }

    private fun canNotify(ctx: Context) =
        Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
}
