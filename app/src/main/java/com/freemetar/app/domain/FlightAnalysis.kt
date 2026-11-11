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
        PATTERN -> loc("flight_analysis.traffic_pattern")
        TOUCH_AND_GO -> loc("flight_analysis.touch_and_go_series")
        CROSS_COUNTRY -> loc("flight_analysis.cross_country")
        PRECISION_APPROACH -> loc("flight_analysis.precision_approach")
    }
    val description: String get() = when (this) {
        PATTERN -> loc("flight_analysis.take_off_and_landing_at_the")
        TOUCH_AND_GO -> loc("flight_analysis.at_least_3_touchdowns_each_scoring")
        CROSS_COUNTRY -> loc("flight_analysis.landing_at_another_airport_climbing_at")
        PRECISION_APPROACH -> loc("flight_analysis.stable_from_1000_ft_2_5")
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
        ev += FlightEvent(takeoff.t, EventKind.TAKEOFF, loc("flight_analysis.take_off") + (f.from?.let { " $it" } ?: ""))
        val base = tr.first().altFt
        if (f.maxAltFt - base >= 2000) {
            val cruise = f.maxAltFt - 300
            tr.firstOrNull { it.altFt >= cruise }?.let { ev += FlightEvent(it.t, EventKind.TOC, loc("flight_analysis.top_of_climb") + " ${altLabel(f.maxAltFt)}") }
            tr.lastOrNull { it.altFt >= cruise }?.let { ev += FlightEvent(it.t, EventKind.TOD, loc("common.top_of_descent")) }
        }
        f.allLandings.forEach { l ->
            l.at?.let { ev += FlightEvent(it, EventKind.LANDING, loc("flight_analysis.touchdown") + " ${l.vsFpm} ft/min" + ((l.runway ?: l.airport)?.let { r -> ", $r" } ?: "")) }
        }
        approaches.filter { it.stableFromFt < 500 }.forEach {
            ev += FlightEvent(it.startAt, EventKind.EXCEEDANCE, loc("flight_analysis.unstable_approach"))
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
            p.bankDeg?.let { if (abs(it) > 45) flag("bank", p, loc("common.bank") + " ${abs(it).roundToInt()}°") }
            p.gLoad?.let { if (it > 2.0f || it < 0f) flag("g", p, loc("flight_analysis.g_load") + " %.1f g".format(it)) }
            val h = p.aglFt
            if (h != null && h < 1000 && (p.vsFpm ?: 0) < -2000) flag("sink", p, loc("flight_analysis.high_sink_rate_low") + " ${p.vsFpm} ft/min")
            p.pitchDeg?.let { if (it > 25 || it < -20) flag("pitch", p, loc("common.pitch") + " ${it.roundToInt()}°") }
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
            stableFrom >= 1000 -> notes += loc("flight_analysis.stable_from_1000_ft_textbook")
            stableFrom >= 500 -> { score -= 10; notes += loc("flight_analysis.stable_only_from_ft_1000_ft", stableFrom) }
            else -> { score -= 35; notes += loc("flight_analysis.unstable_below_500_ft_stable_from", stableFrom) }
        }
        angle?.let { a ->
            val dev = abs(a - 3.0)
            if (dev > 0.5) { score -= (dev * 10).coerceAtMost(25.0); notes += if (a > 3) loc("flight_analysis.steep_path_1f").format(a) else loc("flight_analysis.shallow_path_1f").format(a) }
        }
        spread?.let { if (it > 15) { score -= ((it - 15) * 0.8).coerceAtMost(20.0); notes += loc("flight_analysis.speed_not_held_kt_spread", it) } }
        maxSink?.let { if (it > 1200) { score -= 15; notes += loc("flight_analysis.sink_rate_up_to_ft_min", it) } }
        maxBank?.let { if (it > 10) { score -= 10; notes += loc("flight_analysis.bank_up_to_on_final", it.roundToInt()) } }
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
        if (jerks == 0) notes += loc("flight_analysis.no_abrupt_control_inputs")
        else notes += loc("flight_analysis.abrupt_inputs_1f_per_10_min", jerks).format(jerksPer10)
        bRms?.let { if (it > 3) notes += loc("flight_analysis.bank_changed_nervously_1f_s_on").format(it) }
        pRms?.let { if (it > 1.2) notes += loc("flight_analysis.pitch_unsteady_1f_s_on_average").format(it) }
        if (vRms > 400) notes += loc("flight_analysis.vertical_speed_jumps_around_trim_and")
        maxBank?.let { if (it > 35) notes += loc("flight_analysis.largest_bank", it.roundToInt()) }
        if (bRms == null) notes += loc("flight_analysis.no_bank_data_recorded_rated_from")
        return Smoothness(score.roundToInt().coerceIn(0, 100), bRms, pRms, vRms, jerks, maxBank, maxG, notes)
    }

    fun scenario(f: FlightLog, s: Scenario, a: FlightAnalysisResult = analyze(f)): ScenarioResult {
        val landings = f.allLandings
        val last = landings.lastOrNull()
        val base = f.track.firstOrNull()?.altFt ?: 0
        val app = a.approaches.lastOrNull()
        fun c(label: String, value: String, ok: Boolean?) = Criterion(label, value, ok)
        val landed = c(loc("flight_analysis.scored_landing"), last?.let { "${it.score}" } ?: "—", last?.let { it.score >= 75 } ?: false)
        val list = when (s) {
            Scenario.PATTERN -> {
                val climb = f.maxAltFt - base
                listOf(
                    c(loc("flight_analysis.same_airport"), "${f.from ?: "?"} → ${f.to ?: "?"}", if (f.from == null || f.to == null) null else f.from == f.to),
                    c(loc("flight_analysis.pattern_height"), "$climb ft", climb in 600..1500),
                    c(loc("flight_analysis.up_to_20_min"), "${f.durationMin} min", f.durationMin <= 20),
                    c(loc("flight_analysis.stable_from_500_ft"), app?.let { "${it.stableFromFt} ft" } ?: "—", app?.let { it.stableFromFt >= 500 }),
                    landed,
                )
            }
            Scenario.TOUCH_AND_GO -> listOf(
                c(loc("flight_analysis.touchdowns_min_3"), "${landings.size}", landings.size >= 3),
                c(loc("flight_analysis.each_60_or_more"), landings.minOfOrNull { it.score }?.let { loc("flight_analysis.lowest", it) } ?: "—", landings.isNotEmpty() && landings.all { it.score >= 60 }),
                c(loc("flight_analysis.no_bounces"), "${landings.sumOf { it.bounces }}", landings.none { it.bounces > 0 }),
            )
            Scenario.CROSS_COUNTRY -> listOf(
                c(loc("flight_analysis.different_destination"), "${f.from ?: "?"} → ${f.to ?: "?"}", if (f.from == null || f.to == null) null else f.from != f.to),
                c(loc("flight_analysis.climb_of_3000_ft"), "${f.maxAltFt - base} ft", f.maxAltFt - base >= 3000),
                c(loc("flight_analysis.smoothness_70"), a.smoothness?.score?.toString() ?: "—", a.smoothness?.let { it.score >= 70 }),
                c(loc("flight_analysis.stable_from_1000_ft"), app?.let { "${it.stableFromFt} ft" } ?: "—", app?.let { it.stableFromFt >= 1000 }),
                landed,
            )
            Scenario.PRECISION_APPROACH -> listOf(
                c(loc("flight_analysis.stable_from_1000_ft"), app?.let { "${it.stableFromFt} ft" } ?: "—", app?.let { it.stableFromFt >= 1000 } ?: false),
                c(loc("flight_analysis.path_2_5_3_5"), app?.pathAngleDeg?.let { "%.1f°".format(it) } ?: "—", app?.pathAngleDeg?.let { it in 2.5..3.5 }),
                c(loc("flight_analysis.speed_spread_10_kt"), app?.iasSpreadKt?.let { "$it kt" } ?: "—", app?.iasSpreadKt?.let { it <= 10 }),
                c(loc("flight_analysis.touchdown_zone"), last?.alongFt?.let { "$it ft" } ?: "—", last?.alongFt?.let { it in 500..3000 }),
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
        row(loc("flight_analysis.landing_score"), f.allLandings.lastOrNull()?.score, p.allLandings.lastOrNull()?.score, "", true)
        row(loc("flight_analysis.touchdown_rate"), f.allLandings.lastOrNull()?.vsFpm?.let(::abs), p.allLandings.lastOrNull()?.vsFpm?.let(::abs), " ft/min", false)
        row(loc("flight_analysis.approach_score"), a.approaches.lastOrNull()?.score, b.approaches.lastOrNull()?.score, "", true)
        row(loc("flight_analysis.stable_from"), a.approaches.lastOrNull()?.stableFromFt, b.approaches.lastOrNull()?.stableFromFt, " ft", true)
        row(loc("common.smoothness"), a.smoothness?.score, b.smoothness?.score, "", true)
        row(loc("flight_analysis.abrupt_inputs"), a.smoothness?.jerks, b.smoothness?.jerks, "", false)
        row(loc("common.flight_time"), f.durationMin, p.durationMin, " min", if (same) false else null)
        return Comparison(p, same, rows)
    }

    /** Raport instruktorski: zwykły tekst do udostępnienia (komunikator, e-mail, notatki). */
    fun instructorReport(f: FlightLog, a: FlightAnalysisResult, scenario: ScenarioResult?, cmp: Comparison?): String = buildString {
        val strengths = ArrayList<String>(); val improve = ArrayList<String>(); val advice = ArrayList<String>()
        val last = f.allLandings.lastOrNull()
        val app = a.approaches.lastOrNull()
        val sm = a.smoothness
        appendLine(loc("flight_analysis.instructor_report_opentakeoff"))
        appendLine("${f.from ?: "?"} → ${f.to ?: "?"}" + (f.aircraft?.let { ", $it" } ?: ""))
        appendLine(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.MEDIUM, java.text.DateFormat.SHORT, Lang.locale).format(java.util.Date(f.start)) +
            loc("flight_analysis.time_d_02d_d_nm_highest").format(f.durationMin / 60, f.durationMin % 60, f.distanceNm, altLabel(f.maxAltFt)))
        appendLine()
        appendLine(loc("flight_analysis.scores"))
        f.allLandings.forEachIndexed { i, l -> appendLine(loc("flight_analysis.landing") + (if (f.allLandings.size > 1) " ${i + 1}" else "") + ": ${l.score}/100 (${l.gradeText}), ${l.vsFpm} ft/min") }
        a.approaches.forEachIndexed { i, ap -> appendLine(loc("flight_analysis.approach") + (if (a.approaches.size > 1) " ${i + 1}" else "") + ": ${ap.score}/100, " + loc("common.stable_from_ft", ap.stableFromFt)) }
        sm?.let { appendLine(loc("flight_analysis.smoothness_100", it.score)) }
        scenario?.let { appendLine(loc("flight_analysis.exercise_2") + "„${it.scenario.title}”: ${it.passed}/${it.judged} " + criteriaWord(it.judged)) }

        last?.let { l ->
            if (l.score >= 85) strengths += loc("flight_analysis.touchdown_scored_ft_min", l.score, l.vsFpm)
            else if (l.score < 70) improve += l.noteTexts.firstOrNull() ?: loc("flight_analysis.touchdown_below_expectations", l.score)
            if (abs(l.vsFpm) > 400) advice += loc("flight_analysis.start_the_flare_earlier_about_20")
            if ((l.alongFt ?: 0) > 3000) advice += loc("flight_analysis.pick_an_aiming_point_1000_ft")
            if (abs(l.crossFt ?: 0) > 25) advice += loc("flight_analysis.use_the_rudder_in_the_flare")
        }
        app?.let { ap ->
            if (ap.stableFromFt >= 1000) strengths += loc("flight_analysis.approach_stabilised_from_1000_ft")
            else improve += ap.notes.first()
            ap.pathAngleDeg?.let { if (abs(it - 3) > 0.5) advice += loc("flight_analysis.keep_a_3_path_about_300") }
            ap.iasSpreadKt?.let { if (it > 15) advice += loc("flight_analysis.set_the_approach_configuration_and_speed") }
        }
        sm?.let {
            if (it.score >= 85) strengths += loc("flight_analysis.smooth_aircraft_handling")
            else if (it.score < 65) improve += it.notes.first()
            if (it.jerks > 3) advice += loc("flight_analysis.keep_control_inputs_small_and_smooth")
        }
        a.events.filter { it.kind == EventKind.EXCEEDANCE }.take(3).forEach { improve += it.label + " (" + timeOf(f, it.t) + ")" }
        scenario?.criteria?.filter { it.passed == false }?.forEach { improve += loc("flight_analysis.exercise") + "${it.label} — ${it.value}" }
        cmp?.let { c ->
            val better = c.rows.count { it.better == true }; val worse = c.rows.count { it.better == false }
            val what = if (c.sameRoute) loc("flight_analysis.the_previous_flight_on_this_route") else loc("flight_analysis.the_previous_similar_flight")
            (if (better >= worse) strengths else improve) += loc("flight_analysis.compared_with_better_in_worse_in", what, better, worse, plural(worse, "items_in"))
        }
        if (f.markers.isNotEmpty()) { appendLine(); appendLine(loc("flight_analysis.markers")); f.markers.forEach { appendLine("• ${timeOf(f, it.t)} ${it.label}") } }
        appendLine(); appendLine(loc("flight_analysis.strengths"))
        (strengths.ifEmpty { listOf(loc("flight_analysis.flight_completed_without_major_remarks")) }).forEach { appendLine("+ $it") }
        appendLine(); appendLine(loc("flight_analysis.to_improve"))
        (improve.ifEmpty { listOf(loc("flight_analysis.no_significant_remarks")) }).forEach { appendLine("− $it") }
        if (advice.isNotEmpty()) { appendLine(); appendLine(loc("flight_analysis.for_the_next_flight")); advice.distinct().forEach { appendLine("→ $it") } }
    }.trimEnd()

    /** Czas od początku lotu jako „m:ss” albo „h:mm:ss”. */
    fun timeOf(f: FlightLog, t: Long): String {
        val s = ((t - f.start) / 1000).coerceAtLeast(0)
        return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
    }

    fun altLabel(ft: Int) = if (ft >= 18_000) "FL%03d".format(ft / 100) else "$ft ft"
}
