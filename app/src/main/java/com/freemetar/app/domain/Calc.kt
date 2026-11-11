package com.freemetar.app.domain

import com.freemetar.app.data.Runway
import java.time.LocalDate
import java.time.ZoneOffset
import java.time.ZonedDateTime
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.acos
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.math.tan

private fun Double.rad() = this * PI / 180.0
private fun Double.deg() = this * 180.0 / PI

object Geo {
    fun distanceNm(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val dLat = (lat2 - lat1).rad()
        val dLon = (lon2 - lon1).rad()
        val a = sin(dLat / 2).pow(2) + cos(lat1.rad()) * cos(lat2.rad()) * sin(dLon / 2).pow(2)
        return 2 * 3440.065 * asin(sqrt(a))
    }

    fun bearing(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val y = sin((lon2 - lon1).rad()) * cos(lat2.rad())
        val x = cos(lat1.rad()) * sin(lat2.rad()) - sin(lat1.rad()) * cos(lat2.rad()) * cos((lon2 - lon1).rad())
        return (atan2(y, x).deg() + 360) % 360
    }

    /** Test promienia (ray casting) w układzie lat/lon — wystarczający dla obszarów SIGMET. */
    fun pointInPolygon(p: com.freemetar.app.data.LatLon, poly: List<com.freemetar.app.data.LatLon>): Boolean {
        var inside = false
        var j = poly.size - 1
        for (i in poly.indices) {
            val a = poly[i]; val b = poly[j]
            if ((a.lat > p.lat) != (b.lat > p.lat) &&
                p.lon < (b.lon - a.lon) * (p.lat - a.lat) / (b.lat - a.lat) + a.lon
            ) inside = !inside
            j = i
        }
        return inside
    }

    /** Najmniejsza odległość punktu od krawędzi wielokąta (NM), próbkując każdą krawędź. */
    fun distanceToPolygonNm(p: com.freemetar.app.data.LatLon, poly: List<com.freemetar.app.data.LatLon>): Double {
        if (poly.isEmpty()) return Double.MAX_VALUE
        var best = Double.MAX_VALUE
        for (i in poly.indices) {
            val a = poly[i]; val b = poly[(i + 1) % poly.size]
            for (k in 0..10) {
                val t = k / 10.0
                val d = distanceNm(p.lat, p.lon, a.lat + (b.lat - a.lat) * t, a.lon + (b.lon - a.lon) * t)
                if (d < best) best = d
            }
        }
        return best
    }

    fun compass(deg: Double): String {
        val dirs = listOf("N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW")
        return dirs[((deg / 22.5) + 0.5).toInt() % 16]
    }

    fun formatLatLon(lat: Double, lon: Double): String {
        fun f(v: Double, pos: Char, neg: Char): String {
            val a = abs(v); val d = a.toInt(); val m = (a - d) * 60
            return "%d°%06.3f'%c".format(d, m, if (v >= 0) pos else neg)
        }
        return "${f(lat, 'N', 'S')} ${f(lon, 'E', 'W')}"
    }
}

data class WindComponents(val headwind: Double, val crosswind: Double) {
    /** Dodatni = wiatr z prawej. */
    val crossSide get() = if (crosswind >= 0) loc("calc.from_the_right") else loc("calc.from_the_left")
}

data class RunwayWind(val runwayEnd: String, val heading: Double, val steady: WindComponents, val gust: WindComponents?)

object Wind {
    fun components(windDir: Double, windSpeed: Double, runwayHeading: Double): WindComponents {
        val angle = (windDir - runwayHeading).rad()
        return WindComponents(headwind = windSpeed * cos(angle), crosswind = windSpeed * sin(angle))
    }

    fun headingOf(endIdent: String, headingTrue: Double?): Double? =
        headingTrue ?: endIdent.takeWhile { it.isDigit() }.toIntOrNull()?.let { (it * 10.0) }

    /** Wszystkie końce pasów posortowane od najkorzystniejszego (największy wiatr czołowy). */
    fun analyze(runways: List<Runway>, windDir: Int?, windKt: Int?, gustKt: Int?): List<RunwayWind> {
        if (windDir == null || windKt == null) return emptyList()
        return runways.flatMap { listOf(it.le, it.he) }
            .mapNotNull { end ->
                val hdg = headingOf(end.ident, end.headingTrue) ?: return@mapNotNull null
                RunwayWind(
                    runwayEnd = end.ident,
                    heading = hdg,
                    steady = components(windDir.toDouble(), windKt.toDouble(), hdg),
                    gust = gustKt?.let { components(windDir.toDouble(), it.toDouble(), hdg) },
                )
            }
            .distinctBy { it.runwayEnd }
            .sortedByDescending { it.steady.headwind }
    }
}

