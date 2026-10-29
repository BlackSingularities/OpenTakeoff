package com.freemetar.app.data

import com.freemetar.app.domain.loc

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings as AndroidSettings
import androidx.core.content.FileProvider
import com.freemetar.app.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import okhttp3.Request
import java.io.File
import java.io.IOException

data class UpdateInfo(
    val versionName: String,
    val versionCode: Int,
    val notes: String,
    val apkUrl: String,
    val sizeBytes: Long,
    val pageUrl: String,
)

/**
 * Aktualizacje z GitHub Releases. Tag ma postać vX.Y.Z, a versionCode = X·10000 + Y·100 + Z
 * (jak w app/build.gradle.kts), więc porównujemy liczby, nie napisy.
 */
object Updater {
    const val REPO = "BlackSingularities/OpenTakeoff"
    private val json = Json { ignoreUnknownKeys = true }

    suspend fun check(): UpdateInfo? {
        val body = Http.get("https://redzikowski.com/api/github/repos/$REPO/releases/latest")
        if (body.isBlank()) return null
        val o = json.parseToJsonElement(body).jsonObject
        fun s(k: String) = (o[k] as? JsonPrimitive)?.contentOrNull.orEmpty()
        val tag = s("tag_name").removePrefix("v")
        val parts = tag.split('.').map { it.toIntOrNull() ?: return null }
        if (parts.size != 3) return null
        val code = parts[0] * 10000 + parts[1] * 100 + parts[2]
        if (code <= BuildConfig.VERSION_CODE) return null
        val apk = o["assets"]?.jsonArray?.map { it.jsonObject }
            ?.firstOrNull { (it["name"] as? JsonPrimitive)?.contentOrNull?.endsWith(".apk") == true } ?: return null
        return UpdateInfo(
            versionName = tag,
            versionCode = code,
            notes = s("body"),
            apkUrl = (apk["browser_download_url"] as JsonPrimitive).content,
            sizeBytes = (apk["size"] as? JsonPrimitive)?.longOrNull ?: 0L,
            pageUrl = s("html_url"),
        )
    }

    /** Pobiera APK do cache aplikacji, raportując postęp 0..1. */
    suspend fun download(context: Context, info: UpdateInfo, onProgress: (Float) -> Unit): File = withContext(Dispatchers.IO) {
        val dir = File(context.cacheDir, "updates").apply { mkdirs() }
        dir.listFiles()?.forEach { it.delete() }
        val target = File(dir, "OpenTakeoff-${info.versionName}.apk")
        val req = Request.Builder().url(info.apkUrl).build()
        Http.client.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("HTTP ${resp.code}")
            val body = resp.body ?: throw IOException(loc("Pusta odpowiedź", "Empty response"))
            val total = body.contentLength().takeIf { it > 0 } ?: info.sizeBytes
            body.byteStream().use { input ->
                target.outputStream().use { out ->
                    val buf = ByteArray(64 * 1024)
                    var done = 0L
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        done += n
                        if (total > 0) onProgress((done.toFloat() / total).coerceIn(0f, 1f))
                    }
                }
            }
        }
        target
    }

    fun canInstall(context: Context) = context.packageManager.canRequestPackageInstalls()

    /** Otwiera ustawienia „Instaluj nieznane aplikacje” dla OpenTakeoff. */
    fun openInstallPermission(context: Context) {
        context.startActivity(
            Intent(AndroidSettings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }

    /** Przekazuje APK do systemowego instalatora. */
    fun install(context: Context, apk: File) {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.updates", apk)
        context.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }
}
