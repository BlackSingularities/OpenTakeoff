package com.freemetar.app.data.charts

import com.freemetar.app.domain.loc

import android.content.Context
import com.freemetar.app.data.Http
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import org.xml.sax.Attributes
import org.xml.sax.helpers.DefaultHandler
import java.io.File
import java.io.FilterInputStream
import java.io.IOException
import java.io.InputStream
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import java.time.temporal.ChronoUnit
import javax.xml.parsers.SAXParserFactory

/**
 * Bezpłatne karty FAA (d-TPP) dla lotnisk w USA.
 *
 * Źródło: aeronav.faa.gov/d-tpp/<cykl>/ — metadane cyklu w xml_data/d-TPP_Metafile.xml (~16 MB),
 * karty jako osobne PDF-y (100–500 kB). Cykl to numer AIRAC (RRNN, np. 2610), zmiana co 28 dni o 0901Z.
 */
enum class ChartType(private val labelOf: () -> String) {
    APD({ loc("faa_charts.airport_diagram") }),
    IAP({ loc("faa_charts.approaches") }),
    DP({ loc("faa_charts.departures") }),
    STAR({ loc("faa_charts.arrivals_star") }),
    MIN({ "Minima" }),
    HOT({ "Hot spots" });

    val label: String get() = labelOf()

    companion object {
        /** Kod z metapliku → typ. ODP i DAU to odloty, LAH (LAHSO) trafia do minimów. */
        fun fromCode(code: String): ChartType? = when (code.trim().uppercase()) {
            "APD" -> APD
            "IAP" -> IAP
            "DP", "ODP", "DAU" -> DP
            "STR" -> STAR
            "MIN", "LAH" -> MIN
            "HOT" -> HOT
            else -> null
        }
    }
}

/**
 * Jedna karta. [pdfs] ma kilka plików, gdy karta ma strony „CONT.n” (np. długie SID-y).
 * [cycle] uzupełnia [FaaCharts] przy odczycie — nie jest zapisywany w indeksie.
 */
@Serializable
data class Chart(
    val airport: String,
    val type: ChartType,
    val name: String,
    val pdfs: List<String>,
    val changed: Boolean = false,
    val amendment: String? = null,
    val cycle: String = "",
)

@Serializable
data class ChartAirport(
    val ident: String,
    val icao: String? = null,
    val name: String,
    val city: String = "",
    val state: String = "",
    val military: Boolean = false,
    val charts: List<Chart>,
)

/** Indeks cyklu: lotniska po identyfikatorze FAA i ICAO. Zapisywany jako filesDir/charts/<cykl>/index.json. */
@Serializable
data class DtppIndex(
    val cycle: String,
    val from: String? = null,
    val to: String? = null,
    val airports: List<ChartAirport>,
) {
    private val byIdent by lazy { airports.associateBy { it.ident } }
    private val byIcao by lazy { airports.filter { !it.icao.isNullOrEmpty() }.associateBy { it.icao!! } }

    /** Lotnisko po ICAO (KJFK, PAJN) albo po identyfikatorze FAA (JFK, 79J). */
    fun airport(code: String): ChartAirport? {
        val k = code.trim().uppercase()
        return byIcao[k] ?: byIdent[k] ?: if (k.length == 4 && k[0] == 'K') byIdent[k.substring(1)] else null
    }

    fun charts(code: String): List<Chart> = airport(code)?.charts?.map { it.copy(cycle = cycle) }.orEmpty()

    /** ICAO → identyfikator FAA (KJFK → JFK, PAJN → JNU). */
    fun faaIdent(icao: String): String? = airport(icao)?.ident

    /** Identyfikator FAA → ICAO (JNU → PAJN); null dla lotnisk bez kodu ICAO (np. 79J). */
    fun icaoFor(ident: String): String? = airport(ident)?.icao

    fun contains(code: String): Boolean = airport(code) != null
}

/** Numeracja cykli AIRAC — d-TPP używa tych samych identyfikatorów (RRNN). */
object DtppCycle {
    /** AIRAC 2001 wszedł w życie 2 stycznia 2020; kolejne co 28 dni. */
    private val EPOCH: LocalDate = LocalDate.of(2020, 1, 2)
    private const val DAYS = 28L

