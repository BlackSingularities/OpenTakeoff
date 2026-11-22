package com.freemetar.app.data

import com.freemetar.app.domain.AirportLayout
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.FormBody
import okhttp3.Request
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlin.math.cos
import kotlin.math.max

/**
 * Mapy lotnisk z OpenStreetMap (Overpass API, bez klucza): pobierane raz dla lotniska, gdy mapa jest blisko
 * niego, i trzymane na dysku przez 30 dni (potem odświeżane; bez sieci zostaje stara kopia).
 * Naraz jedno pobranie — publiczne serwery Overpass mają limity.
 */
object AirportLayouts {
    private val _layouts = MutableStateFlow<Map<String, AirportLayout>>(emptyMap())
    val layouts = _layouts.asStateFlow()

    private val pending = ConcurrentHashMap.newKeySet<String>()
    private val failedAt = ConcurrentHashMap<String, Long>()
    private val net = Mutex()
    private const val TTL_MS = 30L * 24 * 3600 * 1000
    private const val RETRY_MS = 5L * 60 * 1000
    private const val KEEP = 6
    private val ENDPOINTS = listOf(
        "https://overpass-api.de/api/interpreter",
        "https://overpass.private.coffee/api/interpreter",
    )

    /** Prostokąt (s, w, n, e) obejmujący pasy lotniska z zapasem na płyty i terminale. */
    private fun bbox(a: Airport, runways: List<Runway>): DoubleArray {
        var s = a.lat; var n = a.lat; var w = a.lon; var e = a.lon
        for (r in runways) for (end in listOf(r.le, r.he)) {
            val la = end.lat ?: continue; val lo = end.lon ?: continue
            s = minOf(s, la); n = maxOf(n, la); w = minOf(w, lo); e = maxOf(e, lo)
        }
        val padLat = 1200.0 / 111_320
        val padLon = 1200.0 / (111_320 * max(0.05, cos(Math.toRadians(a.lat))))
        val minLat = 1500.0 / 111_320
        val minLon = 1500.0 / (111_320 * max(0.05, cos(Math.toRadians(a.lat))))
        s = minOf(s - padLat, a.lat - minLat); n = maxOf(n + padLat, a.lat + minLat)
        w = minOf(w - padLon, a.lon - minLon); e = maxOf(e + padLon, a.lon + minLon)
        return doubleArrayOf(s, w, n, e)
    }

    private fun download(q: String): String {
        var last: Exception? = null
        for (url in ENDPOINTS) {
            try {
                val req = Request.Builder().url(url)
                    .header("User-Agent", "OpenTakeoff-Android")
                    .post(FormBody.Builder().add("data", q).build())
                    .build()
                Http.client.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) throw IOException("Overpass HTTP ${resp.code}")
                    val body = resp.body?.string().orEmpty()
                    if (!body.trimStart().startsWith("{")) throw IOException("Overpass: brak JSON")
                    return body
                }
            } catch (e: Exception) {
                last = e
            }
        }
        throw last ?: IOException("Overpass")
    }

    /** Wczytuje (z dysku albo z sieci) mapę lotniska, jeśli jeszcze jej nie ma w pamięci. */
    fun ensure(scope: CoroutineScope, dir: File, a: Airport, runways: List<Runway>) {
        val id = a.ident
        if (_layouts.value.containsKey(id)) return
        val failed = failedAt[id]
        if (failed != null && System.currentTimeMillis() - failed < RETRY_MS) return
        if (!pending.add(id)) return
        scope.launch(Dispatchers.IO) {
            try {
                dir.mkdirs()
                val f = File(dir, "$id.json")
                val fresh = f.exists() && System.currentTimeMillis() - f.lastModified() < TTL_MS
                val text = if (fresh) f.readText() else {
                    val got = runCatching {
                        net.withLock {
                            val b = bbox(a, runways)
                            download(AirportLayout.query(b[0], b[1], b[2], b[3]))
                        }
                    }.getOrNull()
                    if (got != null) {
                        val tmp = File(dir, "$id.json.part")
                        tmp.writeText(got)
                        if (!tmp.renameTo(f)) { f.delete(); tmp.renameTo(f) }
                        got
                    } else if (f.exists()) f.readText() else null
                }
                if (text == null) { failedAt[id] = System.currentTimeMillis(); return@launch }
                val layout = AirportLayout.parse(id, text)
                _layouts.update { m ->
                    val next = LinkedHashMap(m)
                    next.remove(id)
                    next[id] = layout
                    while (next.size > KEEP) next.remove(next.keys.first())
                    next
                }
            } catch (e: Exception) {
                failedAt[id] = System.currentTimeMillis()
            } finally {
                pending.remove(id)
            }
        }
    }

    /** Czy mapa lotniska jest właśnie pobierana. */
    fun loading(): Boolean = pending.isNotEmpty()
}
