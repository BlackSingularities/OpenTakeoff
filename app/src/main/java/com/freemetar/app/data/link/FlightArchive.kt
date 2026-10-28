package com.freemetar.app.data.link

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.zip.GZIPInputStream
import java.util.zip.GZIPOutputStream
import kotlin.math.roundToInt
import kotlin.math.roundToLong

/**
 * Tor lotu w kolumnach: czas, pozycja i wysokość jako różnice względem poprzedniego punktu
 * (pozycja w 1e-5° ≈ 1 m), reszta wprost. Kolejne liczby są małe i powtarzalne,
 * więc gzip ściska je kilkanaście razy lepiej niż tablicę obiektów JSON.
 */
@Serializable
data class TrackColumns(
    val t0: Long = 0,
    val dt: List<Long> = emptyList(),
    val dLat: List<Long> = emptyList(),
    val dLon: List<Long> = emptyList(),
    val dAlt: List<Int> = emptyList(),
    val gs: List<Int> = emptyList(),
    val vs: List<Int?> = emptyList(),
    val ias: List<Int?> = emptyList(),
    val hdg: List<Int?> = emptyList(),
    /** Przechylenie i pochylenie w dziesiątych stopnia, przeciążenie w setnych g, na ziemi 1/0. */
    val bank: List<Int?> = emptyList(),
    val pitch: List<Int?> = emptyList(),
    val agl: List<Int?> = emptyList(),
    val g: List<Int?> = emptyList(),
    val gnd: List<Int?> = emptyList(),
)

@Serializable
private data class StoredFlight(val meta: FlightLog, val cols: TrackColumns)

@Serializable
private data class ArchiveFile(val app: String = "OpenTakeoff", val format: String = "flights", val version: Int = 2, val flights: List<StoredFlight>)

/** Wcześniejszy zapis (flights.json, wersja 1): tor jako lista obiektów. */
@Serializable
private data class LegacyFile(val flights: List<FlightLog> = emptyList())

/** Kodowanie archiwum lotów — bez Androida, więc testowalne na JVM. */
object FlightArchive {
    private const val SCALE = 100_000.0

    fun columns(track: List<TrackPoint>): TrackColumns {
        if (track.isEmpty()) return TrackColumns()
        val dt = ArrayList<Long>(track.size); val dLat = ArrayList<Long>(track.size); val dLon = ArrayList<Long>(track.size)
        val dAlt = ArrayList<Int>(track.size)
        var pt = track.first().t; var pLat = 0L; var pLon = 0L; var pAlt = 0
        track.forEach { p ->
            val la = (p.lat * SCALE).roundToLong(); val lo = (p.lon * SCALE).roundToLong()
            dt += p.t - pt; dLat += la - pLat; dLon += lo - pLon; dAlt += p.altFt - pAlt
            pt = p.t; pLat = la; pLon = lo; pAlt = p.altFt
        }
        // Kolumny analizy tylko, gdy lot je ma — starsze wpisy nie rosną o listy pełne null
        fun <T> opt(f: (TrackPoint) -> T?): List<T?> = if (track.any { f(it) != null }) track.map(f) else emptyList()
        return TrackColumns(
            track.first().t, dt, dLat, dLon, dAlt, track.map { it.gsKt }, track.map { it.vsFpm }, track.map { it.iasKt }, track.map { it.hdg },
            bank = opt { p -> p.bankDeg?.let { (it * 10).roundToInt() } }, pitch = opt { p -> p.pitchDeg?.let { (it * 10).roundToInt() } },
            agl = opt { it.aglFt }, g = opt { p -> p.gLoad?.let { (it * 100).roundToInt() } }, gnd = opt { p -> p.onGround?.let { if (it) 1 else 0 } },
        )
    }

    fun track(c: TrackColumns): List<TrackPoint> {
        var t = c.t0; var la = 0L; var lo = 0L; var alt = 0
        return c.dt.indices.map { i ->
            t += c.dt[i]; la += c.dLat[i]; lo += c.dLon[i]; alt += c.dAlt[i]
            TrackPoint(
                t, la / SCALE, lo / SCALE, alt, c.gs.getOrElse(i) { 0 }, c.vs.getOrNull(i), c.ias.getOrNull(i), c.hdg.getOrNull(i),
                c.bank.getOrNull(i)?.let { it / 10f }, c.pitch.getOrNull(i)?.let { it / 10f }, c.agl.getOrNull(i),
                c.g.getOrNull(i)?.let { it / 100f }, c.gnd.getOrNull(i)?.let { it == 1 },
            )
        }
    }