    /** Data wejścia w życie cyklu obowiązującego w [date]. */
    fun effective(date: LocalDate): LocalDate {
        val n = Math.floorDiv(ChronoUnit.DAYS.between(EPOCH, date), DAYS)
        return EPOCH.plusDays(n * DAYS)
    }

    /** Identyfikator cyklu obowiązującego w [date], np. 2026-10-18 → "2610". */
    fun idFor(date: LocalDate): String {
        val eff = effective(date)
        val jan1 = LocalDate.of(eff.year, 1, 1)
        val first = EPOCH.plusDays(Math.floorDiv(ChronoUnit.DAYS.between(EPOCH, jan1) + DAYS - 1, DAYS) * DAYS)
        val num = ChronoUnit.DAYS.between(first, eff) / DAYS + 1
        return "%02d%02d".format(eff.year % 100, num)
    }

    /** Cykl obowiązujący w chwili [now] (zmiana o 0901Z w dniu wejścia w życie). */
    fun current(now: Instant = Instant.now()): String = idFor(utcDay(now))

    /** Cykl poprzedzający obowiązujący w chwili [now]. */
    fun previous(now: Instant = Instant.now()): String = idFor(effective(utcDay(now)).minusDays(DAYS))

    private fun utcDay(now: Instant) = now.minusSeconds(9 * 3600L + 60).atOffset(ZoneOffset.UTC).toLocalDate()

    fun base(cycle: String) = "https://aeronav.faa.gov/d-tpp/$cycle/"
    fun metafileUrl(cycle: String) = base(cycle) + "xml_data/d-TPP_Metafile.xml"
    fun pdfUrl(cycle: String, pdf: String) = base(cycle) + pdf

    /** "0901Z  10/01/26" → 2026-10-01. */
    fun parseEdate(s: String?): LocalDate? {
        val m = Regex("""(\d{2})/(\d{2})/(\d{2})""").find(s ?: return null) ?: return null
        val (mm, dd, yy) = m.destructured
        return runCatching { LocalDate.of(2000 + yy.toInt(), mm.toInt(), dd.toInt()) }.getOrNull()
    }
}

/** Strumieniowy parser d-TPP_Metafile.xml (SAX — działa na Androidzie i w testach JVM). */
object DtppMetafile {
    private val CONT = Regex("""^(.*?),\s*CONT\.\s*\d+$""")

    fun parse(input: InputStream): DtppIndex {
        val h = Handler()
        val f = SAXParserFactory.newInstance()
        f.isNamespaceAware = false
        f.newSAXParser().parse(input, h)
        return DtppIndex(h.cycle ?: "", h.from?.toString(), h.to?.toString(), h.airports)
    }

    private class Handler : DefaultHandler() {
        var cycle: String? = null
        var from: LocalDate? = null
        var to: LocalDate? = null
        val airports = ArrayList<ChartAirport>(3300)

        private var state = ""
        private var city = ""
        private var aptAttrs: Map<String, String> = emptyMap()
        private val charts = ArrayList<Chart>()
        private val rec = HashMap<String, String>()
        private val text = StringBuilder()
        private var inRecord = false

        override fun startElement(uri: String?, localName: String?, qName: String, attributes: Attributes) {
            when (qName) {
                "digital_tpp" -> {
                    cycle = attributes.getValue("cycle")?.trim()
                    from = DtppCycle.parseEdate(attributes.getValue("from_edate"))
                    to = DtppCycle.parseEdate(attributes.getValue("to_edate"))
                }
                "state_code" -> state = attributes.getValue("ID").orEmpty()
                "city_name" -> city = attributes.getValue("ID").orEmpty()
                "airport_name" -> {
                    aptAttrs = (0 until attributes.length).associate { attributes.getQName(it) to attributes.getValue(it) }
                    charts.clear()
                }
                "record" -> { inRecord = true; rec.clear() }
            }
            text.setLength(0)
        }

