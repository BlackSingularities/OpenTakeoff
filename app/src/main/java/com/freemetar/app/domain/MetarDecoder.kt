package com.freemetar.app.domain

/** Jedna rozkodowana grupa METAR/TAF: oryginalny token i opis po polsku. */
data class Decoded(val token: String, val category: Category, val text: String)

enum class Category { STATION, TIME, WIND, VISIBILITY, RVR, WEATHER, CLOUDS, TEMP, PRESSURE, TREND, REMARK, OTHER }

object MetarDecoder {

    // Słowniki jako właściwości z getterem — budowane przy każdym użyciu, więc zawsze w bieżącym języku
    private val intensity get() = mapOf("-" to loc("metar_decoder.light"), "+" to loc("metar_decoder.heavy"), "VC" to loc("metar_decoder.in_the_vicinity"))
    private val descriptors get() = mapOf(
        "MI" to loc("metar_decoder.shallow"), "PR" to loc("metar_decoder.partial"), "BC" to loc("metar_decoder.patches_of"), "DR" to loc("metar_decoder.low_drifting"),
        "BL" to loc("metar_decoder.blowing"), "SH" to loc("metar_decoder.showers_of"), "TS" to loc("metar_decoder.thunderstorm_with"), "FZ" to loc("metar_decoder.freezing"),
    )
    private val phenomena get() = mapOf(
        "DZ" to loc("metar_decoder.drizzle"), "RA" to loc("metar_decoder.rain"), "SN" to loc("metar_decoder.snow"), "SG" to loc("metar_decoder.snow_grains"),
        "IC" to loc("metar_decoder.ice_crystals"), "PL" to loc("metar_decoder.ice_pellets"), "GR" to loc("metar_decoder.hail"), "GS" to loc("metar_decoder.small_hail"),
        "UP" to loc("metar_decoder.unknown_precipitation"), "BR" to loc("metar_decoder.mist"), "FG" to loc("metar_decoder.fog"), "FU" to loc("metar_decoder.smoke"),
        "VA" to loc("metar_decoder.volcanic_ash"), "DU" to loc("metar_decoder.dust"), "SA" to loc("metar_decoder.sand"), "HZ" to loc("metar_decoder.haze"),
        "PY" to loc("metar_decoder.spray"), "PO" to loc("metar_decoder.dust_whirls"), "SQ" to loc("metar_decoder.squalls"), "FC" to loc("metar_decoder.funnel_cloud"),
        "SS" to loc("metar_decoder.sandstorm"), "DS" to loc("metar_decoder.dust_storm"),
    )
    private val cover get() = mapOf(
        "FEW" to loc("metar_decoder.few_1_2_8"), "SCT" to loc("metar_decoder.scattered_3_4_8"),
        "BKN" to loc("metar_decoder.broken_5_7_8"), "OVC" to loc("metar_decoder.overcast_8_8"), "VV" to loc("metar_decoder.vertical_visibility"),
    )

    private val reTime = Regex("""^(\d{2})(\d{2})(\d{2})Z$""")
    private val reWind = Regex("""^(\d{3}|VRB)(\d{2,3})(G(\d{2,3}))?(KT|MPS|KMH)$""")
    private val reWindVar = Regex("""^(\d{3})V(\d{3})$""")
    private val reVis = Regex("""^(\d{4})(NDV|N|NE|E|SE|S|SW|W|NW)?$""")
    private val reVisSm = Regex("""^([MP]?)(\d+ )?(\d+)?(/?\d*)SM$""")
    private val reSmFraction = Regex("""^\d/\d{1,2}SM$""")
    private val reRvr = Regex("""^R(\d{2}[LCR]?)/([PM]?)(\d{4})(V([PM]?)(\d{4}))?(FT)?([UDN])?$""")
    private val reWx = Regex("""^(-|\+|VC)?(MI|PR|BC|DR|BL|SH|TS|FZ)?((DZ|RA|SN|SG|IC|PL|GR|GS|UP|BR|FG|FU|VA|DU|SA|HZ|PY|PO|SQ|FC|SS|DS)+)?$""")
    private val reCloud = Regex("""^(FEW|SCT|BKN|OVC|VV)(\d{3}|///)(CB|TCU|///)?$""")
    private val reTemp = Regex("""^(M?\d{2})/(M?\d{2})?$""")
    private val reQ = Regex("""^Q(\d{4})$""")
    private val reA = Regex("""^A(\d{4})$""")
    private val reRecent = Regex("""^RE([A-Z]{2,6})$""")
    private val reWs = Regex("""^WS$""")
    private val reRwyState = Regex("""^R?(\d{2}[LCR]?)/(\d|/)(\d|/)(\d\d|//)(\d\d|//)$""")
    private val reTafValid = Regex("""^(\d{2})(\d{2})/(\d{2})(\d{2})$""")
    private val reFm = Regex("""^FM(\d{2})(\d{2})(\d{2})$""")
    private val reProb = Regex("""^PROB(\d{2})$""")

