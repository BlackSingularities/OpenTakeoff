package com.freemetar.app.ui.screens

import com.freemetar.app.domain.Geo
import org.maplibre.geojson.Feature
import org.maplibre.geojson.LineString
import org.maplibre.geojson.Point
import kotlin.math.abs
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sign

/** Punkt odniesienia radiali: lotnisko, pomoc nawigacyjna albo dowolne miejsce na mapie ([name] = null). */
data class RefPoint(val lat: Double, val lon: Double, val name: String?)

internal fun norm360(a: Double) = ((a % 360) + 360) % 360
internal fun norm180(a: Double) = norm360(a + 180) - 180

/** Kurs w zapisie kompasowym: 360 zamiast 000, zawsze trzy cyfry. */
internal fun deg3(a: Double): String = norm360(a).let { Math.round(it).toInt() }.let { if (it == 0) 360 else it }.let { "%03d".format(it) }

private const val EARTH_M = 6_371_008.8

/** Radial jako liczba 1…360 (północ = 360). */
internal fun radialOf(deg: Double): Int = Math.round(norm360(deg)).toInt().let { if (it == 0 || it == 360) 360 else it }

/** Punkt w odległości [distM] od (lat, lon) w kierunku [brgTrueDeg] (ortodroma). */
internal fun destPoint(lat: Double, lon: Double, brgTrueDeg: Double, distM: Double): Point {
    val d = distM / EARTH_M
    val b = Math.toRadians(brgTrueDeg)
    val la1 = Math.toRadians(lat); val lo1 = Math.toRadians(lon)
    val la2 = asin(sin(la1) * cos(d) + cos(la1) * sin(d) * cos(b))
    val lo2 = lo1 + atan2(sin(b) * sin(d) * cos(la1), cos(d) - sin(la1) * sin(la2))
    return Point.fromLngLat((Math.toDegrees(lo2) + 540) % 360 - 180, Math.toDegrees(la2))
}

private fun line(pts: List<Point>, color: String, w: Float) = Feature.fromGeometry(LineString.fromLngLats(pts)).apply {
    addStringProperty("color", color); addNumberProperty("w", w)
}

private fun label(p: Point, text: String, rot: Double, color: String, size: Float) = Feature.fromGeometry(p).apply {
    addStringProperty("label", text); addNumberProperty("rot", norm360(rot)); addStringProperty("color", color); addNumberProperty("size", size)
}

/** Linie i napisy róży: osobno, bo napisy idą do warstwy symboli. */
internal class RoseGeo(val lines: List<Feature>, val labels: List<Feature>)

/**
 * Róża kompasowa w płaszczyźnie mapy: okrąg o promieniu [rM], kreski co [tickDeg]° (dłuższe co 10° i 30°),
 * opisy co 30° jak na HSI (N, 3, 6, E…). Zera róży są magnetyczne: obrót o deklinację [magVar] (E +).
 */
internal fun compassRose(lat: Double, lon: Double, rM: Double, magVar: Double, color: String, tickDeg: Int = 5, labelSize: Float = 12f): RoseGeo {
    val lines = ArrayList<Feature>(); val labels = ArrayList<Feature>()
    lines += line((0..120).map { destPoint(lat, lon, it * 3.0, rM) }, color, 1.6f)
    for (m in 0 until 360 step tickDeg) {
        val t = m + magVar
        val len = when { m % 30 == 0 -> 0.13; m % 10 == 0 -> 0.08; else -> 0.045 }
        lines += line(listOf(destPoint(lat, lon, t, rM), destPoint(lat, lon, t, rM * (1 - len))), color, if (m % 30 == 0) 2.2f else 1.3f)
        if (m % 30 == 0) {
            val txt = when (m) { 0 -> "N"; 90 -> "E"; 180 -> "S"; 270 -> "W"; else -> (m / 10).toString() }
            labels += label(destPoint(lat, lon, t, rM * 0.76), txt, t, color, labelSize)
        }
    }
    return RoseGeo(lines, labels)
}