        override fun characters(ch: CharArray, start: Int, length: Int) {
            if (inRecord) text.append(ch, start, length)
        }

        override fun endElement(uri: String?, localName: String?, qName: String) {
            when (qName) {
                "record" -> { inRecord = false; addRecord() }
                "airport_name" -> {
                    val ident = aptAttrs["apt_ident"]?.trim().orEmpty()
                    if (ident.isNotEmpty() && charts.isNotEmpty()) {
                        airports += ChartAirport(
                            ident = ident,
                            icao = aptAttrs["icao_ident"]?.trim()?.takeIf { it.isNotEmpty() },
                            name = aptAttrs["ID"]?.trim().orEmpty(),
                            city = city, state = state,
                            military = aptAttrs["military"] == "Y",
                            charts = charts.toList(),
                        )
                    }
                }
                else -> if (inRecord) rec[qName] = text.toString().trim()
            }
        }

        private fun addRecord() {
            if (rec["useraction"].equals("D", ignoreCase = true)) return
            val pdf = rec["pdf_name"].orEmpty()
            if (pdf.isEmpty() || pdf.startsWith("DELETED", ignoreCase = true)) return
            val type = ChartType.fromCode(rec["chart_code"].orEmpty()) ?: return
            val name = rec["chart_name"].orEmpty()
            val ident = aptAttrs["apt_ident"]?.trim().orEmpty()
            // „KENNEDY FIVE, CONT.1” to druga strona karty „KENNEDY FIVE”
            CONT.find(name)?.let { m ->
                val base = m.groupValues[1].trim()
                val i = charts.indexOfLast { it.type == type && it.name == base }
                if (i >= 0) { charts[i] = charts[i].copy(pdfs = charts[i].pdfs + pdf); return }
            }
            val amdt = rec["amdtnum"].orEmpty().let { n -> if (n.isEmpty()) null else listOfNotNull("Amdt $n", rec["amdtdate"]?.takeIf { it.isNotEmpty() }).joinToString(" ") }
            charts += Chart(ident, type, name, listOf(pdf), changed = rec["useraction"].equals("C", true) || rec["useraction"].equals("A", true), amendment = amdt)
        }
    }
}

/** Stan katalogu kart do pokazania w UI (pobieranie indeksu trwa chwilę — 16 MB). */
sealed interface ChartsStatus {
    data object Idle : ChartsStatus
    data class Downloading(val cycle: String, val bytes: Long, val total: Long) : ChartsStatus
    data class Ready(val cycle: String, val stale: Boolean) : ChartsStatus
    data class Failed(val message: String) : ChartsStatus
}

/**
 * Karty FAA „najpierw lokalnie”: indeks i pobrane PDF-y leżą w filesDir/charts/<cykl>/,
 * więc po pobraniu działają bez sieci. Nowy cykl zastępuje stary (stare katalogi są usuwane).
 */