    /** Dzieli depeszę na grupy; amerykańskie "1 1/2SM" zostaje jedną grupą. */
    fun tokenize(raw: String): List<String> {
        val parts = raw.trim().replace("=", "").split(Regex("\\s+")).filter { it.isNotBlank() }
        val out = ArrayList<String>(parts.size)
        parts.forEachIndexed { i, t ->
            if (i > 0 && reSmFraction.matches(t) && parts[i - 1].length <= 2 && parts[i - 1].all { it.isDigit() } && out.isNotEmpty()) {
                out[out.lastIndex] = out.last() + " " + t
            } else out += t
        }
        return out
    }

    fun decode(raw: String): List<Decoded> {
        val tokens = tokenize(raw)
        val out = ArrayList<Decoded>()
        var inRemarks = false
        tokens.forEachIndexed { i, t ->
            if (inRemarks) {
                out += Decoded(t, Category.REMARK, remark(t)); return@forEachIndexed
            }
            out += when {
                i == 0 && (t == "METAR" || t == "SPECI" || t == "TAF") ->
                    Decoded(t, Category.OTHER, when (t) { "SPECI" -> loc("metar_decoder.special_report_conditions_changed"); "TAF" -> loc("metar_decoder.terminal_aerodrome_forecast"); else -> loc("metar_decoder.routine_metar_report") })
                t == "AMD" -> Decoded(t, Category.OTHER, loc("metar_decoder.amended_forecast"))
                (i <= 1 || (i == 2 && (tokens[1] == "AMD" || tokens[1] == "COR"))) && t.length == 4 && t.all { it.isLetterOrDigit() } && t.first().isLetter() && out.none { it.category == Category.STATION } ->
                    Decoded(t, Category.STATION, loc("metar_decoder.airport", t))
                reTime.matches(t) -> reTime.find(t)!!.destructured.let { (d, h, m) -> Decoded(t, Category.TIME, loc("metar_decoder.day_utc", d, h, m)) }
                t == "AUTO" -> Decoded(t, Category.OTHER, loc("metar_decoder.automated_report_no_observer"))
                t == "COR" -> Decoded(t, Category.OTHER, loc("metar_decoder.corrected_report"))
                t == "NIL" -> Decoded(t, Category.OTHER, loc("metar_decoder.no_report"))
                reWind.matches(t) -> wind(t)
                t == "00000KT" -> Decoded(t, Category.WIND, loc("metar_decoder.calm"))
                reWindVar.matches(t) -> reWindVar.find(t)!!.destructured.let { (a, b) -> Decoded(t, Category.WIND, loc("metar_decoder.direction_varying_between_and", a, b)) }
                t == "CAVOK" -> Decoded(t, Category.VISIBILITY, loc("metar_decoder.cavok_visibility_10_km_no_cloud"))
                t == "9999" -> Decoded(t, Category.VISIBILITY, loc("metar_decoder.visibility_10_km_or_more"))
                // Po temperaturze 4 cyfry to widzialność tylko w trendzie (TEMPO 4000)
                reVis.matches(t) && out.lastOrNull { it.category == Category.TEMP || it.category == Category.TREND }?.category != Category.TEMP ->
                    reVis.find(t)!!.destructured.let { (v, dir) ->
                        val m = v.toInt()
                        val txt = if (m >= 9999) loc("metar_decoder.10_km_or_more") else if (m >= 5000) "${m / 1000} km" else "$m m"
                        Decoded(t, Category.VISIBILITY, when (dir) {
                            "" -> loc("metar_decoder.visibility_2", txt)
                            "NDV" -> loc("metar_decoder.visibility_no_directional_variation", txt)
                            else -> loc("metar_decoder.minimum_visibility_towards", txt, dir)
                        })
                    }
                reVisSm.matches(t) -> Decoded(
                    t, Category.VISIBILITY,
                    when (t.first()) { 'P' -> loc("metar_decoder.visibility_above"); else -> loc("metar_decoder.visibility") } +
                        t.removeSuffix("SM").trimStart('M', 'P') + loc("metar_decoder.statute_miles") + if (t.startsWith("M")) loc("metar_decoder.less_than") else "",
                )
                reRvr.matches(t) -> rvr(t)
                reRwyState.matches(t) && t.contains("/") && t.length >= 8 -> Decoded(t, Category.OTHER, loc("metar_decoder.runway_surface_condition", reRwyState.find(t)!!.groupValues[1]))
                t == "NSC" -> Decoded(t, Category.CLOUDS, loc("metar_decoder.no_significant_cloud"))
                t == "NCD" -> Decoded(t, Category.CLOUDS, loc("metar_decoder.no_cloud_detected_automated_station"))
                t == "SKC" || t == "CLR" -> Decoded(t, Category.CLOUDS, loc("metar_decoder.sky_clear"))
                t == "NSW" -> Decoded(t, Category.WEATHER, loc("metar_decoder.no_significant_weather"))
                reCloud.matches(t) -> cloud(t)
                reTemp.matches(t) -> reTemp.find(t)!!.destructured.let { (a, b) ->
                    val tt = a.replace("M", "-").toInt()
                    val dd = b.takeIf { it.isNotEmpty() }?.replace("M", "-")?.toInt()
                    Decoded(t, Category.TEMP, loc("metar_decoder.temperature_c", tt) + (dd?.let { loc("metar_decoder.dew_point_c_spread", it, tt - it) } ?: ""))
                }
                reQ.matches(t) -> Decoded(t, Category.PRESSURE, "QNH ${t.drop(1).toInt()} hPa (${Atmo.hpaToInHg(t.drop(1).toDouble()).fmt(2)} inHg)")
                reA.matches(t) -> t.drop(1).toInt().let { v -> Decoded(t, Category.PRESSURE, loc("metar_decoder.altimeter") + " ${v / 100}." + "%02d".format(v % 100) + " inHg (${Atmo.inHgToHpa(v / 100.0).fmt()} hPa)") }
                reRecent.matches(t) -> Decoded(t, Category.WEATHER, loc("metar_decoder.recent") + weatherText(t.drop(2)))
                reWs.matches(t) -> Decoded(t, Category.WIND, loc("metar_decoder.wind_shear"))
                t == "NOSIG" -> Decoded(t, Category.TREND, loc("metar_decoder.no_significant_change_expected_within_2"))
                t == "BECMG" -> Decoded(t, Category.TREND, loc("metar_decoder.becoming"))
                t == "TEMPO" -> Decoded(t, Category.TREND, loc("metar_decoder.temporarily"))
                t == "INTER" -> Decoded(t, Category.TREND, loc("metar_decoder.intermittently"))
                reProb.matches(t) -> Decoded(t, Category.TREND, loc("metar_decoder.probability", t.drop(4)))
                reFm.matches(t) -> reFm.find(t)!!.destructured.let { (d, h, m) -> Decoded(t, Category.TREND, loc("metar_decoder.from_day_utc", d, h, m)) }
                reTafValid.matches(t) -> reTafValid.find(t)!!.destructured.let { (d1, h1, d2, h2) -> Decoded(t, Category.TIME, loc("metar_decoder.valid_from_day_00_to_day", d1, h1, d2, h2)) }
                t.startsWith("TX") || t.startsWith("TN") -> Decoded(t, Category.TEMP, (if (t.startsWith("TX")) loc("metar_decoder.max") else loc("metar_decoder.min")) + loc("metar_decoder.temperature") + t.drop(2).substringBefore("/").replace("M", "-") + " °C")
                t == "RMK" -> { inRemarks = true; Decoded(t, Category.REMARK, loc("metar_decoder.remarks")) }
                t.startsWith("AT") || t.startsWith("TL") -> Decoded(t, Category.TREND, loc("metar_decoder.time_utc", t.drop(2)))
                t != "" && reWx.matches(t) && t.length >= 2 -> Decoded(t, Category.WEATHER, weatherText(t))
                else -> Decoded(t, Category.OTHER, "—")
            }
        }
        return out
    }

