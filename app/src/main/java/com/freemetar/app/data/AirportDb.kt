package com.freemetar.app.data

import com.freemetar.app.domain.loc

import android.content.Context
import com.freemetar.app.domain.Geo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Baza lotnisk, pasów, częstotliwości i pomocy nawigacyjnych z OurAirports
 * (domena publiczna): https://ourairports.com/data/
 * Pliki CSV są pobierane raz i trzymane w pamięci podręcznej aplikacji.
 */
class AirportDb(context: Context) {
    private val dir = File(context.filesDir, "ourairports").apply { mkdirs() }

    private var airports: List<Airport> = emptyList()
    private var byIdent: Map<String, Airport> = emptyMap()
    private var byIata: Map<String, Airport> = emptyMap()
    private var runways: Map<String, List<Runway>> = emptyMap()
    private var freqs: Map<String, List<Frequency>> = emptyMap()
    private var navaids: List<Navaid> = emptyList()

    val isLoaded get() = airports.isNotEmpty()
    val airportCount get() = airports.size

    fun lastUpdated(): Long? = File(dir, FILES.first()).takeIf { it.exists() }?.lastModified()

    fun hasCache() = FILES.all { File(dir, it).exists() }

    /** Pobiera brakujące lub przeterminowane pliki. */
    suspend fun ensureDownloaded(force: Boolean = false, maxAgeDays: Int = 30, onProgress: (String) -> Unit = {}) {
        val maxAge = maxAgeDays * 24L * 3600 * 1000
        for (name in FILES) {
            val f = File(dir, name)
            val stale = !f.exists() || System.currentTimeMillis() - f.lastModified() > maxAge
            if (force || stale) {
                onProgress(loc("common.downloading", name))
                try {
                    Http.download("$BASE/$name", f)
                } catch (e: Exception) {
                    // Stary plik jest lepszy niż żaden
                    if (!f.exists()) throw e
                }
            }
        }
    }

    suspend fun load() = withContext(Dispatchers.Default) {
        // Lotniska najpierw — budują mapę aliasów potrzebną pasom i częstotliwościom
        val parsed = parseAirports()
        coroutineScope {
            val r = async { parseRunways() }
            val f = async { parseFrequencies() }
            val n = async { parseNavaids() }
            airports = parsed
            byIdent = airports.associateBy { it.ident }
            byIata = airports.filter { it.iata.isNotBlank() }.associateBy { it.iata }
            runways = r.await()
            freqs = f.await()
            navaids = n.await()
        }
    }

    fun airport(ident: String): Airport? = byIdent[ident.uppercase()] ?: byIata[ident.uppercase()]
    fun runways(ident: String): List<Runway> = runways[ident].orEmpty().filterNot { it.closed }
    fun frequencies(ident: String): List<Frequency> = freqs[ident].orEmpty().sortedWith(compareBy({ it.order }, { it.mhz }))

    fun search(query: String, limit: Int = 40): List<Airport> {
        val q = query.trim().uppercase()
        if (q.length < 2) return emptyList()
        val exact = listOfNotNull(byIdent[q], byIata[q])
        val rest = airports.asSequence()
            .filter { it.ident.startsWith(q) || it.iata == q || it.name.uppercase().contains(q) || it.municipality.uppercase().contains(q) }
            .sortedWith(compareBy({ !it.ident.startsWith(q) }, { typeRank(it.type) }, { it.ident }))
            .take(limit)
            .toList()
        return (exact + rest).distinctBy { it.ident }.take(limit)
    }

    fun nearby(lat: Double, lon: Double, radiusNm: Double = 60.0, limit: Int = 12, exclude: String? = null): List<Pair<Airport, Double>> =
        airports.asSequence()
            .filter { it.ident != exclude && it.type in setOf("large_airport", "medium_airport", "small_airport") }
            .filter { kotlin.math.abs(it.lat - lat) < radiusNm / 60.0 + 0.1 }
            .map { it to Geo.distanceNm(lat, lon, it.lat, it.lon) }
            .filter { it.second <= radiusNm }
            .sortedBy { it.second }
            .take(limit)
            .toList()

    fun navaidsNear(lat: Double, lon: Double, radiusNm: Double = 40.0): List<Pair<Navaid, Double>> =
        navaids.asSequence()
            .filter { kotlin.math.abs(it.lat - lat) < radiusNm / 60.0 + 0.1 }
            .map { it to Geo.distanceNm(lat, lon, it.lat, it.lon) }
            .filter { it.second <= radiusNm }
            .sortedBy { it.second }
            .take(20)
            .toList()

    private fun typeRank(t: String) = when (t) {
        "large_airport" -> 0; "medium_airport" -> 1; "small_airport" -> 2; else -> 3
    }

    // --- parsowanie CSV ---

