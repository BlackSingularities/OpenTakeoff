package com.freemetar.app.domain

import com.freemetar.app.data.Metar
import com.freemetar.app.data.Runway
import java.time.Instant
import java.time.ZoneOffset
import kotlin.math.roundToInt

/** Kategoria lotu wg kryteriów NWS: pułap (ft) i widzialność (m). */
object FlightCategory {
    fun of(visM: Int?, ceilingFt: Int?): String? {
        if (visM == null && ceilingFt == null) return null
        val v = visM ?: 99999
        val c = ceilingFt ?: 99999
        return when {
            c < 500 || v < 1600 -> "LIFR"
            c < 1000 || v < 4800 -> "IFR"
            c <= 3000 || v <= 8047 -> "MVFR" // 5 SM = 8047 m
            else -> "VFR"
        }
    }
}

enum class PeriodKind { BASE, FROM, BECOMING, TEMPO, PROB }

/** Jeden okres prognozy TAF z wyliczoną kategorią lotu. */
data class TafPeriod(
    val kind: PeriodKind,
    val probability: Int?,
    val fromDay: Int?, val fromHour: Int?,
    val toDay: Int?, val toHour: Int?,
    val raw: String,
    val windText: String?,
    val visM: Int?,
    val ceilingFt: Int?,
    val weather: String?,
    val category: String?,
) {
    val title: String
        get() {
            val span = when {
                fromHour != null && toHour != null -> "%02d–%02dZ".format(fromHour, toHour)
                fromHour != null -> loc("briefing.from_02dz").format(fromHour)
                else -> ""
            }
            return when (kind) {
                PeriodKind.BASE -> loc("briefing.main_forecast", span)
                // span zaczyna się już od „od …” / „from …”, więc tylko wielka litera
                PeriodKind.FROM -> span.replaceFirstChar { it.uppercase() }
                PeriodKind.BECOMING -> loc("briefing.becoming", span)
                PeriodKind.TEMPO -> loc("briefing.temporarily", span)
                PeriodKind.PROB -> loc("briefing.chance") + " ${probability ?: ""}% $span"
            }.trim()
        }
}

object TafAnalyzer {
    private val reValid = Regex("""^(\d{2})(\d{2})/(\d{2})(\d{2})$""")
    private val reFm = Regex("""^FM(\d{2})(\d{2})(\d{2})$""")
    private val reWind = Regex("""^(\d{3}|VRB)(\d{2,3})(G(\d{2,3}))?KT$""")
    private val reCloud = Regex("""^(BKN|OVC|VV)(\d{3})""")
    private val reAnyCloud = Regex("""^((FEW|SCT|BKN|OVC|VV)(\d{3}|///)|SKC|CLR|NSC|NCD)""")
    private val reVis = Regex("""^\d{4}$""")
    private val reSm = Regex("""^(P?)(\d+ )?(\d+)?(/?\d*)SM$""")

    fun periods(raw: String): List<TafPeriod> {
        val lines = MetarDecoder.splitTaf(raw)
        var base: TafPeriod? = null
        return lines.mapIndexed { i, line ->
            val t = MetarDecoder.tokenize(line)
            val kind = when {
                i == 0 -> PeriodKind.BASE
                t.first().startsWith("FM") -> PeriodKind.FROM
                t.first() == "BECMG" -> PeriodKind.BECOMING
                t.first().startsWith("PROB") -> PeriodKind.PROB
                else -> PeriodKind.TEMPO
            }
            val prob = t.firstOrNull { it.startsWith("PROB") }?.drop(4)?.toIntOrNull()
            var fd: Int? = null; var fh: Int? = null; var td: Int? = null; var th: Int? = null
            t.firstNotNullOfOrNull { reValid.find(it) }?.destructured?.let { (a, b, c, d) ->
                fd = a.toInt(); fh = b.toInt(); td = c.toInt(); th = d.toInt()
            }
            t.firstNotNullOfOrNull { reFm.find(it) }?.destructured?.let { (a, b, _) -> fd = a.toInt(); fh = b.toInt() }

            val wind = t.firstOrNull { reWind.matches(it) }?.let { w ->
                val (d, s, _, g) = reWind.find(w)!!.destructured
                (if (d == "VRB") loc("common.variable") else "$d°") + " $s kt" + if (g.isNotEmpty()) loc("briefing.gusts", g) else ""
            }
            val vis = when {
                t.contains("CAVOK") -> 10000
                else -> t.firstOrNull { reVis.matches(it) }?.toInt()
                    ?: t.firstOrNull { reSm.matches(it) }?.let { smToM(it) }
            }
            val ceiling = when {
                t.contains("CAVOK") || t.contains("NSC") -> null
                else -> t.mapNotNull { reCloud.find(it)?.groupValues?.get(2)?.toIntOrNull() }.minOrNull()?.times(100)
            }
            val wx = t.filter { MetarDecoder.isWeather(it) }.joinToString(", ") { MetarDecoder.weatherText(it).lowercase() }.ifBlank { null }

            // Okresy zmian dziedziczą brakujące elementy z prognozy głównej
            val effVis = vis ?: base?.visM
            // Każda grupa chmur (też FEW/SCT) zastępuje zachmurzenie z prognozy głównej
            val effCeil = if (t.contains("CAVOK") || t.any { reAnyCloud.containsMatchIn(it) }) ceiling else base?.ceilingFt
            TafPeriod(
                kind, prob, fd, fh, td, th, line, wind, vis, ceiling, wx,
                FlightCategory.of(effVis, effCeil),
            ).also { if (i == 0) base = it }
        }
    }

