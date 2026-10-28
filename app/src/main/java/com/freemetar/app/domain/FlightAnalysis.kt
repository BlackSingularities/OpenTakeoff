package com.freemetar.app.domain

import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.data.link.TrackPoint
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.roundToInt
import kotlin.math.sqrt

/** Rodzaj zdarzenia na osi czasu lotu. */
enum class EventKind { TAKEOFF, TOC, TOD, LANDING, MARKER, EXCEEDANCE }

data class FlightEvent(val t: Long, val kind: EventKind, val label: String)

/** Wartości na bramce podejścia (1000 / 500 ft nad przyziemieniem). */
data class Gate(val heightFt: Int, val iasKt: Int?, val vsFpm: Int?, val bankDeg: Float?, val stable: Boolean)

/**
 * Analiza podejścia do jednego lądowania: od 1500 ft nad punktem przyziemienia do przyziemienia.
 * [stableFromFt] — wysokość, od której do 50 ft wszystkie kryteria były spełnione bez przerwy.
 */
data class ApproachAnalysis(
    val landingAt: Long,
    val startAt: Long,
    val gates: List<Gate>,
    val stableFromFt: Int,
    val pathAngleDeg: Double?,
    val iasSpreadKt: Int?,
    val maxSinkFpm: Int?,
    val maxBankDeg: Float?,
    val score: Int,
    val notes: List<String>,
)

/** Płynność prowadzenia w powietrzu: średnie kwadratowe tempa zmian i liczba szarpnięć. */
data class Smoothness(
    val score: Int,
    val bankRateRms: Double?,
    val pitchRateRms: Double?,
    val vsChangeRms: Double,
    val jerks: Int,
    val maxBankDeg: Float?,
    val maxG: Float?,
    val notes: List<String>,
)

data class Criterion(val label: String, val value: String, val passed: Boolean?)

/** Ćwiczenia, z którymi można porównać lot. Kryteria liczone z toru i ocen lądowań. */
enum class Scenario(val id: String) {
    PATTERN("pattern"), TOUCH_AND_GO("touch-and-go"), CROSS_COUNTRY("cross-country"), PRECISION_APPROACH("approach");

    val title: String get() = when (this) {
        PATTERN -> loc("Krąg nadlotniskowy", "Traffic pattern")
        TOUCH_AND_GO -> loc("Seria touch and go", "Touch-and-go series")
        CROSS_COUNTRY -> loc("Przelot", "Cross-country")
        PRECISION_APPROACH -> loc("Podejście precyzyjne", "Precision approach")
    }
    val description: String get() = when (this) {
        PATTERN -> loc("Start i lądowanie na tym samym lotnisku, wysokość kręgu 600–1500 ft, do 20 min.", "Take-off and landing at the same airport, pattern height 600–1500 ft, up to 20 min.")
        TOUCH_AND_GO -> loc("Co najmniej 3 przyziemienia, każde z oceną od 60.", "At least 3 touchdowns, each scoring 60 or more.")
        CROSS_COUNTRY -> loc("Lądowanie na innym lotnisku, wznoszenie co najmniej 3000 ft, płynny pilotaż.", "Landing at another airport, climbing at least 3000 ft, smooth handling.")
        PRECISION_APPROACH -> loc("Stabilne od 1000 ft, ścieżka 2,5–3,5°, przyziemienie w strefie.", "Stable from 1000 ft, 2.5–3.5° path, touchdown in the zone.")
    }

    companion object { fun of(id: String?) = entries.firstOrNull { it.id == id } }
}

data class ScenarioResult(val scenario: Scenario, val criteria: List<Criterion>) {
    val passed get() = criteria.count { it.passed == true }
    val judged get() = criteria.count { it.passed != null }
    val percent get() = if (judged == 0) 0 else passed * 100 / judged
}

data class CompareRow(val label: String, val now: String, val before: String, val better: Boolean?)
data class Comparison(val previous: FlightLog, val sameRoute: Boolean, val rows: List<CompareRow>)

data class FlightAnalysisResult(
    val events: List<FlightEvent>,
    val approaches: List<ApproachAnalysis>,
    val smoothness: Smoothness?,
)

/** Analiza zapisanego lotu — czysta funkcja toru, testowalna na JVM. */
object FlightAnalysis {
    private const val FT_PER_NM = 6076.12