/** Znacznik na obwodzie róży: trójkąt skierowany do środka (namiar, ślad, nastawa HDG). */
internal fun roseMarker(lat: Double, lon: Double, rM: Double, brgTrue: Double, color: String, outside: Boolean = true): Feature {
    val tip = destPoint(lat, lon, brgTrue, if (outside) rM * 1.01 else rM * 0.99)
    val h = rM * 0.09
    val base = if (outside) rM * 1.01 + h else rM * 0.99 - h
    val half = Math.toDegrees(h * 0.7 / rM)
    val ring = listOf(tip, destPoint(lat, lon, brgTrue - half, base), destPoint(lat, lon, brgTrue + half, base), tip)
    return line(ring, color, 2.6f)
}

internal fun segment(a: Point, b: Point, color: String, w: Float) = line(listOf(a, b), color, w)

internal fun roseLabel(p: Point, text: String, rot: Double, color: String, size: Float) = label(p, text, rot, color, size)

/** Położenie samolotu względem punktu: radial (od punktu), kurs do punktu i odległość — w stopniach magnetycznych. */
data class RadialFix(val radialMag: Double, val toMag: Double, val distNm: Double)

internal fun radialFix(ref: RefPoint, lat: Double, lon: Double, magVar: Double): RadialFix {
    val from = Geo.bearing(ref.lat, ref.lon, lat, lon)
    val to = Geo.bearing(lat, lon, ref.lat, ref.lon)
    return RadialFix(norm360(from - magVar), norm360(to - magVar), Geo.distanceNm(ref.lat, ref.lon, lat, lon))
}

/**
 * Prowadzenie na wybrany radial.
 * [course] — kurs do utrzymania na radialu (OUT = radial, IN = radial + 180),
 * [xtkNm] — odchylenie, dodatnie = samolot na prawo od kursu, [alongNm] — odległość wzdłuż kursu od punktu,
 * [track] — ślad przechwycenia, [heading] — kurs z poprawką na wiatr ([wcaDeg]), [interceptDeg] — kąt przechwycenia,
 * [behind] — samolot po złej stronie punktu (najpierw trzeba do niego dolecieć).
 */
data class RadialGuide(
    val course: Double, val xtkNm: Double, val alongNm: Double, val track: Double, val heading: Double,
    val wcaDeg: Double, val interceptDeg: Double, val behind: Boolean, val established: Boolean,
)

internal fun radialGuide(radialMag: Int, inbound: Boolean, fix: RadialFix, windFromMag: Double?, windKt: Double?, tasKt: Double?): RadialGuide {
    val off = Math.toRadians(norm180(fix.radialMag - radialMag))
    // Strona zgodna z ruchem wskazówek zegara względem radialu, patrząc od punktu na zewnątrz
    val cw = fix.distNm * sin(off)
    val out = fix.distNm * cos(off)
    val course = if (inbound) norm360(radialMag + 180.0) else radialMag.toDouble()
    val xtk = if (inbound) -cw else cw
    // IN: samolot ma być na radialu przed punktem (out > 0); OUT: za punktem (out > 0) — inaczej jest po złej stronie
    val behind = out < -0.3
    val along = if (inbound) -out else out
    // Kąt przechwycenia rośnie z odchyleniem: ok. 30° na 1 NM, najwyżej 45° (blisko radialu łagodnie)
    val intercept = if (behind) 0.0 else (abs(xtk) * 30.0).coerceAtMost(45.0)
    val track = if (behind) fix.toMag else norm360(course - sign(xtk) * intercept)
    val wca = if (windFromMag != null && windKt != null && tasKt != null && tasKt > 40) {
        val cross = windKt * sin(Math.toRadians(windFromMag - track))
        Math.toDegrees(asin((cross / tasKt).coerceIn(-1.0, 1.0)))
    } else 0.0
    return RadialGuide(course, xtk, along, track, norm360(track + wca), wca, intercept, behind, !behind && abs(xtk) < 0.3)
}