class FaaCharts(
    private val root: File,
    private val client: OkHttpClient = Http.client,
    private val clock: () -> Instant = Instant::now,
) {
    private val mutex = Mutex()
    @Volatile private var index: DtppIndex? = null
    private val _status = MutableStateFlow<ChartsStatus>(ChartsStatus.Idle)
    val status: StateFlow<ChartsStatus> = _status.asStateFlow()

    /** Cykl obowiązujący teraz (liczony z daty, bez sieci). */
    fun currentCycle(): String = DtppCycle.current(clock())

    /** Najnowszy cykl zapisany w telefonie albo null. */
    fun localCycle(): String? = index?.cycle ?: localCycles().firstOrNull()

    fun hasIndex(): Boolean = localCycle() != null

    /** Zapisany indeks jest z wcześniejszego cyklu niż obowiązujący. */
    fun isStale(): Boolean = localCycle()?.let { it != currentCycle() } ?: true

    /**
     * Lotnisko w USA (karty d-TPP)? Gdy indeks jest wczytany — sprawdza w nim, inaczej
     * po prefiksie ICAO: K (kontynent), PA/PF/PO/PP (Alaska), PH (Hawaje), PG/PW/PK/PM/PT (Pacyfik),
     * TJ (Portoryko), TI (Wyspy Dziewicze), NSTU (Samoa).
     */
    fun isUsAirport(icao: String): Boolean {
        val k = icao.trim().uppercase()
        index?.let { return it.contains(k) }
        return isUsIcao(k)
    }

    /**
     * Karty lotniska z lokalnego indeksu (także z poprzedniego cyklu — wtedy [isStale]).
     * Gdy w telefonie nie ma żadnego indeksu, pobiera bieżący. Rzuca IOException bez sieci i bez indeksu.
     */
    suspend fun chartsFor(icao: String): List<Chart> = (loadIndex() ?: update()).charts(icao)

    /** Lotnisko z indeksu (nazwa, identyfikatory) — do nagłówka panelu. */
    suspend fun airport(icao: String): ChartAirport? = loadIndex()?.airport(icao)

    /** Pierwszy PDF karty (pobiera, jeśli go nie ma). */
    suspend fun pdf(chart: Chart): File = pdfs(chart).first()

    /** Wszystkie pliki karty (strony CONT.n), pobierane na żądanie i trzymane w katalogu cyklu. */
    suspend fun pdfs(chart: Chart): List<File> = withContext(Dispatchers.IO) {
        val cycle = chart.cycle.ifEmpty { localCycle() ?: currentCycle() }
        val dir = File(root, cycle).apply { mkdirs() }
        chart.pdfs.map { name ->
            val f = File(dir, safeName(name))
            if (!f.exists() || f.length() == 0L) download(DtppCycle.pdfUrl(cycle, name), f)
            f
        }
    }

    /** Czy wszystkie strony karty są już w telefonie (do ikonki „offline” na liście). */
    fun isDownloaded(chart: Chart): Boolean {
        val dir = File(root, chart.cycle.ifEmpty { localCycle() ?: return false })
        return chart.pdfs.all { File(dir, safeName(it)).let { f -> f.exists() && f.length() > 0 } }
    }

    /**
     * Pobiera indeks bieżącego cyklu, jeśli w telefonie jest starszy (albo [force]).
     * Po sukcesie usuwa katalogi starych cykli razem z ich PDF-ami.
     */
    suspend fun update(force: Boolean = false): DtppIndex = mutex.withLock {
        val want = currentCycle()
        val have = index ?: readLocal()
        if (!force && have != null && have.cycle == want) return@withLock have
        val idx = try {
            fetchIndex(want)
        } catch (e: IOException) {
            // Metaplik nowego cyklu bywa publikowany z opóźnieniem — próbujemy poprzedni
            if (have == null) runCatching { fetchIndex(DtppCycle.previous(clock())) }.getOrNull()
                ?: run { _status.value = ChartsStatus.Failed(e.message ?: loc("faa_charts.no_connection")); throw e }
            else { _status.value = ChartsStatus.Ready(have.cycle, stale = true); throw e }
        }
        index = idx
        cleanup(keep = idx.cycle)
        _status.value = ChartsStatus.Ready(idx.cycle, stale = idx.cycle != want)
        idx
    }

    /** Usuwa katalogi cykli innych niż [keep]. */
    suspend fun cleanup(keep: String) = withContext(Dispatchers.IO) {
        root.listFiles()?.filter { it.isDirectory && it.name != keep }?.forEach { it.deleteRecursively() }
    }

    /** Miejsce zajęte przez karty (indeks + PDF-y). */
    suspend fun diskUsage(): Long = withContext(Dispatchers.IO) { root.walkTopDown().filter { it.isFile }.sumOf { it.length() } }

    /** Usuwa wszystko (ustawienia → wyczyść karty). */
    suspend fun clear() = mutex.withLock { withContext(Dispatchers.IO) { index = null; root.deleteRecursively(); _status.value = ChartsStatus.Idle } }

    // ---------- wewnętrzne ----------

    private suspend fun loadIndex(): DtppIndex? = index ?: mutex.withLock { index ?: readLocal() }

    private suspend fun readLocal(): DtppIndex? = withContext(Dispatchers.IO) {
        for (c in localCycles()) {
            val idx = runCatching { json.decodeFromString(DtppIndex.serializer(), File(File(root, c), INDEX).readText()) }.getOrNull()
            if (idx != null) {
                index = idx
                _status.value = ChartsStatus.Ready(idx.cycle, stale = idx.cycle != currentCycle())
                return@withContext idx
            }
        }
        null
    }

    /** Cykle z indeksem w telefonie, od najnowszego. */
    private fun localCycles(): List<String> =
        root.listFiles()?.filter { it.isDirectory && File(it, INDEX).exists() }?.map { it.name }?.sortedDescending().orEmpty()

    /** Pobiera metaplik strumieniowo i od razu go parsuje (16 MB XML nie trafia na dysk). */
    private suspend fun fetchIndex(cycle: String): DtppIndex = withContext(Dispatchers.IO) {
        val req = Request.Builder().url(DtppCycle.metafileUrl(cycle)).header("User-Agent", UA).build()
        val idx = client.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException(loc("faa_charts.http_for_d_tpp_metafile", resp.code, cycle))
            val body = resp.body ?: throw IOException(loc("faa_charts.empty_response_from_faa"))
            val total = body.contentLength()
            _status.value = ChartsStatus.Downloading(cycle, 0, total)
            val counting = object : FilterInputStream(body.byteStream()) {
                var n = 0L
                var last = 0L
                private fun tick(read: Long) {
                    if (read > 0) n += read
                    if (n - last > 256 * 1024) { last = n; _status.value = ChartsStatus.Downloading(cycle, n, total) }
                }
                override fun read(): Int = super.read().also { if (it >= 0) tick(1) }
                override fun read(b: ByteArray, off: Int, len: Int): Int = super.read(b, off, len).also { tick(it.toLong()) }
            }
            try {
                DtppMetafile.parse(counting)
            } catch (e: IOException) {
                throw e
            } catch (e: Exception) {
                throw IOException(loc("faa_charts.could_not_read_the_d_tpp") + e.message, e)
            }
        }
        if (idx.airports.isEmpty()) throw IOException(loc("faa_charts.d_tpp_metafile_contains_no_airports", cycle))
        val dir = File(root, idx.cycle.ifEmpty { cycle }).apply { mkdirs() }
        val f = File(dir, INDEX)
        val tmp = File(dir, "$INDEX.tmp")
        tmp.writeText(json.encodeToString(DtppIndex.serializer(), idx))
        if (!tmp.renameTo(f)) { f.delete(); tmp.renameTo(f) }
        if (idx.cycle.isEmpty()) idx.copy(cycle = cycle) else idx
    }

    private fun download(url: String, target: File) {
        val req = Request.Builder().url(url).header("User-Agent", UA).build()
        client.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException(loc("common.http_for", resp.code, url))
            val tmp = File(target.parentFile, target.name + ".part")
            resp.body!!.byteStream().use { input -> tmp.outputStream().use { input.copyTo(it) } }
            if (!tmp.renameTo(target)) { target.delete(); tmp.renameTo(target) }
        }
    }

    companion object {
        private const val INDEX = "index.json"
        private const val UA = "OpenTakeoff-Android/1.0"
        private val json = Json { ignoreUnknownKeys = true; encodeDefaults = false; explicitNulls = false }

        @Volatile private var instance: FaaCharts? = null

        /** Wspólna instancja na filesDir/charts. */
        fun get(context: Context): FaaCharts = instance ?: synchronized(this) {
            instance ?: FaaCharts(File(context.applicationContext.filesDir, "charts")).also { instance = it }
        }

        private val US_ICAO = Regex("""^(K[A-Z0-9]{3}|P[AFOPHGWKMT][A-Z0-9]{2}|T[JI][A-Z0-9]{2}|NSTU)$""")

        fun isUsIcao(icao: String): Boolean = US_ICAO.matches(icao.trim().uppercase())

        /** Nazwa pliku z metapliku bez ścieżek i dziwnych znaków. */
        internal fun safeName(name: String) = name.filter { it.isLetterOrDigit() || it == '_' || it == '-' || it == '.' }.ifEmpty { "chart.pdf" }
    }
}