    fun analyze(f: FlightLog): FlightAnalysisResult {
        val approaches = f.allLandings.mapNotNull { l -> l.at?.let { approach(f.track, it) } }
        return FlightAnalysisResult(events(f, approaches), approaches, smoothness(f.track))
    }

    /** Punkt toru najbliższy chwili [t] (tor posortowany po czasie). */
    fun at(track: List<TrackPoint>, t: Long): TrackPoint? {
        if (track.isEmpty()) return null
        var lo = 0; var hi = track.size - 1
        while (lo < hi) { val mid = (lo + hi) / 2; if (track[mid].t < t) lo = mid + 1 else hi = mid }
        return if (lo > 0 && t - track[lo - 1].t < track[lo].t - t) track[lo - 1] else track[lo]
    }

    fun events(f: FlightLog, approaches: List<ApproachAnalysis> = emptyList()): List<FlightEvent> {
        val tr = f.track
        if (tr.size < 2) return f.markers.map { FlightEvent(it.t, EventKind.MARKER, it.label) }
        val ev = ArrayList<FlightEvent>()
        val takeoff = tr.firstOrNull { it.onGround == false } ?: tr.first()
        ev += FlightEvent(takeoff.t, EventKind.TAKEOFF, loc("Start", "Take-off") + (f.from?.let { " $it" } ?: ""))
        val base = tr.first().altFt
        if (f.maxAltFt - base >= 2000) {
            val cruise = f.maxAltFt - 300
            tr.firstOrNull { it.altFt >= cruise }?.let { ev += FlightEvent(it.t, EventKind.TOC, loc("Koniec wznoszenia", "Top of climb") + " ${altLabel(f.maxAltFt)}") }
            tr.lastOrNull { it.altFt >= cruise }?.let { ev += FlightEvent(it.t, EventKind.TOD, loc("Początek zniżania", "Top of descent")) }
        }
        f.allLandings.forEach { l ->
            l.at?.let { ev += FlightEvent(it, EventKind.LANDING, loc("Przyziemienie", "Touchdown") + " ${l.vsFpm} ft/min" + ((l.runway ?: l.airport)?.let { r -> ", $r" } ?: "")) }
        }
        approaches.filter { it.stableFromFt < 500 }.forEach {
            ev += FlightEvent(it.startAt, EventKind.EXCEEDANCE, loc("Podejście niestabilne", "Unstable approach"))
        }
        // Przekroczenia: pierwsze w serii, kolejne tego samego rodzaju dopiero po 30 s spokoju
        val lastOf = HashMap<String, Long>()
        fun flag(key: String, p: TrackPoint, label: String) {
            val last = lastOf[key]
            if (last == null || p.t - last > 30_000) ev += FlightEvent(p.t, EventKind.EXCEEDANCE, label)
            lastOf[key] = p.t
        }
        tr.forEach { p ->
            if (p.onGround == true) return@forEach
            p.bankDeg?.let { if (abs(it) > 45) flag("bank", p, loc("Przechylenie", "Bank") + " ${abs(it).roundToInt()}°") }
            p.gLoad?.let { if (it > 2.0f || it < 0f) flag("g", p, loc("Przeciążenie", "G-load") + " %.1f g".format(it)) }
            val h = p.aglFt
            if (h != null && h < 1000 && (p.vsFpm ?: 0) < -2000) flag("sink", p, loc("Duże opadanie nisko", "High sink rate low") + " ${p.vsFpm} ft/min")
            p.pitchDeg?.let { if (it > 25 || it < -20) flag("pitch", p, loc("Pochylenie", "Pitch") + " ${it.roundToInt()}°") }
        }
        f.markers.forEach { ev += FlightEvent(it.t, EventKind.MARKER, it.label) }
        return ev.sortedBy { it.t }
    }