    private fun smToM(s: String): Int? {
        val v = s.removeSuffix("SM").removePrefix("P")
        fun part(p: String): Double? = when {
            p.contains("/") -> p.split("/").let { (a, b) -> a.toDoubleOrNull()?.div(b.toDoubleOrNull() ?: return null) }
            else -> p.toDoubleOrNull()
        }
        // "1 1/2" = 1 + 1/2
        val sm = v.split(" ").map { part(it) ?: return null }.sum()
        return (sm * 1609).roundToInt()
    }
}

/**
 * Generuje komunikat w stylu ATIS (po angielsku, jak w sieciach VATSIM/IVAO)
 * do odczytu przez syntezator mowy.
 */
object Atis {
    private val digits = listOf("zero", "one", "two", "tree", "four", "fife", "six", "seven", "eight", "niner")
    private val alphabet = listOf(
        "Alfa", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliett", "Kilo", "Lima", "Mike",
        "November", "Oscar", "Papa", "Quebec", "Romeo", "Sierra", "Tango", "Uniform", "Victor", "Whiskey", "X-ray", "Yankee", "Zulu",
    )
    private val cover = mapOf("FEW" to "few", "SCT" to "scattered", "BKN" to "broken", "OVC" to "overcast", "VV" to "vertical visibility")

    fun spell(n: String) = n.map { if (it.isDigit()) digits[it - '0'] else it.toString() }.joinToString(" ")

    /** Litera informacji zależna od godziny obserwacji — zmienia się co godzinę. */
    fun letter(m: Metar): String {
        val h = Instant.ofEpochSecond(m.obsTimeEpoch).atZone(ZoneOffset.UTC).hour
        return alphabet[h % 26]
    }

    private fun feet(ft: Int): String {
        val th = ft / 1000; val hu = (ft % 1000) / 100
        return listOfNotNull(
            th.takeIf { it > 0 }?.let { "${spell(it.toString())} thousand" },
            hu.takeIf { it > 0 }?.let { "${digits[it]} hundred" },
        ).joinToString(" ").ifBlank { "surface" } + " feet"
    }

    fun text(name: String, m: Metar, runways: List<Runway>): String {
        val l = letter(m)
        val time = Instant.ofEpochSecond(m.obsTimeEpoch).atZone(ZoneOffset.UTC).let { "%02d%02d".format(it.hour, it.minute) }
        val rwy = Wind.analyze(runways, m.windDir, m.windKt, m.gustKt).firstOrNull()?.runwayEnd
        // Tylko część główna — widzialność z TEMPO/BECMG to nie bieżące warunki
        val tokens = m.raw.split(" ").takeWhile { it !in setOf("TEMPO", "BECMG", "NOSIG", "RMK") }
        return buildList {
            add("$name information $l, time ${spell(time)} zulu.")
            if (rwy != null) {
                val num = spell(rwy.takeWhile { it.isDigit() })
                val side = when (rwy.dropWhile { it.isDigit() }) { "L" -> " left"; "R" -> " right"; "C" -> " center"; else -> "" }
                add("Runway in use $num$side.")
            }
            add(
                when {
                    (m.windKt ?: 0) == 0 -> "Wind calm."
                    m.windVariable -> "Wind variable, ${spell(m.windKt.toString())} knots."
                    else -> "Wind ${spell("%03d".format(m.windDir ?: 0))} degrees, ${spell(m.windKt.toString())} knots" +
                        (m.gustKt?.let { ", gusting ${spell(it.toString())}" } ?: "") + "."
                }
            )
            when {
                "CAVOK" in tokens -> add("CAVOK.")
                else -> tokens.firstOrNull { it.matches(Regex("""\d{4}(NDV)?""")) }?.take(4)?.toInt()?.let { v ->
                    add(if (v >= 9999) "Visibility one zero kilometers or more." else if (v >= 5000) "Visibility ${spell((v / 1000).toString())} kilometers." else "Visibility ${spell(v.toString())} meters.")
                }
            }
            m.wx?.let { add(MetarDecoder.weatherTextEn(it) + ".") }
            if (m.clouds.isNotEmpty() && "CAVOK" !in tokens) {
                add("Clouds " + m.clouds.joinToString(", ") { c ->
                    "${cover[c.cover] ?: c.cover} ${c.baseFt?.let { feet(it) } ?: ""}" + when (c.type) { "CB" -> " cumulonimbus"; "TCU" -> " towering cumulus"; else -> "" }
                } + ".")
            }
            if (m.tempC != null) {
                fun t(v: Double) = (if (v < 0) "minus " else "") + spell(kotlin.math.abs(v.roundToInt()).toString())
                add("Temperature ${t(m.tempC)}" + (m.dewpointC?.let { ", dew point ${t(it)}" } ?: "") + ".")
            }
            m.altimHpa?.let { add("QNH ${spell(it.roundToInt().toString())}.") }
            add("Advise on initial contact you have information $l.")
        }.joinToString(" ")
    }
}