    private fun wind(t: String): Decoded {
        val (dir, spd, _, gust, unit) = reWind.find(t)!!.destructured
        val u = when (unit) { "MPS" -> "m/s"; "KMH" -> "km/h"; else -> "kt" }
        val speed = spd.toInt()
        if (speed == 0) return Decoded(t, Category.WIND, loc("metar_decoder.calm"))
        val d = if (dir == "VRB") loc("common.variable") else loc("metar_decoder.from", dir)
        val g = if (gust.isNotEmpty()) loc("metar_decoder.gusting", gust.toInt(), u) else ""
        return Decoded(t, Category.WIND, loc("metar_decoder.wind", d, speed, u, g))
    }

    private fun rvr(t: String): Decoded {
        val m = reRvr.find(t)!!.groupValues
        val trend = when (m[8]) { "U" -> loc("metar_decoder.increasing"); "D" -> loc("metar_decoder.decreasing"); "N" -> loc("metar_decoder.no_change"); else -> "" }
        val pre = when (m[2]) { "P" -> loc("metar_decoder.above"); "M" -> loc("metar_decoder.below"); else -> "" }
        val unit = if (m[7] == "FT") "ft" else "m"
        val range = if (m[4].isNotEmpty()) " – ${m[6].toInt()}" else ""
        return Decoded(t, Category.RVR, loc("metar_decoder.rvr_runway", m[1], pre, m[3].toInt(), range, unit, trend))
    }