    fun approach(track: List<TrackPoint>, landingAt: Long): ApproachAnalysis? {
        val tdIdx = track.indexOfLast { it.t <= landingAt }.takeIf { it > 0 } ?: return null
        val tdElev = track[tdIdx].altFt
        // Wstecz od przyziemienia, dopóki nad punktem przyziemienia jest najwyżej 1500 ft (i nie dalej niż 15 min)
        var startIdx = tdIdx
        while (startIdx > 0 && track[startIdx - 1].altFt - tdElev <= 1500 && landingAt - track[startIdx - 1].t <= 900_000) startIdx--
        val seg = track.subList(startIdx, tdIdx + 1)
        if (seg.size < 5) return null
        fun hat(p: TrackPoint) = p.altFt - tdElev
        val final = seg.filter { hat(it) in 50..1000 }
        if (final.size < 3) return null
        val refIas = seg.filter { hat(it) in 50..500 }.mapNotNull { it.iasKt }.sorted().let { if (it.isEmpty()) null else it[it.size / 2] }
        fun stable(p: TrackPoint): Boolean =
            (p.vsFpm ?: 0) >= -1000 &&
                (p.bankDeg?.let { abs(it) <= 7f } ?: true) &&
                (refIas == null || p.iasKt == null || abs(p.iasKt - refIas) <= 10)

        // Najwyżej położony punkt, od którego do 50 ft wszystko było stabilne
        var stableFrom = 0
        for (p in final.asReversed()) { if (stable(p)) stableFrom = hat(p) else break }
        if (stableFrom >= final.maxOf { hat(it) } - 50 && final.maxOf { hat(it) } >= 950) stableFrom = 1000

        val gates = listOf(1000, 500).mapNotNull { g ->
            seg.lastOrNull { hat(it) >= g }?.takeIf { hat(it) < g + 300 }?.let { p -> Gate(g, p.iasKt, p.vsFpm, p.bankDeg, stable(p)) }
        }
        val angle = seg.lastOrNull { hat(it) >= 1000 }?.let { p ->
            val d = Geo.distanceNm(p.lat, p.lon, track[tdIdx].lat, track[tdIdx].lon) * FT_PER_NM
            if (d > 500) Math.toDegrees(atan2(hat(p).toDouble(), d)) else null
        }
        val ias = final.mapNotNull { it.iasKt }
        val spread = if (ias.size >= 3) ias.max() - ias.min() else null
        val maxSink = final.mapNotNull { it.vsFpm }.minOrNull()?.let { -it }
        val maxBank = final.mapNotNull { it.bankDeg?.let(::abs) }.maxOrNull()

        var score = 100.0
        val notes = ArrayList<String>()
        when {
            stableFrom >= 1000 -> notes += loc("Stabilne od 1000 ft — wzorowo.", "Stable from 1000 ft — textbook.")
            stableFrom >= 500 -> { score -= 10; notes += loc("Stabilne dopiero od $stableFrom ft (zalecane 1000 ft w IMC).", "Stable only from $stableFrom ft (1000 ft recommended in IMC).") }
            else -> { score -= 35; notes += loc("Niestabilne poniżej 500 ft (stabilne od $stableFrom ft) — w liniach lotniczych wymagane odejście na drugi krąg.", "Unstable below 500 ft (stable from $stableFrom ft) — airlines would require a go-around.") }
        }
        angle?.let { a ->
            val dev = abs(a - 3.0)
            if (dev > 0.5) { score -= (dev * 10).coerceAtMost(25.0); notes += if (a > 3) loc("Ścieżka stroma: %.1f°.", "Steep path: %.1f°.").format(a) else loc("Ścieżka płaska: %.1f°.", "Shallow path: %.1f°.").format(a) }
        }
        spread?.let { if (it > 15) { score -= ((it - 15) * 0.8).coerceAtMost(20.0); notes += loc("Prędkość niestała: rozrzut $it kt.", "Speed not held: $it kt spread.") } }
        maxSink?.let { if (it > 1200) { score -= 15; notes += loc("Opadanie do $it ft/min na prostej.", "Sink rate up to $it ft/min on final.") } }
        maxBank?.let { if (it > 10) { score -= 10; notes += loc("Przechylenie do ${it.roundToInt()}° na prostej.", "Bank up to ${it.roundToInt()}° on final.") } }
        return ApproachAnalysis(landingAt, seg.first().t, gates, stableFrom, angle, spread, maxSink, maxBank, score.roundToInt().coerceIn(0, 100), notes)
    }

