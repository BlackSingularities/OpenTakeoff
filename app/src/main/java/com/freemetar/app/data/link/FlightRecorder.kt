package com.freemetar.app.data.link

import android.content.Context
import com.freemetar.app.domain.Geo
import com.freemetar.app.domain.LandingReport
import com.freemetar.app.domain.LandingScore
import com.freemetar.app.domain.RunwayGeom
import com.freemetar.app.domain.RunwayMath
import com.freemetar.app.domain.TouchdownData
import com.freemetar.app.domain.Wind
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import java.io.File
import java.util.UUID

@Serializable
data class TrackPoint(
    val t: Long, val lat: Double, val lon: Double, val altFt: Int, val gsKt: Int,
    val vsFpm: Int? = null, val iasKt: Int? = null, val hdg: Int? = null,
    /** Przechylenie i pochylenie (°, dokładność 0,1), wysokość nad terenem, przeciążenie, na ziemi — do analizy pilotażu. */
    val bankDeg: Float? = null, val pitchDeg: Float? = null, val aglFt: Int? = null, val gLoad: Float? = null, val onGround: Boolean? = null,
)

/** Znacznik na osi czasu lotu: ręczny (z kokpitu lub odtwarzania) z opisem. */
@Serializable
data class FlightMarker(val t: Long, val label: String)

/** Zapisany lot z symulatora: tor, lotniska, czasy. */
@Serializable
data class FlightLog(
    val id: String = UUID.randomUUID().toString(),
    val start: Long,
    val end: Long,
    val aircraft: String?,
    val from: String?,
    val to: String?,
    val maxAltFt: Int,
    val distanceNm: Int,
    val track: List<TrackPoint>,
    /** Ostatnie lądowanie lotu (zgodność ze starszymi wpisami i szybki podgląd). */
    val landing: LandingReport? = null,
    val category: String? = null,
    /** Wszystkie lądowania w kolejności (touch and go, międzylądowania); puste we wpisach ze starszych wersji. */
    val landings: List<LandingReport> = emptyList(),
    /** Ręczne znaczniki (w kolejności czasu). */
    val markers: List<FlightMarker> = emptyList(),
    /** Ćwiczenie, z którym porównywany jest lot (id z [com.freemetar.app.domain.Scenario]). */
    val scenario: String? = null,
) {
    val durationMin get() = ((end - start) / 60_000).toInt()

    /** Lądowania do statystyk: lista, a dla starszych wpisów pojedyncze [landing]. */
    val allLandings: List<LandingReport> get() = landings.ifEmpty { listOfNotNull(landing) }
}

@Serializable
private data class FlightLogFile(val app: String = "OpenTakeoff", val version: Int = 1, val flights: List<FlightLog>)

/**
 * Rejestrator lotu oparty wyłącznie na kolejnych stanach — bez zależności od Androida, więc testowalny.
 * Start: w powietrzu i GS > 40 kt. Koniec: na ziemi i GS < 5 kt przez 60 s (albo [finish] po rozłączeniu).
 */