    private fun cloud(t: String): Decoded {
        val (c, h, type) = reCloud.find(t)!!.destructured
        val height = h.toIntOrNull()?.let { "${it * 100} ft" } ?: loc("metar_decoder.height_unknown")
        val ty = when (type) { "CB" -> " — cumulonimbus!"; "TCU" -> loc("metar_decoder.towering_cumulus"); else -> "" }
        return Decoded(t, Category.CLOUDS, loc("metar_decoder.at", cover[c]?.replaceFirstChar { it.uppercase() }, height, ty))
    }

    /** Czy token jest grupą zjawisk (np. -SHRA, TSRA, BR). */
    fun isWeather(t: String) = t.length >= 2 && reWx.matches(t) && t != "NSW" &&
        (phenomena.keys.any { t.contains(it) } || t.endsWith("TS"))

    private val phenomenaEn = mapOf(
        "DZ" to "drizzle", "RA" to "rain", "SN" to "snow", "SG" to "snow grains", "IC" to "ice crystals", "PL" to "ice pellets",
        "GR" to "hail", "GS" to "small hail", "BR" to "mist", "FG" to "fog", "FU" to "smoke", "HZ" to "haze", "DU" to "dust",
        "SA" to "sand", "SQ" to "squalls", "FC" to "funnel cloud", "SS" to "sandstorm", "DS" to "duststorm", "VA" to "volcanic ash",
    )
    private val descriptorsEn = mapOf(
        "MI" to "shallow", "PR" to "partial", "BC" to "patches of", "DR" to "low drifting", "BL" to "blowing",
        "SH" to "showers of", "TS" to "thunderstorm with", "FZ" to "freezing",
    )

