package com.freemetar.app.data

import com.freemetar.app.domain.loc

import com.freemetar.app.domain.Geo
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull

data class LatLon(val lat: Double, val lon: Double)

/** SIGMET (krajowy USA lub międzynarodowy) z obszarem i zakresem wysokości. */
data class Sigmet(
    val id: String,
    val fir: String,
    val hazard: String,
    val qualifier: String?,
    val baseFt: Int?,
    val topFt: Int?,
    val validFrom: Long,
    val validTo: Long,
    val area: List<LatLon>,
    val raw: String,
) {
    val hazardLabel: String
        get() = when (hazard.uppercase()) {
            "TS", "CONVECTIVE", "CONV" -> loc("hazards.thunderstorms")
            "TURB" -> loc("hazards.turbulence")
            "ICE" -> loc("hazards.icing")
            "MTW" -> loc("hazards.mountain_wave")
            "VA" -> loc("hazards.volcanic_ash")
            "TC" -> loc("hazards.tropical_cyclone")
            "DS" -> loc("hazards.dust_storm")
            "SS" -> loc("hazards.sandstorm")
            "IFR" -> loc("common.ifr_conditions")
            else -> hazard
        } + (qualifier?.takeIf { it.isNotBlank() && it.length <= 6 }?.let { " ($it)" } ?: "")

    val levels: String
        get() = when {
            baseFt == null && topFt == null -> loc("hazards.levels_not_specified")
            (baseFt ?: 0) == 0 -> loc("hazards.surface_to", fl(topFt))
            else -> "${fl(baseFt)}–${fl(topFt)}"
        }

    private fun fl(ft: Int?) = ft?.let { "FL%03d".format(it / 100) } ?: "?"

    fun contains(p: LatLon) = area.size >= 3 && Geo.pointInPolygon(p, area)

    /** Odległość punktu od obszaru w NM (0 = wewnątrz). Przybliżenie po wierzchołkach i krawędziach. */
    fun distanceNm(p: LatLon): Double = if (contains(p)) 0.0 else Geo.distanceToPolygonNm(p, area)
}

/** Raport pilota (PIREP/AIREP). */
data class Pirep(
    val raw: String,
    val lat: Double,
    val lon: Double,
    val flightLevel: Int?,
    val aircraft: String?,
    val obsTime: Long,
    val turbulence: String?,
    val icing: String?,
    val urgent: Boolean,
) {
    /** Czy raport zgłasza jakieś zjawisko (NEG = pilot potwierdza jego brak). */
    val reportsHazard: Boolean
        get() = listOfNotNull(turbulence, icing).any { !it.contains("NEG") }

    val summary: String
        get() = listOfNotNull(
            turbulence?.let { if (it.contains("NEG")) loc("hazards.no_turbulence") else "turbulencja ${intensity(it, neuter = false)}" },
            icing?.let { if (it.contains("NEG")) loc("hazards.no_icing") else "oblodzenie ${intensity(it, neuter = true)}" },
        ).joinToString(", ").ifBlank { loc("hazards.nothing_reported") }

    // turbulencja (r. żeński) / oblodzenie (r. nijaki)
    private fun intensity(s: String, neuter: Boolean) = when {
        s.contains("SEV") -> if (neuter) loc("hazards.severe_2") else loc("hazards.severe")
        s.contains("MOD") -> if (neuter) loc("hazards.moderate_2") else loc("hazards.moderate")
        s.contains("LGT") -> if (neuter) loc("hazards.light_2") else loc("hazards.light")
        else -> s.lowercase()
    }
}

object HazardParser {
    private val json = Json { ignoreUnknownKeys = true; isLenient = true }

    fun sigmets(body: String): List<Sigmet> {
        if (body.isBlank()) return emptyList()
        return json.parseToJsonElement(body).jsonArray.mapNotNull { e ->
            runCatching {
                val o = e.jsonObject
                val area = (o["coords"] as? JsonArray)?.mapNotNull { c ->
                    val co = c.jsonObject
                    val lat = co.num("lat") ?: return@mapNotNull null
                    val lon = co.num("lon") ?: return@mapNotNull null
                    LatLon(lat, lon)
                }.orEmpty()
                Sigmet(
                    id = listOfNotNull(o.str("icaoId") ?: o.str("firId"), o.str("seriesId")).joinToString(" "),
                    fir = o.str("firName") ?: o.str("firId") ?: "",
                    hazard = o.str("hazard") ?: "?",
                    qualifier = o.str("qualifier"),
                    baseFt = o.num("base")?.toInt() ?: o.num("altitudeLow1")?.toInt(),
                    topFt = o.num("top")?.toInt() ?: o.num("altitudeHi1")?.toInt(),
                    validFrom = o.long("validTimeFrom") ?: 0L,
                    validTo = o.long("validTimeTo") ?: 0L,
                    area = area,
                    raw = o.str("rawSigmet") ?: o.str("rawAirSigmet") ?: "",
                )
            }.getOrNull()
        }
    }

    fun pireps(body: String): List<Pirep> {
        if (body.isBlank()) return emptyList()
        return json.parseToJsonElement(body).jsonArray.mapNotNull { e ->
            runCatching {
                val o = e.jsonObject
                val raw = o.str("rawOb") ?: return@runCatching null
                Pirep(
                    raw = raw,
                    lat = o.num("lat") ?: return@runCatching null,
                    lon = o.num("lon") ?: return@runCatching null,
                    flightLevel = o.num("fltLvl")?.toInt(),
                    aircraft = o.str("acType"),
                    obsTime = o.long("obsTime") ?: 0L,
                    turbulence = o.str("tbInt1")?.takeIf { it.isNotBlank() },
                    icing = o.str("icgInt1")?.takeIf { it.isNotBlank() },
                    urgent = raw.contains(" UUA ") || o.str("pirepType") == "UUA",
                )
            }.getOrNull()
        }
    }

    private fun JsonObject.str(k: String): String? = (this[k] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() && it != "null" }
    private fun JsonObject.num(k: String): Double? = (this[k] as? JsonPrimitive)?.let { it.doubleOrNull ?: it.contentOrNull?.toDoubleOrNull() }
    private fun JsonObject.long(k: String): Long? = (this[k] as? JsonPrimitive)?.let { it.longOrNull ?: it.doubleOrNull?.toLong() }
}