    /** Płynność z par kolejnych punktów w powietrzu; przy rzadkim zapisie (> 3 s) tempo zmian nie ma sensu — null. */
    fun smoothness(track: List<TrackPoint>): Smoothness? {
        val air = track.filter { it.onGround != true && it.gsKt > 40 }
        if (air.size < 30) return null
        val pairs = air.zipWithNext().filter { (a, b) -> b.t - a.t in 200..5_000 }
        if (pairs.size < 20) return null
        val medianDt = pairs.map { (a, b) -> b.t - a.t }.sorted()[pairs.size / 2]
        if (medianDt > 3_000) return null
        val bankRates = ArrayList<Double>(); val pitchRates = ArrayList<Double>(); val vsChanges = ArrayList<Double>()
        var jerks = 0; var lastJerk = 0L
        pairs.forEach { (a, b) ->
            val dt = (b.t - a.t) / 1000.0
            val br = if (a.bankDeg != null && b.bankDeg != null) abs(b.bankDeg - a.bankDeg) / dt else null
            val pr = if (a.pitchDeg != null && b.pitchDeg != null) abs(b.pitchDeg - a.pitchDeg) / dt else null
            val vc = if (a.vsFpm != null && b.vsFpm != null) abs(b.vsFpm - a.vsFpm) / dt else null
            br?.let { bankRates += it }; pr?.let { pitchRates += it }; vc?.let { vsChanges += it }
            val jerk = (br ?: 0.0) > 10 || (pr ?: 0.0) > 5 || (vc ?: 0.0) > 2000
            if (jerk && b.t - lastJerk > 5_000) { jerks++; lastJerk = b.t }
        }
        fun rms(v: List<Double>) = if (v.isEmpty()) null else sqrt(v.sumOf { it * it } / v.size)
        val bRms = rms(bankRates); val pRms = rms(pitchRates); val vRms = rms(vsChanges) ?: 0.0
        val minutes = (air.last().t - air.first().t) / 60_000.0
        var score = 100.0
        bRms?.let { score -= ((it - 1.5) * 6).coerceIn(0.0, 25.0) }
        pRms?.let { score -= ((it - 0.6) * 15).coerceIn(0.0, 20.0) }
        score -= ((vRms - 150) / 25).coerceIn(0.0, 20.0)
        val jerksPer10 = jerks / minutes.coerceAtLeast(1.0) * 10
        score -= (jerksPer10 * 4).coerceAtMost(30.0)
        val maxBank = air.mapNotNull { it.bankDeg?.let(::abs) }.maxOrNull()
        val maxG = air.mapNotNull { it.gLoad }.maxOrNull()
        val notes = ArrayList<String>()
        if (jerks == 0) notes += loc("Bez gwałtownych ruchów sterami.", "No abrupt control inputs.")
        else notes += loc("Gwałtowne ruchy: $jerks (%.1f na 10 min).", "Abrupt inputs: $jerks (%.1f per 10 min).").format(jerksPer10)
        bRms?.let { if (it > 3) notes += loc("Przechylenie zmieniane nerwowo (%.1f°/s średnio).", "Bank changed nervously (%.1f°/s on average).").format(it) }
        pRms?.let { if (it > 1.2) notes += loc("Pochylenie niespokojne (%.1f°/s średnio).", "Pitch unsteady (%.1f°/s on average).").format(it) }
        if (vRms > 400) notes += loc("Prędkość pionowa skacze — trymuj i prowadź łagodniej.", "Vertical speed jumps around — trim and fly more gently.")
        maxBank?.let { if (it > 35) notes += loc("Największe przechylenie ${it.roundToInt()}°.", "Largest bank ${it.roundToInt()}°.") }
        if (bRms == null) notes += loc("Lot bez zapisu przechylenia — ocena tylko z prędkości pionowej.", "No bank data recorded — rated from vertical speed only.")
        return Smoothness(score.roundToInt().coerceIn(0, 100), bRms, pRms, vRms, jerks, maxBank, maxG, notes)
    }

