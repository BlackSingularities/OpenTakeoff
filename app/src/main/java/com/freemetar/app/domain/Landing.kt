package com.freemetar.app.domain

import kotlinx.serialization.Serializable
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

/** Geometria pasa: próg, kurs rzeczywisty i długość — do położenia przyziemienia. */
data class RunwayGeom(val ident: String, val thresholdLat: Double, val thresholdLon: Double, val headingTrueDeg: Double, val lengthFt: Double?)

/** Położenie punktu względem progu: wzdłuż osi (ft, + = za progiem) i w poprzek (ft, + = w prawo). */
data class RunwayOffset(val alongFt: Double, val crossFt: Double)

object RunwayMath {
    /** Przybliżenie płaskie (wystarczające na długości pasa). */
    fun offset(rw: RunwayGeom, lat: Double, lon: Double): RunwayOffset {
        val ftPerDegLat = 364_000.0
        val ftPerDegLon = 364_000.0 * cos(Math.toRadians(rw.thresholdLat))
        val north = (lat - rw.thresholdLat) * ftPerDegLat
        val east = (lon - rw.thresholdLon) * ftPerDegLon
        val h = Math.toRadians(rw.headingTrueDeg)
        val along = north * cos(h) + east * sin(h)
        val cross = east * cos(h) - north * sin(h)
        return RunwayOffset(along, cross)
    }

    /** Pas, na którym wylądował samolot: kurs zgodny (±30°) i najmniejsze odchylenie od osi. */
    fun pick(candidates: List<RunwayGeom>, lat: Double, lon: Double, headingTrueDeg: Double?): RunwayGeom? =
        candidates.filter { rw ->
            headingTrueDeg == null || abs(((rw.headingTrueDeg - headingTrueDeg + 540) % 360) - 180) <= 30
        }.map { it to offset(it, lat, lon) }
            .filter { (rw, o) -> o.alongFt > -1500 && o.alongFt < (rw.lengthFt ?: 15000.0) + 1000 && abs(o.crossFt) < 500 }
            .minByOrNull { abs(it.second.crossFt) }?.first
}

/** Dane przyziemienia do oceny. Brakujące wartości po prostu nie wpływają na wynik. */
data class TouchdownData(
    val vsFpm: Double,
    val gForce: Double? = null,
    val bankDeg: Double? = null,
    val pitchDeg: Double? = null,
    val iasKt: Double? = null,
    val runway: String? = null,
    val alongFt: Double? = null,
    val crossFt: Double? = null,
    val runwayLengthFt: Double? = null,
    val crosswindKt: Double? = null,
    val bounces: Int = 0,
)

@Serializable
data class LandingReport(
    val score: Int,
    val grade: String,
    val vsFpm: Int,
    val gForce: Double? = null,
    val bankDeg: Double? = null,
    val pitchDeg: Double? = null,
    val iasKt: Int? = null,
    val runway: String? = null,
    val alongFt: Int? = null,
    val crossFt: Int? = null,
    val crosswindKt: Int? = null,
    val bounces: Int = 0,
    val notes: List<String> = emptyList(),
    /** Lotnisko najbliższe przyziemieniu (lot może mieć kilka lądowań na różnych lotniskach). */
    val airport: String? = null,
    /** Chwila przyziemienia (ms). */
    val at: Long? = null,
    /** Długość pasa (ft) — do „zostało … ft pasa” przy odtwarzaniu uwag. */
    val runwayLengthFt: Int? = null,
) {
    /**
     * Ocena słowna i uwagi w bieżącym języku aplikacji. Zapisane [grade] i [notes] są w języku z chwili
     * lądowania, więc do wyświetlania uwagi są odtwarzane z liczb tą samą oceną co przy zapisie.
     */
    val gradeText: String get() = LandingScore.gradeOf(score)
    val noteTexts: List<String> get() = LandingScore.evaluate(
        TouchdownData(
            vsFpm = vsFpm.toDouble(), gForce = gForce, bankDeg = bankDeg, pitchDeg = pitchDeg, iasKt = iasKt?.toDouble(),
            runway = runway, alongFt = alongFt?.toDouble(), crossFt = crossFt?.toDouble(), runwayLengthFt = runwayLengthFt?.toDouble(),
            crosswindKt = crosswindKt?.toDouble(), bounces = bounces,
        ),
    ).notes
}

/**
 * Ocena lądowania 0–100. Progi odpowiadają typowym kryteriom: miękkie przyziemienie 60–240 ft/min,
 * twarde od 400 ft/min, ciężkie (wymaga przeglądu w realnym lotnictwie) od 600 ft/min lub 2 g;
 * strefa przyziemienia 500–3000 ft za progiem.
 */
