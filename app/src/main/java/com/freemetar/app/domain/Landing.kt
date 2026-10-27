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
            vs < 60 -> { score -= 5; notes += loc("Bardzo miękko (${vs.roundToInt()} ft/min) — uważaj na przeciąganie wyrównania.", "Very soft (${vs.roundToInt()} ft/min) — watch out for a long float.") }
            vs <= 240 -> notes += loc("Miękkie przyziemienie: ${vs.roundToInt()} ft/min.", "Soft touchdown: ${vs.roundToInt()} ft/min.")
            vs <= 400 -> { score -= 10 + (vs - 240) / 160 * 15; notes += loc("Wyczuwalne przyziemienie: ${vs.roundToInt()} ft/min.", "Firm touchdown: ${vs.roundToInt()} ft/min.") }
            vs <= 600 -> { score -= 25 + (vs - 400) / 200 * 25; notes += loc("Twarde lądowanie: ${vs.roundToInt()} ft/min.", "Hard landing: ${vs.roundToInt()} ft/min.") }
            else -> { score -= 60; notes += loc("Bardzo twarde lądowanie: ${vs.roundToInt()} ft/min — w realnym lotnictwie przegląd techniczny.", "Very hard landing: ${vs.roundToInt()} ft/min — in real aviation this means a hard-landing inspection.") }
        }
        t.gForce?.let { g ->
            when {
                g >= 2.0 -> { score -= 30; notes += loc("Przeciążenie", "G-force") + " %.2f g.".format(g) }
                g >= 1.6 -> { score -= 15; notes += loc("Przeciążenie", "G-force") + " %.2f g.".format(g) }
                g >= 1.4 -> score -= 5
            }
        }
        t.bankDeg?.let { b ->
            val a = abs(b)
            when {
                a > 8 -> { score -= 20; notes += loc("Duże przechylenie przy przyziemieniu: ${a.roundToInt()}°.", "Large bank at touchdown: ${a.roundToInt()}°.") }
                a > 4 -> { score -= 8; notes += loc("Przechylenie ${a.roundToInt()}° — przyziemienie na jedno koło.", "Bank ${a.roundToInt()}° — touchdown on one main gear.") }
            }
        }
        t.pitchDeg?.let { p ->
            when {
                p < 0 -> { score -= 20; notes += loc("Przyziemienie na przednie koło (pochylenie %.1f°).", "Nose-wheel first touchdown (pitch %.1f°).").format(p) }
                p > 10 -> { score -= 10; notes += loc("Duże pochylenie ${p.roundToInt()}° — ryzyko uderzenia ogonem.", "High pitch ${p.roundToInt()}° — risk of a tail strike.") }
            }
        }
        t.alongFt?.let { a ->
            when {
                a < 0 -> { score -= 40; notes += loc("Przyziemienie przed progiem pasa (${(-a).roundToInt()} ft).", "Touchdown before the runway threshold (${(-a).roundToInt()} ft).") }
                a < 500 -> { score -= 5; notes += loc("Przyziemienie tuż za progiem (${a.roundToInt()} ft).", "Touchdown just past the threshold (${a.roundToInt()} ft).") }
                a <= 3000 -> notes += loc("W strefie przyziemienia: ${a.roundToInt()} ft za progiem.", "In the touchdown zone: ${a.roundToInt()} ft past the threshold.")
                else -> {
                    val remaining = t.runwayLengthFt?.let { it - a }
                    score -= if (a > 4500) 25 else 10
                    notes += loc("Długie lądowanie: ${a.roundToInt()} ft za progiem", "Long landing: ${a.roundToInt()} ft past the threshold") + (remaining?.let { loc(", zostało ${it.roundToInt()} ft pasa.", ", ${it.roundToInt()} ft of runway remaining.") } ?: ".")
                }
            }
        }
        t.crossFt?.let { x ->
            val a = abs(x)
            val side = if (x >= 0) loc("w prawo", "right") else loc("w lewo", "left")
            when {
                a > 50 -> { score -= 25; notes += loc("Daleko od osi pasa: ${a.roundToInt()} ft $side.", "Far off the centreline: ${a.roundToInt()} ft $side.") }
                a > 25 -> { score -= 12; notes += loc("Odchylenie od osi: ${a.roundToInt()} ft $side.", "Off the centreline: ${a.roundToInt()} ft $side.") }
                a > 12 -> score -= 4
            }
        }
        if (t.bounces > 0) { score -= 12.0 * t.bounces; notes += loc("Odbicia: ${t.bounces}.", "Bounces: ${t.bounces}.") }
        // Silny wiatr boczny łagodzi ocenę przechylenia i osi
        t.crosswindKt?.let { if (abs(it) >= 15) { score += 5; notes += loc("Wiatr boczny ${abs(it).roundToInt()} kt — wynik złagodzony.", "Crosswind ${abs(it).roundToInt()} kt — score adjusted.") } }

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
        s >= 90 -> loc("Doskonałe", "Excellent")
        s >= 75 -> loc("Dobre", "Good")
        s >= 55 -> loc("Poprawne", "Fair")
        s >= 35 -> loc("Twarde", "Hard")
        else -> loc("Bardzo twarde", "Very hard")
    }
}