    fun scenario(f: FlightLog, s: Scenario, a: FlightAnalysisResult = analyze(f)): ScenarioResult {
        val landings = f.allLandings
        val last = landings.lastOrNull()
        val base = f.track.firstOrNull()?.altFt ?: 0
        val app = a.approaches.lastOrNull()
        fun c(label: String, value: String, ok: Boolean?) = Criterion(label, value, ok)
        val landed = c(loc("Lądowanie z oceną", "Scored landing"), last?.let { "${it.score}" } ?: "—", last?.let { it.score >= 75 } ?: false)
        val list = when (s) {
            Scenario.PATTERN -> {
                val climb = f.maxAltFt - base
                listOf(
                    c(loc("To samo lotnisko", "Same airport"), "${f.from ?: "?"} → ${f.to ?: "?"}", if (f.from == null || f.to == null) null else f.from == f.to),
                    c(loc("Wysokość kręgu", "Pattern height"), "$climb ft", climb in 600..1500),
                    c(loc("Czas do 20 min", "Up to 20 min"), "${f.durationMin} min", f.durationMin <= 20),
                    c(loc("Stabilne od 500 ft", "Stable from 500 ft"), app?.let { "${it.stableFromFt} ft" } ?: "—", app?.let { it.stableFromFt >= 500 }),
                    landed,
                )
            }
            Scenario.TOUCH_AND_GO -> listOf(
                c(loc("Przyziemienia (min. 3)", "Touchdowns (min. 3)"), "${landings.size}", landings.size >= 3),
                c(loc("Każde od 60 pkt", "Each 60 or more"), landings.minOfOrNull { it.score }?.let { loc("najsłabsze $it", "lowest $it") } ?: "—", landings.isNotEmpty() && landings.all { it.score >= 60 }),
                c(loc("Bez odbić", "No bounces"), "${landings.sumOf { it.bounces }}", landings.none { it.bounces > 0 }),
            )
            Scenario.CROSS_COUNTRY -> listOf(
                c(loc("Inne lotnisko docelowe", "Different destination"), "${f.from ?: "?"} → ${f.to ?: "?"}", if (f.from == null || f.to == null) null else f.from != f.to),
                c(loc("Wznoszenie od 3000 ft", "Climb of 3000 ft+"), "${f.maxAltFt - base} ft", f.maxAltFt - base >= 3000),
                c(loc("Płynność od 70", "Smoothness 70+"), a.smoothness?.score?.toString() ?: "—", a.smoothness?.let { it.score >= 70 }),
                c(loc("Stabilne od 1000 ft", "Stable from 1000 ft"), app?.let { "${it.stableFromFt} ft" } ?: "—", app?.let { it.stableFromFt >= 1000 }),
                landed,
            )
            Scenario.PRECISION_APPROACH -> listOf(
                c(loc("Stabilne od 1000 ft", "Stable from 1000 ft"), app?.let { "${it.stableFromFt} ft" } ?: "—", app?.let { it.stableFromFt >= 1000 } ?: false),
                c(loc("Ścieżka 2,5–3,5°", "Path 2.5–3.5°"), app?.pathAngleDeg?.let { "%.1f°".format(it) } ?: "—", app?.pathAngleDeg?.let { it in 2.5..3.5 }),
                c(loc("Rozrzut prędkości ≤ 10 kt", "Speed spread ≤ 10 kt"), app?.iasSpreadKt?.let { "$it kt" } ?: "—", app?.iasSpreadKt?.let { it <= 10 }),
                c(loc("Strefa przyziemienia", "Touchdown zone"), last?.alongFt?.let { "$it ft" } ?: "—", last?.alongFt?.let { it in 500..3000 }),
                landed,
            )
        }
        return ScenarioResult(s, list)
    }

    /**
     * Poprzedni lot do porównania: ta sama trasa, inaczej to samo lotnisko docelowe, inaczej ten sam samolot.
     * [all] — dziennik (dowolna kolejność); brane są tylko loty wcześniejsze.
     */
    fun previous(f: FlightLog, all: List<FlightLog>): Pair<FlightLog, Boolean>? {
        val earlier = all.filter { it.start < f.start && it.id != f.id }.sortedByDescending { it.start }
        earlier.firstOrNull { it.from == f.from && it.to == f.to && f.to != null }?.let { return it to true }
        earlier.firstOrNull { it.to == f.to && f.to != null }?.let { return it to false }
        return earlier.firstOrNull { it.aircraft == f.aircraft && f.aircraft != null }?.let { it to false }
    }