    // OurAirports kluczuje pasy i częstotliwości po "ident"; mapujemy na nasz identyfikator ICAO
    private val identAlias = HashMap<String, String>()

    private fun parseAirports(): List<Airport> = readCsv("airports.csv") { r ->
        val type = r["type"]
        if (type == "closed") return@readCsv null
        val ident = r["icao_code"].ifBlank { r["gps_code"].ifBlank { r["ident"] } }.uppercase()
        if (ident != r["ident"]) identAlias[r["ident"]] = ident
        Airport(
            ident = ident,
            type = type,
            name = r["name"],
            lat = r["latitude_deg"].toDoubleOrNull() ?: return@readCsv null,
            lon = r["longitude_deg"].toDoubleOrNull() ?: return@readCsv null,
            elevationFt = r["elevation_ft"].toIntOrNull(),
            country = r["iso_country"],
            region = r["iso_region"],
            municipality = r["municipality"],
            iata = r["iata_code"].uppercase(),
            scheduled = r["scheduled_service"] == "yes",
            wikipedia = r["wikipedia_link"],
            homeLink = r["home_link"],
        )
    }

    private fun key(rawIdent: String) = identAlias[rawIdent] ?: rawIdent

    private fun parseRunways(): Map<String, List<Runway>> = readCsv("runways.csv") { r ->
        key(r["airport_ident"]) to Runway(
            lengthFt = r["length_ft"].toIntOrNull(),
            widthFt = r["width_ft"].toIntOrNull(),
            surface = r["surface"],
            lighted = r["lighted"] == "1",
            closed = r["closed"] == "1",
            le = end(r, "le"),
            he = end(r, "he"),
        )
    }.groupBy({ it.first }, { it.second })

    private fun end(r: Row, p: String) = RunwayEnd(
        ident = r["${p}_ident"],
        headingTrue = r["${p}_heading_degT"].toDoubleOrNull(),
        elevationFt = r["${p}_elevation_ft"].toIntOrNull(),
        displacedFt = r["${p}_displaced_threshold_ft"].toIntOrNull(),
        lat = r["${p}_latitude_deg"].toDoubleOrNull(),
        lon = r["${p}_longitude_deg"].toDoubleOrNull(),
    )

    private fun parseFrequencies(): Map<String, List<Frequency>> = readCsv("airport-frequencies.csv") { r ->
        val mhz = r["frequency_mhz"].toDoubleOrNull() ?: return@readCsv null
        key(r["airport_ident"]) to Frequency(r["type"], r["description"], mhz)
    }.groupBy({ it.first }, { it.second })

    private fun parseNavaids(): List<Navaid> = readCsv("navaids.csv") { r ->
        Navaid(
            ident = r["ident"],
            name = r["name"],
            type = r["type"],
            frequencyKhz = r["frequency_khz"].toIntOrNull(),
            lat = r["latitude_deg"].toDoubleOrNull() ?: return@readCsv null,
            lon = r["longitude_deg"].toDoubleOrNull() ?: return@readCsv null,
            associatedAirport = r["associated_airport"],
        )
    }

    class Row(private val header: Map<String, Int>, private val cells: List<String>) {
        operator fun get(col: String): String = header[col]?.let { cells.getOrNull(it) }.orEmpty()
    }

    private fun <T> readCsv(name: String, map: (Row) -> T?): List<T> {
        val f = File(dir, name)
        if (!f.exists()) return emptyList()
        val out = ArrayList<T>()
        f.bufferedReader().use { br ->
            val header = Csv.split(br.readLine() ?: return out).withIndex().associate { it.value to it.index }
            var line = br.readLine()
            while (line != null) {
                // Pole w cudzysłowie może zawierać znak nowej linii
                var full: String = line
                while (full.count { it == '"' } % 2 == 1) {
                    val next = br.readLine() ?: break
                    full += "\n" + next
                }
                map(Row(header, Csv.split(full)))?.let(out::add)
                line = br.readLine()
            }
        }
        return out
    }

    companion object {
        private const val BASE = "https://davidmegginson.github.io/ourairports-data"
        val FILES = listOf("airports.csv", "runways.csv", "airport-frequencies.csv", "navaids.csv")
    }
}

object Csv {
    fun split(line: String): List<String> {
        val out = ArrayList<String>(20)
        val sb = StringBuilder()
        var inQuotes = false
        var i = 0
        while (i < line.length) {
            val c = line[i]
            when {
                c == '"' && inQuotes && i + 1 < line.length && line[i + 1] == '"' -> { sb.append('"'); i++ }
                c == '"' -> inQuotes = !inQuotes
                c == ',' && !inQuotes -> { out.add(sb.toString()); sb.setLength(0) }
                else -> sb.append(c)
            }
            i++
        }
        out.add(sb.toString())
        return out
    }
}