    fun encode(flights: List<FlightLog>): ByteArray {
        val file = ArchiveFile(flights = flights.map { StoredFlight(it.copy(track = emptyList()), columns(it.track)) })
        val bytes = ByteArrayOutputStream()
        GZIPOutputStream(bytes).bufferedWriter().use { it.write(LinkJson.encodeToString(ArchiveFile.serializer(), file)) }
        return bytes.toByteArray()
    }

    /** Czyta archiwum (gzip) albo zwykły JSON — nowy lub stary format. Zły plik → wyjątek. */
    fun decode(bytes: ByteArray): List<FlightLog> {
        val gz = bytes.size > 2 && bytes[0] == 0x1f.toByte() && bytes[1] == 0x8b.toByte()
        val text = if (gz) GZIPInputStream(ByteArrayInputStream(bytes)).bufferedReader().use { it.readText() } else bytes.decodeToString()
        return if (text.contains("\"cols\"")) {
            LinkJson.decodeFromString(ArchiveFile.serializer(), text).flights.map { it.meta.copy(track = track(it.cols)) }
        } else {
            LinkJson.decodeFromString(LegacyFile.serializer(), text).flights
        }
    }

    /** Łączy listy po id (import nie dubluje lotów); najnowsze na górze. */
    fun merge(current: List<FlightLog>, incoming: List<FlightLog>): List<FlightLog> =
        (current + incoming.filter { n -> current.none { it.id == n.id } }).sortedByDescending { it.start }

    /** Przybliżona liczba punktów na sekundę dla odstępu w ms — do opisu w ustawieniach. */
    fun rateLabel(ms: Int): String = if (ms < 1000) "${(1000.0 / ms).roundToInt()}/s" else "1/${ms / 1000}s"
}

/** Historia lotów w pliku flights.oab (gzip, dane aplikacji — przetrwa aktualizację). */
class FlightLogStore(private val context: Context) {
    private val file = File(context.filesDir, "flights.oab")
    private val legacy = File(context.filesDir, "flights.json")

    suspend fun load(): List<FlightLog> = withContext(Dispatchers.IO) {
        val src = if (file.exists()) file else legacy
        if (!src.exists()) emptyList() else runCatching { FlightArchive.decode(src.readBytes()) }.getOrDefault(emptyList())
    }

    suspend fun save(list: List<FlightLog>) = withContext(Dispatchers.IO) {
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeBytes(FlightArchive.encode(list))
        if (!tmp.renameTo(file)) { file.delete(); tmp.renameTo(file) }
        // Po pierwszym zapisie w nowym formacie stary plik nie jest już potrzebny
        if (legacy.exists()) legacy.delete()
    }

    private val draft = File(context.filesDir, "flight-in-progress.oab")

    /** Szkic trwającego lotu (nadpisywany co kilkadziesiąt sekund). */
    suspend fun saveDraft(f: FlightLog) = withContext(Dispatchers.IO) {
        val tmp = File(draft.parentFile, draft.name + ".tmp")
        tmp.writeBytes(FlightArchive.encode(listOf(f)))
        if (!tmp.renameTo(draft)) { draft.delete(); tmp.renameTo(draft) }
    }

    /** Szkic po poprzednim uruchomieniu (aplikacja zamknięta w trakcie lotu) albo null. */
    suspend fun takeDraft(): FlightLog? = withContext(Dispatchers.IO) {
        if (!draft.exists()) null else runCatching { FlightArchive.decode(draft.readBytes()).firstOrNull() }.getOrNull()
    }

    suspend fun clearDraft() = withContext(Dispatchers.IO) { draft.delete(); Unit }

    /** Rozmiar archiwum na dysku (bajty). */
    fun sizeBytes(): Long = if (file.exists()) file.length() else if (legacy.exists()) legacy.length() else 0
}