object LandingScore {
    fun evaluate(t: TouchdownData): LandingReport {
        var score = 100.0
        val notes = mutableListOf<String>()
        val vs = abs(t.vsFpm)
        when {
            vs < 60 -> { score -= 5; notes += loc("landing.very_soft_ft_min_watch_out", vs.roundToInt()) }
            vs <= 240 -> notes += loc("landing.soft_touchdown_ft_min", vs.roundToInt())
            vs <= 400 -> { score -= 10 + (vs - 240) / 160 * 15; notes += loc("landing.firm_touchdown_ft_min", vs.roundToInt()) }
            vs <= 600 -> { score -= 25 + (vs - 400) / 200 * 25; notes += loc("landing.hard_landing_ft_min", vs.roundToInt()) }
            else -> { score -= 60; notes += loc("landing.very_hard_landing_ft_min_in", vs.roundToInt()) }
        }
        t.gForce?.let { g ->
            when {
                g >= 2.0 -> { score -= 30; notes += loc("common.g_force") + " %.2f g.".format(g) }
                g >= 1.6 -> { score -= 15; notes += loc("common.g_force") + " %.2f g.".format(g) }
                g >= 1.4 -> score -= 5
            }
        }
        t.bankDeg?.let { b ->
            val a = abs(b)
            when {
                a > 8 -> { score -= 20; notes += loc("landing.large_bank_at_touchdown", a.roundToInt()) }
                a > 4 -> { score -= 8; notes += loc("landing.bank_touchdown_on_one_main_gear", a.roundToInt()) }
            }
        }
        t.pitchDeg?.let { p ->
            when {
                p < 0 -> { score -= 20; notes += loc("landing.nose_wheel_first_touchdown_pitch_1f").format(p) }
                p > 10 -> { score -= 10; notes += loc("landing.high_pitch_risk_of_a_tail", p.roundToInt()) }
            }
        }
        t.alongFt?.let { a ->
            when {
                a < 0 -> { score -= 40; notes += loc("landing.touchdown_before_the_runway_threshold_ft", (-a).roundToInt()) }
                a < 500 -> { score -= 5; notes += loc("landing.touchdown_just_past_the_threshold_ft", a.roundToInt()) }
                a <= 3000 -> notes += loc("landing.in_the_touchdown_zone_ft_past", a.roundToInt())
                else -> {
                    val remaining = t.runwayLengthFt?.let { it - a }
                    score -= if (a > 4500) 25 else 10
                    notes += loc("landing.long_landing_ft_past_the_threshold", a.roundToInt()) + (remaining?.let { loc("landing.ft_of_runway_remaining", it.roundToInt()) } ?: ".")
                }
            }
        }
        t.crossFt?.let { x ->
            val a = abs(x)
            val side = if (x >= 0) loc("common.right") else loc("common.left")
            when {
                a > 50 -> { score -= 25; notes += loc("landing.far_off_the_centreline_ft", a.roundToInt(), side) }
                a > 25 -> { score -= 12; notes += loc("landing.off_the_centreline_ft", a.roundToInt(), side) }
                a > 12 -> score -= 4
            }
        }
        if (t.bounces > 0) { score -= 12.0 * t.bounces; notes += loc("landing.bounces", t.bounces) }
        // Silny wiatr boczny łagodzi ocenę przechylenia i osi
        t.crosswindKt?.let { if (abs(it) >= 15) { score += 5; notes += loc("landing.crosswind_kt_score_adjusted", abs(it).roundToInt()) } }

        val s = score.roundToInt().coerceIn(0, 100)
        return LandingReport(
            score = s, grade = gradeOf(s), vsFpm = t.vsFpm.roundToInt(), gForce = t.gForce, bankDeg = t.bankDeg, pitchDeg = t.pitchDeg,
            iasKt = t.iasKt?.roundToInt(), runway = t.runway, alongFt = t.alongFt?.roundToInt(), crossFt = t.crossFt?.roundToInt(),
            crosswindKt = t.crosswindKt?.roundToInt(), bounces = t.bounces, notes = notes,
            runwayLengthFt = t.runwayLengthFt?.roundToInt(),
        )
    }

    /** Ocena słowna z wyniku 0–100 w bieżącym języku. */
    fun gradeOf(s: Int): String = when {
        s >= 90 -> loc("landing.excellent")
        s >= 75 -> loc("landing.good")
        s >= 55 -> loc("landing.fair")
        s >= 35 -> loc("landing.hard")
        else -> loc("landing.very_hard")
    }
}