    fun compare(f: FlightLog, all: List<FlightLog>, a: FlightAnalysisResult = analyze(f)): Comparison? {
        val (p, same) = previous(f, all) ?: return null
        val b = analyze(p)
        val rows = ArrayList<CompareRow>()
        fun row(label: String, now: Int?, before: Int?, unit: String, higherBetter: Boolean?) {
            if (now == null && before == null) return
            val better = if (now == null || before == null || higherBetter == null || now == before) null else (now > before) == higherBetter
            rows += CompareRow(label, now?.let { "$it$unit" } ?: "—", before?.let { "$it$unit" } ?: "—", better)
        }
        row(loc("Ocena lądowania", "Landing score"), f.allLandings.lastOrNull()?.score, p.allLandings.lastOrNull()?.score, "", true)
        row(loc("Prędkość przyziemienia", "Touchdown rate"), f.allLandings.lastOrNull()?.vsFpm?.let(::abs), p.allLandings.lastOrNull()?.vsFpm?.let(::abs), " ft/min", false)
        row(loc("Ocena podejścia", "Approach score"), a.approaches.lastOrNull()?.score, b.approaches.lastOrNull()?.score, "", true)
        row(loc("Stabilne od", "Stable from"), a.approaches.lastOrNull()?.stableFromFt, b.approaches.lastOrNull()?.stableFromFt, " ft", true)
        row(loc("Płynność", "Smoothness"), a.smoothness?.score, b.smoothness?.score, "", true)
        row(loc("Gwałtowne ruchy", "Abrupt inputs"), a.smoothness?.jerks, b.smoothness?.jerks, "", false)
        row(loc("Czas lotu", "Flight time"), f.durationMin, p.durationMin, " min", if (same) false else null)
        return Comparison(p, same, rows)
    }

    /** Raport instruktorski: zwykły tekst do udostępnienia (komunikator, e-mail, notatki). */
    fun instructorReport(f: FlightLog, a: FlightAnalysisResult, scenario: ScenarioResult?, cmp: Comparison?): String = buildString {
        val strengths = ArrayList<String>(); val improve = ArrayList<String>(); val advice = ArrayList<String>()
        val last = f.allLandings.lastOrNull()
        val app = a.approaches.lastOrNull()
        val sm = a.smoothness
        appendLine(loc("RAPORT INSTRUKTORSKI — OpenTakeoff", "INSTRUCTOR REPORT — OpenTakeoff"))
        appendLine("${f.from ?: "?"} → ${f.to ?: "?"}" + (f.aircraft?.let { ", $it" } ?: ""))
        appendLine(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT, Lang.locale).format(java.util.Date(f.start)) +
            loc(", czas %d:%02d, %d NM, najwyżej %s", ", time %d:%02d, %d NM, highest %s").format(f.durationMin / 60, f.durationMin % 60, f.distanceNm, altLabel(f.maxAltFt)))
        appendLine()
        appendLine(loc("OCENY", "SCORES"))
        f.allLandings.forEachIndexed { i, l -> appendLine(loc("• Lądowanie", "• Landing") + (if (f.allLandings.size > 1) " ${i + 1}" else "") + ": ${l.score}/100 (${l.gradeText}), ${l.vsFpm} ft/min") }
        a.approaches.forEachIndexed { i, ap -> appendLine(loc("• Podejście", "• Approach") + (if (a.approaches.size > 1) " ${i + 1}" else "") + ": ${ap.score}/100, " + loc("stabilne od ${ap.stableFromFt} ft", "stable from ${ap.stableFromFt} ft")) }
        sm?.let { appendLine(loc("• Płynność: ${it.score}/100", "• Smoothness: ${it.score}/100")) }
        scenario?.let { appendLine(loc("• Ćwiczenie ", "• Exercise ") + "„${it.scenario.title}”: ${it.passed}/${it.judged} " + loc("kryteriów", "criteria")) }

