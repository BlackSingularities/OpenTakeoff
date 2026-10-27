package com.freemetar.app.data.link

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import com.freemetar.app.MainActivity
import com.freemetar.app.R
import com.freemetar.app.domain.loc

/**
 * Usługa pierwszoplanowa na czas połączenia z symulatorem: po wyjściu do ekranu głównego albo wygaszeniu
 * ekranu system nie zamraża aplikacji, Link dalej przysyła stany, a rejestrator zapisuje tor bez przerwy.
 * Trzyma blokadę procesora i Wi-Fi (bez niej telefon usypia kartę sieciową i dane przestają płynąć).
 */
class FlightLinkService : Service() {
    private var wake: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val n = notification(this, intent?.getBooleanExtra(EXTRA_RECORDING, false) == true)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        else startForeground(ID, n)
        if (wake == null) wake = (getSystemService(POWER_SERVICE) as PowerManager)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "OpenTakeoff:link").apply { setReferenceCounted(false); acquire(12 * 3_600_000L) }
        if (wifi == null) wifi = (applicationContext.getSystemService(WIFI_SERVICE) as? WifiManager)?.let { wm ->
            @Suppress("DEPRECATION")
            val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
            wm.createWifiLock(mode, "OpenTakeoff:link").apply { setReferenceCounted(false); acquire() }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        runCatching { wake?.takeIf { it.isHeld }?.release() }
        runCatching { wifi?.takeIf { it.isHeld }?.release() }
        wake = null; wifi = null
        super.onDestroy()
    }

    companion object {
        private const val ID = 4710
        private const val CHANNEL = "link"
        private const val EXTRA_RECORDING = "recording"
        private var running = false

        private fun notification(ctx: Context, recording: Boolean): Notification {
            val nm = ctx.getSystemService(NotificationManager::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && nm.getNotificationChannel(CHANNEL) == null) {
                nm.createNotificationChannel(NotificationChannel(CHANNEL, loc("Połączenie z symulatorem", "Simulator connection"), NotificationManager.IMPORTANCE_LOW))
            }
            val open = PendingIntent.getActivity(
                ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            return NotificationCompat.Builder(ctx, CHANNEL)
                .setSmallIcon(R.drawable.opentakeoff_mark_mono)
                .setContentTitle(if (recording) loc("Zapis lotu trwa", "Recording the flight") else loc("Połączono z symulatorem", "Connected to the simulator"))
                .setContentText(if (recording) loc("Tor jest zapisywany także w tle", "The track keeps recording in the background")
                    else loc("Zapis zacznie się po starcie", "Recording starts after takeoff"))
                .setOngoing(true).setOnlyAlertOnce(true).setSilent(true)
                .setContentIntent(open)
                .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
                .build()
        }

        /**
         * Uruchamia usługę albo odświeża jej powiadomienie. Uruchomienie z tła bywa zabronione (Android 12+),
         * więc gdy usługa już działa, zmienia się tylko treść powiadomienia.
         */
        fun update(ctx: Context, recording: Boolean) {
            if (running) {
                runCatching { ctx.getSystemService(NotificationManager::class.java).notify(ID, notification(ctx, recording)) }
                return
            }
            running = runCatching {
                ContextCompat.startForegroundService(ctx, Intent(ctx, FlightLinkService::class.java).putExtra(EXTRA_RECORDING, recording))
            }.isSuccess
        }

        fun stop(ctx: Context) {
            if (!running) return
            running = false
            runCatching { ctx.stopService(Intent(ctx, FlightLinkService::class.java)) }
        }
    }
}