    /** Zjawiska po angielsku dla komunikatu ATIS, np. "-SHRA BR" → "light showers of rain, mist". */
    fun weatherTextEn(wx: String): String = wx.split(" ").filter { it.isNotBlank() }.joinToString(", ") { t ->
        var rest = t
        val parts = mutableListOf<String>()
        when {
            rest.startsWith("+") -> { parts += "heavy"; rest = rest.drop(1) }
            rest.startsWith("-") -> { parts += "light"; rest = rest.drop(1) }
            rest.startsWith("VC") -> { parts += "in the vicinity"; rest = rest.drop(2) }
        }
        descriptorsEn.keys.firstOrNull { rest.startsWith(it) }?.let { parts += descriptorsEn[it]!!; rest = rest.removePrefix(it) }
        rest.chunked(2).forEach { phenomenaEn[it]?.let(parts::add) }
        parts.joinToString(" ").removeSuffix(" with").removeSuffix(" of").ifBlank { t }
    }.replaceFirstChar { it.uppercase() }

    fun weatherText(t: String): String {
        var rest = t
        val intensityText = listOf("+", "-", "VC").firstOrNull { rest.startsWith(it) }?.let { rest = rest.removePrefix(it); intensity[it]!! }
        val descriptorCode = descriptors.keys.firstOrNull { rest.startsWith(it) }?.also { rest = rest.removePrefix(it) }
        val phenomenaText = rest.chunked(2).mapNotNull { phenomena[it] }
        // „showers of” / „thunderstorm with” bez dalszego zjawiska — obcinamy łącznik
        val descriptor = descriptorCode?.let { descriptors[it]!! }?.let { d ->
            if (phenomenaText.isEmpty()) d.removeSuffix(" with").removeSuffix(" of").removeSuffix(" con").removeSuffix(" de") else d
        }
        val parts = if (Lang.current == AppLang.ES) {
            // Po hiszpańsku przymiotnik i intensywność stoją za rzeczownikiem: „lluvia engelante débil”
            val adjective = descriptorCode in setOf("MI", "PR", "FZ")
            listOfNotNull(descriptor.takeUnless { adjective }) + phenomenaText + listOfNotNull(descriptor.takeIf { adjective }, intensityText)
        } else {
            listOfNotNull(intensityText, descriptor) + phenomenaText
        }
        return parts.joinToString(" ").replaceFirstChar { it.uppercase() }.ifBlank { t }
    }

    private fun remark(t: String) = when {
        t == "AO2" -> loc("metar_decoder.automated_station_with_precipitation_sensor")
        t == "AO1" -> loc("metar_decoder.automated_station_without_precipitation_sensor")
        t.startsWith("SLP") -> loc("metar_decoder.sea_level_pressure") + t.drop(3).toIntOrNull()?.let { (if (it < 500) 1000 + it / 10.0 else 900 + it / 10.0).fmt(1) + " hPa" }.orEmpty()
        t.matches(Regex("""T[01]\d{3}[01]\d{3}""")) -> {
            fun v(s: String) = (if (s[0] == '1') -1 else 1) * s.drop(1).toInt() / 10.0
            loc("metar_decoder.precisely_temp_c_dew_point_c", v(t.substring(1, 5)), v(t.substring(5, 9)))
        }
        else -> ""
    }

    /** Dzieli TAF na linie okresów (FM/BECMG/TEMPO/PROB). */
    fun splitTaf(raw: String): List<String> {
        val tokens = raw.trim().split(Regex("\\s+"))
        val lines = mutableListOf(StringBuilder())
        tokens.forEachIndexed { i, t ->
            val starts = reFm.matches(t) || t == "BECMG" || (t == "TEMPO" && i > 0 && !reProb.matches(tokens[i - 1])) || reProb.matches(t)
            if (starts && lines.last().isNotEmpty()) lines += StringBuilder()
            if (lines.last().isNotEmpty()) lines.last().append(' ')
            lines.last().append(t)
        }
        return lines.map { it.toString() }
    }
}