class FlightRecorder(
    /** Pasy w pobliżu punktu przyziemienia (z bazy MSFS lub OurAirports). */
    private val runwaysNear: (Double, Double) -> List<RunwayGeom> = { _, _ -> emptyList() },
    private val nearestAirport: (Double, Double) -> String?,
) {
    /** Odstęp między punktami toru (ms) — z ustawień, 250 ms…10 s. */
    var intervalMs: Long = 1_000

    private val points = ArrayList<TrackPoint>()

    private fun point(s: FlightState, now: Long, lat: Double, lon: Double, alt: Int, gs: Double) = TrackPoint(
        now, lat, lon, alt, gs.toInt(), s.speed.vsFpm?.toInt(), s.speed.iasKt?.toInt(),
        (s.attitude.hdgTrueDeg ?: s.attitude.trackTrueDeg)?.let { ((it % 360 + 360) % 360).toInt() },
        s.attitude.bankDeg?.toFloat(), s.attitude.pitchDeg?.toFloat(),
        (s.avionics?.radioAltFt ?: s.position.altAglFt)?.toInt(), s.avionics?.gForce?.toFloat(), s.aircraft.onGround,
    )
    private val markers = ArrayList<FlightMarker>()

    /** Ręczny znacznik w bieżącym nagraniu; false, gdy nic się nie nagrywa. */
    fun mark(label: String, now: Long = System.currentTimeMillis()): Boolean {
        if (!recording) return false
        markers += FlightMarker(now, label)
        return true
    }
    private var aircraft: String? = null
    private var category: String? = null
    private var stoppedSince: Long? = null

    // Przyziemienia: pierwsze po locie to lądowanie, kolejne w ciągu 15 s to odbicia.
    // Po ponownym wzniesieniu (touch and go, międzylądowanie) następne przyziemienie to nowe lądowanie.
    private var lastTdSeq: Long? = null
    private val landings = ArrayList<LandingReport>()
    private var landing: TouchdownData? = null
    private var landingAirport: String? = null
    private var landingAt = 0L
    private var bounces = 0
    private var prev: FlightState? = null
    private var lastAt = 0L
    private var recordingId = UUID.randomUUID().toString()
    private var airborneSince: Long? = null

    /** Paliwo na początku nagrania (kg) — do zużycia w panelu postępu. */
    var startFuelKg: Double? = null
        private set

    /** Ostatnia ocena lądowania (do pokazania od razu po przyziemieniu). */
    var lastLanding: LandingReport? = null
        private set
    var recording = false
        private set

    val currentTrack: List<TrackPoint> get() = points.toList()
    val pointCount: Int get() = points.size

    /** Przyjmuje kolejny stan; zwraca zakończony lot, gdy samolot stanął po lądowaniu. */
    fun onState(s: FlightState, now: Long = System.currentTimeMillis()): FlightLog? {
        val lat = s.position.lat ?: return null
        val lon = s.position.lon ?: return null
        if (s.aircraft.paused == true) return null
        val gs = s.speed.gsKt ?: 0.0
        val onGround = s.aircraft.onGround == true
        val alt = (s.position.altMslFt ?: 0.0).toInt()

        if (!recording) {
            // Po ręcznym zakończeniu nie zaczynaj od razu nowego nagrania w powietrzu — dopiero po wylądowaniu
            if (holdUntilGround) { if (onGround) holdUntilGround = false; prev = s; lastAt = now; return null }
            if (!onGround && gs > 40) {
                recording = true
                recordingId = UUID.randomUUID().toString()
                points.clear()
                aircraft = s.aircraft.title
                category = s.aircraft.category
                stoppedSince = null
                landing = null; bounces = 0; lastLanding = null; landings.clear(); markers.clear()
                airborneSince = now
                startFuelKg = s.fuel.totalKg
                lastTdSeq = s.touchdown?.seq
                points += point(s, now, lat, lon, alt, gs)
            }
            prev = s
            lastAt = now
            return null
        }
        // Restart lotu w symulatorze (nowy lot, powrót na start, zmiana samolotu): zamknij bieżący
        // i zacznij od nowa. Skok pozycji większy niż możliwy przelot od poprzedniego stanu = teleport.
        val p = prev
        val pLat = p?.position?.lat; val pLon = p?.position?.lon
        if (pLat != null && pLon != null) {
            val dtH = (now - lastAt).coerceAtLeast(1) / 3_600_000.0
            val possibleNm = (maxOf(gs, p.speed.gsKt ?: 0.0) + 100) * dtH * 1.5 + 1.0
            val jumped = Geo.distanceNm(pLat, pLon, lat, lon) > possibleNm
            val otherAircraft = aircraft != null && s.aircraft.title != null && s.aircraft.title != aircraft
            if (jumped || otherAircraft) {
                val done = finish(lastAt)
                prev = null
                val next = onState(s, now)   // nowy stan od zera: może od razu zacząć nowe nagranie
                return done ?: next
            }
        }
        if (!onGround && airborneSince == null) airborneSince = now
        detectTouchdown(s, now)
        prev = s
        lastAt = now
        val last = points.last()
        // Punkt co [intervalMs] (ustawienie); archiwum trzyma tor w kolumnach różnic spakowanych gzipem
        if (now - last.t >= intervalMs) {
            points += point(s, now, lat, lon, alt, gs)
        }
        if (onGround && gs < 5) {
            val since = stoppedSince ?: now.also { stoppedSince = it }
            if (now - since >= 60_000) return finish(now)
        } else {
            stoppedSince = null
        }
        return null
    }

    private fun detectTouchdown(s: FlightState, now: Long) {
        val td = s.touchdown
        val fromLink = td != null && td.seq != lastTdSeq && td.vsFpm != null
        // Bez danych z Link: przejście z powietrza na ziemię, prędkość pionowa z poprzedniego stanu
        val fromTransition = !fromLink && prev?.aircraft?.onGround == false && s.aircraft.onGround == true
        if (!fromLink && !fromTransition) return
        if (fromLink) lastTdSeq = td!!.seq
        val flownLongEnough = (airborneSince?.let { now - it } ?: 0) >= 60_000
        if (landing != null && (now - landingAt <= 15_000 || !flownLongEnough)) { bounces++; return }
        if (!flownLongEnough) return
        val vs = if (fromLink) td!!.vsFpm!! else (prev?.speed?.vsFpm ?: 0.0)
        val lat = (if (fromLink) td!!.lat else null) ?: s.position.lat ?: return
        val lon = (if (fromLink) td!!.lon else null) ?: s.position.lon ?: return
        val hdg = (if (fromLink) td!!.headingTrueDeg else null) ?: s.attitude.hdgTrueDeg
        val rw = RunwayMath.pick(runwaysNear(lat, lon), lat, lon, hdg)
        val off = rw?.let { RunwayMath.offset(it, lat, lon) }
        val cross = rw?.let { r ->
            val wd = s.environment.windDirDeg; val ws = s.environment.windSpdKt
            if (wd != null && ws != null) Wind.components(wd, ws, r.headingTrueDeg).crosswind else null
        }
        report()?.let { landings += it }
        landing = TouchdownData(
            vsFpm = vs,
            gForce = if (fromLink) td!!.gForce else null,
            bankDeg = (if (fromLink) td!!.bankDeg else null) ?: prev?.attitude?.bankDeg,
            pitchDeg = (if (fromLink) td!!.pitchDeg else null) ?: prev?.attitude?.pitchDeg,
            iasKt = (if (fromLink) td!!.iasKt else null) ?: prev?.speed?.iasKt,
            runway = rw?.ident, alongFt = off?.alongFt, crossFt = off?.crossFt, runwayLengthFt = rw?.lengthFt,
            crosswindKt = cross,
        )
        landingAt = now
        landingAirport = nearestAirport(lat, lon)
        bounces = 0
        airborneSince = null
        lastLanding = report()
    }

    private fun report(): LandingReport? = landing?.let {
        LandingScore.evaluate(it.copy(bounces = bounces)).copy(airport = landingAirport, at = landingAt)
    }

    companion object { const val MAX_POINTS = 60_000 }

    /** Wstrzymanie automatycznego startu po ręcznym zakończeniu (do najbliższego postoju na ziemi). */
    var holdUntilGround = false
        private set

    /** Czas startu bieżącego nagrania (ms) albo null. */
    val startedAt: Long? get() = if (recording) points.firstOrNull()?.t else null

    /** Zakończenie zapisu na żądanie użytkownika: zwraca lot (jeśli ≥ 2 min) i wstrzymuje nowe nagranie do lądowania. */
    fun stopByUser(now: Long = System.currentTimeMillis()): FlightLog? {
        val log = finish(now)
        holdUntilGround = true
        return log
    }

    /** Wznowienie automatycznego zapisu (nowe nagranie zacznie się przy kolejnym stanie w powietrzu). */
    fun resume() { holdUntilGround = false }

    /** Kończy nagrywanie (np. po rozłączeniu); krótkie loty (< 2 min) są odrzucane. */
    fun finish(now: Long = System.currentTimeMillis()): FlightLog? {
        if (!recording) return null
        recording = false
        val log = build(points.toList(), now)
        points.clear()
        return log
    }

    /**
     * Bieżący lot jako wpis do dziennika (bez kończenia nagrania) — do okresowego zapisu szkicu,
     * żeby lot nie przepadł, gdy system zamknie aplikację w trakcie. Ten sam id co wpis końcowy.
     */
    fun snapshot(now: Long = System.currentTimeMillis()): FlightLog? = if (recording) build(points.toList(), now) else null

    private fun build(pts: List<TrackPoint>, now: Long): FlightLog? {
        if (pts.size < 2 || now - pts.first().t < 120_000) return null
        val dist = pts.zipWithNext { a, b -> Geo.distanceNm(a.lat, a.lon, b.lat, b.lon) }.sum()
        return FlightLog(
            id = recordingId,
            start = pts.first().t, end = now, aircraft = aircraft,
            from = nearestAirport(pts.first().lat, pts.first().lon),
            to = nearestAirport(pts.last().lat, pts.last().lon),
            maxAltFt = pts.maxOf { it.altFt }, distanceNm = dist.toInt(),
            landing = report(), category = category, landings = landings + listOfNotNull(report()), markers = markers.toList(),
            // Bezpiecznik: ponad 60 tys. punktów (np. 4 Hz przez 4 h) — co n-ty punkt, końce zawsze zostają
            track = if (pts.size <= MAX_POINTS) pts else pts.filterIndexed { i, _ -> i % (pts.size / MAX_POINTS + 1) == 0 } + pts.last(),
        )
    }
}