        last?.let { l ->
            if (l.score >= 85) strengths += loc("Przyziemienie na ${l.score} pkt (${l.vsFpm} ft/min).", "Touchdown scored ${l.score} (${l.vsFpm} ft/min).")
            else if (l.score < 70) improve += l.noteTexts.firstOrNull() ?: loc("Przyziemienie poniżej oczekiwań (${l.score}).", "Touchdown below expectations (${l.score}).")
            if (abs(l.vsFpm) > 400) advice += loc("Wyrównanie zacznij wcześniej (ok. 20–30 ft) i stopniowo zmniejszaj opadanie, zamiast dociągać tuż nad pasem.", "Start the flare earlier (about 20–30 ft) and reduce the sink gradually instead of pulling just above the runway.")
            if ((l.alongFt ?: 0) > 3000) advice += loc("Ustal punkt celowania 1000 ft za progiem i trzymaj go w jednym miejscu szyby.", "Pick an aiming point 1000 ft past the threshold and keep it fixed in the windscreen.")
            if (abs(l.crossFt ?: 0) > 25) advice += loc("Pracuj sterem kierunku w wyrównaniu, żeby utrzymać oś pasa.", "Use the rudder in the flare to stay on the centreline.")
        }
        app?.let { ap ->
            if (ap.stableFromFt >= 1000) strengths += loc("Podejście ustabilizowane od 1000 ft.", "Approach stabilised from 1000 ft.")
            else improve += ap.notes.first()
            ap.pathAngleDeg?.let { if (abs(it - 3) > 0.5) advice += loc("Pilnuj ścieżki 3°: ok. 300 ft na każdą milę do progu, prędkość pionowa ≈ 5 × GS.", "Keep a 3° path: about 300 ft per mile to the threshold, vertical speed ≈ 5 × GS.") }
            ap.iasSpreadKt?.let { if (it > 15) advice += loc("Ustal konfigurację i prędkość podejścia przed 1000 ft; korekty mocy małe i wcześnie.", "Set the approach configuration and speed before 1000 ft; make small, early power corrections.") }
        }
        sm?.let {
            if (it.score >= 85) strengths += loc("Płynne prowadzenie samolotu.", "Smooth aircraft handling.")
            else if (it.score < 65) improve += it.notes.first()
            if (it.jerks > 3) advice += loc("Wykonuj ruchy sterami małe i płynne; trymuj po każdej zmianie prędkości lub mocy.", "Keep control inputs small and smooth; trim after every speed or power change.")
        }
        a.events.filter { it.kind == EventKind.EXCEEDANCE }.take(3).forEach { improve += it.label + " (" + timeOf(f, it.t) + ")" }
        scenario?.criteria?.filter { it.passed == false }?.forEach { improve += loc("Ćwiczenie: ", "Exercise: ") + "${it.label} — ${it.value}" }
        cmp?.let { c ->
            val better = c.rows.count { it.better == true }; val worse = c.rows.count { it.better == false }
            val what = if (c.sameRoute) loc("poprzednim lotem na tej trasie", "the previous flight on this route") else loc("poprzednim podobnym lotem", "the previous similar flight")
            (if (better >= worse) strengths else improve) += loc("W porównaniu z $what: lepiej w $better, gorzej w $worse pozycjach.", "Compared with $what: better in $better, worse in $worse items.")
        }
        if (f.markers.isNotEmpty()) { appendLine(); appendLine(loc("ZNACZNIKI", "MARKERS")); f.markers.forEach { appendLine("• ${timeOf(f, it.t)} ${it.label}") } }
        appendLine(); appendLine(loc("MOCNE STRONY", "STRENGTHS"))
        (strengths.ifEmpty { listOf(loc("Lot ukończony bez poważnych uwag.", "Flight completed without major remarks.")) }).forEach { appendLine("+ $it") }
        appendLine(); appendLine(loc("DO POPRAWY", "TO IMPROVE"))
        (improve.ifEmpty { listOf(loc("Brak istotnych uwag.", "No significant remarks.")) }).forEach { appendLine("− $it") }
        if (advice.isNotEmpty()) { appendLine(); appendLine(loc("ZALECENIA NA NASTĘPNY LOT", "FOR THE NEXT FLIGHT")); advice.distinct().forEach { appendLine("→ $it") } }
    }.trimEnd()

    /** Czas od początku lotu jako „m:ss” albo „h:mm:ss”. */
    fun timeOf(f: FlightLog, t: Long): String {
        val s = ((t - f.start) / 1000).coerceAtLeast(0)
        return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
    }

    fun altLabel(ft: Int) = if (ft >= 18_000) "FL%03d".format(ft / 100) else "$ft ft"
}