object Atmo {
    fun hpaToInHg(h: Double) = h * 0.0295299830714
    fun inHgToHpa(i: Double) = i / 0.0295299830714
    fun ftToM(ft: Double) = ft * 0.3048
    fun mToFt(m: Double) = m / 0.3048
    fun cToF(c: Double) = c * 9 / 5 + 32
    fun fToC(f: Double) = (f - 32) * 5 / 9
    fun ktToKmh(kt: Double) = kt * 1.852
    fun ktToMs(kt: Double) = kt * 0.514444
    fun smToKm(sm: Double) = sm * 1.609344
    fun nmToKm(nm: Double) = nm * 1.852

    fun pressureAltitude(elevFt: Double, qnhHpa: Double) = elevFt + (1013.25 - qnhHpa) * 27.0

    fun isaTemp(pressureAltFt: Double) = 15.0 - 1.98 * pressureAltFt / 1000.0

    /** Wysokość gęstościowa (przybliżenie stosowane w podręcznikach PPL). */
    fun densityAltitude(elevFt: Double, qnhHpa: Double, oatC: Double): Double {
        val pa = pressureAltitude(elevFt, qnhHpa)
        return pa + 120.0 * (oatC - isaTemp(pa))
    }

    /** Wilgotność względna z równania Magnusa. */
    fun relativeHumidity(tC: Double, dpC: Double): Double {
        fun es(t: Double) = exp(17.625 * t / (243.04 + t))
        return (100 * es(dpC) / es(tC)).coerceIn(0.0, 100.0)
    }

    /** Szacowana podstawa chmur konwekcyjnych nad terenem (ft). */
    fun cloudBaseFt(tC: Double, dpC: Double) = ((tC - dpC) / 2.5 * 1000).coerceAtLeast(0.0)

    /** QFE z QNH dla danej elewacji. */
    fun qfe(qnhHpa: Double, elevFt: Double) = qnhHpa - elevFt / 27.0

    /** Prędkość rzeczywista TAS z IAS (przybliżenie 2%/1000 ft wysokości gęstościowej). */
    fun tas(ias: Double, densityAltFt: Double) = ias * (1 + 0.02 * densityAltFt / 1000.0)

    /** Temperatura odczuwalna / ryzyko oblodzenia gaźnika — prosta klasyfikacja. */
    fun carbIceRisk(tC: Double, dpC: Double): String {
        val spread = tC - dpC
        return when {
            tC in -10.0..30.0 && spread <= 5 -> loc("calc.high")
            tC in -10.0..30.0 && spread <= 12 -> loc("calc.moderate")
            else -> loc("calc.low")
        }
    }

    fun isFreezingWithMoisture(tC: Double, dpC: Double) = tC in -15.0..2.0 && tC - dpC <= 3
}

object Sun {
    data class Times(val sunrise: ZonedDateTime?, val sunset: ZonedDateTime?, val civilDawn: ZonedDateTime?, val civilDusk: ZonedDateTime?)

    /** Algorytm NOAA; zwraca czasy UTC. */
    fun times(date: LocalDate, lat: Double, lon: Double): Times {
        fun calc(zenith: Double, rising: Boolean): ZonedDateTime? {
            val n = date.dayOfYear
            val lngHour = lon / 15.0
            val t = n + ((if (rising) 6.0 else 18.0) - lngHour) / 24.0
            val m = 0.9856 * t - 3.289
            var l = m + 1.916 * sin(m.rad()) + 0.020 * sin((2 * m).rad()) + 282.634
            l = (l + 360) % 360
            var ra = atan(0.91764 * tan(l.rad())).deg()
            ra = (ra + 360) % 360
            ra += (Math.floor(l / 90) * 90 - Math.floor(ra / 90) * 90)
            ra /= 15.0
            val sinDec = 0.39782 * sin(l.rad())
            val cosDec = cos(asin(sinDec))
            val cosH = (cos(zenith.rad()) - sinDec * sin(lat.rad())) / (cosDec * cos(lat.rad()))
            if (cosH > 1 || cosH < -1) return null
            val h = (if (rising) 360 - acos(cosH).deg() else acos(cosH).deg()) / 15.0
            val localT = h + ra - 0.06571 * t - 6.622
            val ut = ((localT - lngHour) % 24 + 24) % 24
            val minutes = (ut * 60).roundToInt()
            return date.atStartOfDay(ZoneOffset.UTC).plusMinutes(minutes.toLong())
        }
        return Times(calc(90.833, true), calc(90.833, false), calc(96.0, true), calc(96.0, false))
    }

    private fun atan(x: Double) = kotlin.math.atan(x)
}

fun Double.fmt(d: Int = 0): String = "%.${d}f".format(this)

/** Tysiące oddzielone spacją w każdym języku: 12 000. */
fun Int.grouped(): String = "%,d".format(java.util.Locale.US, this).replace(',', ' ')
