package com.freemetar.app

import com.freemetar.app.domain.loc

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.freemetar.app.data.Airport
import com.freemetar.app.data.AirportDb
import com.freemetar.app.data.Frequency
import com.freemetar.app.data.Metar
import com.freemetar.app.data.Navaid
import com.freemetar.app.data.Prefs
import com.freemetar.app.data.Runway
import com.freemetar.app.data.Settings
import com.freemetar.app.data.UpdateInfo
import com.freemetar.app.data.Updater
import java.io.File
import com.freemetar.app.data.WeatherApi
import com.freemetar.app.data.WeatherBundle
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import com.freemetar.app.data.Locator
import com.freemetar.app.data.nav.AirportNav
import com.freemetar.app.data.nav.NavDataStore
import com.freemetar.app.data.nav.NavRepository
import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.data.link.FlightLogStore
import com.freemetar.app.data.link.FlightRecorder
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.data.link.LinkStatus
import com.freemetar.app.data.link.TrackPoint
import android.net.Uri
import com.freemetar.app.data.LatLon
import com.freemetar.app.data.OpenMeteo
import com.freemetar.app.data.Pirep
import com.freemetar.app.data.Sigmet
import com.freemetar.app.data.WindsAloft
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

sealed interface DbState {
    data object Idle : DbState
    data class Loading(val message: String) : DbState
    data class Ready(val count: Int, val updated: Long?) : DbState
    data class Error(val message: String) : DbState
}

data class AirportDetail(
    val ident: String,
    val airport: Airport? = null,
    val runways: List<Runway> = emptyList(),
    val frequencies: List<Frequency> = emptyList(),
    val nearby: List<Pair<Airport, Double>> = emptyList(),
    val navaids: List<Pair<Navaid, Double>> = emptyList(),
    val weather: WeatherBundle? = null,
    val nearbyMetars: Map<String, Metar> = emptyMap(),
    /** SIGMET obejmujące lotnisko lub do 50 NM od niego, z odległością (0 = nad lotniskiem). */
    val sigmets: List<Pair<Sigmet, Double>> = emptyList(),
    val pireps: List<Pirep> = emptyList(),
    val aloft: WindsAloft? = null,
    /** Dane z bazy MSFS (pasy z ILS, częstotliwości, procedury) — z pamięci telefonu lub z Link. */
    val nav: AirportNav? = null,
    val navLoading: Boolean = false,
    val loading: Boolean = true,
    val error: String? = null,
)

sealed interface UpdateState {
    data object Idle : UpdateState
    data object Checking : UpdateState
    data object UpToDate : UpdateState
    data class Available(val info: UpdateInfo) : UpdateState
    data class Downloading(val info: UpdateInfo, val progress: Float) : UpdateState
    data class Ready(val info: UpdateInfo, val file: File) : UpdateState
    data class Error(val message: String) : UpdateState
}

sealed interface NearMe {
    data object Idle : NearMe
    data object Locating : NearMe
    /** [aircraft] = lista wokół samolotu z symulatora (zamiast lokalizacji telefonu). */
    data class Found(val airports: List<Pair<Airport, Double>>, val metars: Map<String, Metar>, val aircraft: Boolean = false, val loaded: Boolean = true) : NearMe
    data class Failed(val message: String) : NearMe
}

class MainViewModel(app: Application) : AndroidViewModel(app) {
    private val db = AirportDb(app)
    private val prefs = Prefs(app)

    val settings: StateFlow<Settings> = prefs.settings.stateIn(viewModelScope, SharingStarted.Eagerly, Settings())
    /** Ustawienia wczytane z dysku — do tego czasu [settings] ma wartości domyślne (np. welcomeShown = false). */
    val settingsLoaded: StateFlow<Boolean> = prefs.settings.map { true }.stateIn(viewModelScope, SharingStarted.Eagerly, false)

    private val flightLogStore = FlightLogStore(app)

    private val _db = MutableStateFlow<DbState>(DbState.Idle)
    val dbState = _db.asStateFlow()

    private val _favMetars = MutableStateFlow<Map<String, Metar>>(emptyMap())
    val favMetars = _favMetars.asStateFlow()
    private val _favLoading = MutableStateFlow(false)
    val favLoading = _favLoading.asStateFlow()

    private val _search = MutableStateFlow<List<Airport>>(emptyList())
    val search = _search.asStateFlow()

    private val _detail = MutableStateFlow<AirportDetail?>(null)
    val detail = _detail.asStateFlow()

    private var refreshJob: Job? = null

    private val _update = MutableStateFlow<UpdateState>(UpdateState.Idle)
    val update = _update.asStateFlow()

    // Zapowiedzi głosowe: ustawienia w Prefs, zasilane stanem z symulatora (startLinkWatchers)
    private val callouts = com.freemetar.app.data.Callouts(app)
    val calloutsEnabled = settings.map { it.calloutsEnabled }.stateIn(viewModelScope, SharingStarted.Eagerly, false)
    val enabledCallouts = settings.map { it.enabledCallouts }.stateIn(viewModelScope, SharingStarted.Eagerly, Settings().enabledCallouts)
    val calloutsLanguage = settings.map { it.calloutsLanguage }.stateIn(viewModelScope, SharingStarted.Eagerly, "en")
    val calloutsSpeechRate = settings.map { it.calloutsSpeechRate }.stateIn(viewModelScope, SharingStarted.Eagerly, 1.0f)

    // Checklists
    private val checklistsManager = com.freemetar.app.data.ChecklistsManager(app)
    private val _currentChecklists = MutableStateFlow<com.freemetar.app.data.AircraftChecklists?>(null)
    val currentChecklists = _currentChecklists.asStateFlow()

    /** Dialog o nowej wersji pokazujemy raz na uruchomienie. */
    private val _updatePromptDismissed = MutableStateFlow(false)
    val updatePromptDismissed = _updatePromptDismissed.asStateFlow()

    /** Krótki komunikat dla użytkownika (ocena lądowania, błędy) — pokazywany jako pasek u dołu ekranu Lot. */
    private val _flightMessage = MutableStateFlow<String?>(null)
    val flightMessage = _flightMessage.asStateFlow()

    fun consumeFlightMessage() { _flightMessage.value = null }

    // ---------- OpenTakeoff Link (MSFS) ----------

    val link = LinkClient(app, prefs.linkTokens)
    val linkStatus = link.status
    val linkServers = link.servers
    val flightState = link.state
    val nav = NavRepository(link, NavDataStore(app))
    private val recorder = FlightRecorder(
        nearestAirport = { lat, lon -> db.nearby(lat, lon, radiusNm = 5.0, limit = 1).firstOrNull()?.first?.ident },
        // Progi pasów lotnisk do 5 NM od przyziemienia (OurAirports: współrzędne progów i kursy rzeczywiste)
        runwaysNear = { lat, lon ->
            db.nearby(lat, lon, radiusNm = 5.0, limit = 3).flatMap { (a, _) ->
                db.runways(a.ident).flatMap { rw ->
                    listOf(rw.le, rw.he).mapNotNull { e ->
                        val tLat = e.lat ?: return@mapNotNull null
                        val tLon = e.lon ?: return@mapNotNull null
                        val hdg = com.freemetar.app.domain.Wind.headingOf(e.ident, e.headingTrue) ?: return@mapNotNull null
                        com.freemetar.app.domain.RunwayGeom("${a.ident} ${e.ident}", tLat, tLon, hdg, rw.lengthFt?.toDouble())
                    }
                }
            }
        },
    )
    private var announcedLanding: com.freemetar.app.domain.LandingReport? = null
    private val _flightLogs = MutableStateFlow<List<FlightLog>>(emptyList())
    val flightLogs = _flightLogs.asStateFlow()
    private val _track = MutableStateFlow<List<TrackPoint>>(emptyList())
    /** Tor bieżącego lotu (do rysowania na mapie). */
    val track = _track.asStateFlow()
    private var userDisconnected = false

    /** Odkrywanie działa, gdy ekran Lot lub Mapa jest widoczny; znany komputer łączy się sam. */
    fun startLinkDiscovery() {
        link.startDiscovery()
    }

    fun stopLinkDiscovery() = link.stopDiscovery()

    fun linkConnect(ip: String, port: Int = 47810, code: String? = null) {
        userDisconnected = false
        link.connect(ip, port, code)
    }

    fun linkDisconnect() {
        userDisconnected = true
        link.disconnect()
    }

    /** Tor lotu jako GPX 1.1 (do Google Earth, Garmin, Strava i innych). */
    fun exportFlightGpx(ctx: android.content.Context, f: FlightLog, uri: Uri) = viewModelScope.launch {
        runCatching {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                ctx.contentResolver.openOutputStream(uri, "wt")!!.bufferedWriter().use { w -> w.write(com.freemetar.app.data.link.FlightExport.gpx(f)) }
            }
        }.onSuccess { _flightMessage.value = loc("Zapisano tor lotu do GPX.", "Flight track saved as GPX.") }
            .onFailure { _flightMessage.value = loc("Nie zapisano pliku: ${it.message}", "File not saved: ${it.message}") }
    }

    /** Wszystkie loty w jednym pliku .oab (gzip) — kopia zapasowa albo przeniesienie na inny telefon. */
    fun exportFlightArchive(ctx: android.content.Context, uri: Uri) = viewModelScope.launch {
        runCatching {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                ctx.contentResolver.openOutputStream(uri, "wt")!!.use { it.write(com.freemetar.app.data.link.FlightArchive.encode(_flightLogs.value)) }
            }
        }.onSuccess { _flightMessage.value = loc("Wyeksportowano ${_flightLogs.value.size} lotów.", "Exported ${_flightLogs.value.size} flights.") }
            .onFailure { _flightMessage.value = loc("Nie zapisano pliku: ${it.message}", "File not saved: ${it.message}") }
    }

    /** Import archiwum .oab lub starego flights.json; loty o tym samym id nie są dublowane. */
    fun importFlightArchive(ctx: android.content.Context, uri: Uri) = viewModelScope.launch {
        runCatching {
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                com.freemetar.app.data.link.FlightArchive.decode(ctx.contentResolver.openInputStream(uri)!!.use { it.readBytes() })
            }
        }.onSuccess { incoming ->
            val before = _flightLogs.value.size
            _flightLogs.value = com.freemetar.app.data.link.FlightArchive.merge(_flightLogs.value, incoming)
            flightLogStore.save(_flightLogs.value)
            val added = _flightLogs.value.size - before
            _flightMessage.value = loc("Zaimportowano $added lotów (pominięto ${incoming.size - added} już zapisanych).", "Imported $added flights (skipped ${incoming.size - added} already saved).")
        }.onFailure { _flightMessage.value = loc("To nie jest archiwum lotów OpenTakeoff.", "This isn't an OpenTakeoff flight archive.") }
    }

    /** Stan zapisu lotu do kokpitu: nagrywa (od kiedy) albo wstrzymany po ręcznym zakończeniu. */
    data class RecordingInfo(val recording: Boolean = false, val startedAt: Long? = null, val held: Boolean = false, val startFuelKg: Double? = null)
    private val _recordingInfo = MutableStateFlow(RecordingInfo())
    val recordingInfo = _recordingInfo.asStateFlow()
    private fun publishRecording() {
        _recordingInfo.value = RecordingInfo(recorder.recording, recorder.startedAt, recorder.holdUntilGround, recorder.startFuelKg)
        recorder.startedAt?.let(::adoptMapTools)
        // Powiadomienie usługi pokazuje, czy tor jest właśnie zapisywany
        if (link.status.value.let { it is LinkStatus.Connected || it is LinkStatus.Lost }) com.freemetar.app.data.link.FlightLinkService.update(getApplication(), recorder.recording)
    }

    /** Ręczne zakończenie zapisu bieżącego lotu — trafia do dziennika (jeśli trwał co najmniej 2 min). */
    fun stopRecording() {
        val log = recorder.stopByUser()
        if (log != null) saveFlight(log)
        else { _flightMessage.value = loc("Zapis zakończony — lot krótszy niż 2 min nie trafia do dziennika.", "Recording stopped — flights under 2 min are not saved."); viewModelScope.launch { flightLogStore.clearDraft() } }
        _track.value = emptyList()
        publishRecording()
    }

    fun resumeRecording() { recorder.resume(); publishRecording() }

    private val _nearbyNav = MutableStateFlow<List<com.freemetar.app.data.nav.FixResult>>(emptyList())
    /** Lotniska, VOR, NDB i punkty trasy z bazy MSFS wokół samolotu — do mapy ND. */
    val nearbyNav = _nearbyNav.asStateFlow()

    private val _mapFlight = MutableStateFlow<String?>(null)
    /** Zapisany lot pokazywany na mapie (zamiast toru bieżącego lotu); null = brak. */
    val mapFlight = _mapFlight.asStateFlow()
    fun showFlightOnMap(id: String?) { _mapFlight.value = id }

    /**
     * Ustawienia mapy (warstwy, róża, radiale, kadr) — trzymane tu, a nie w ekranie, żeby po powrocie na mapę
     * działały dalej. [flight] = początek nagrania lotu, w którym je ustawiono; nowy lot zaczyna od domyślnych.
     */
    data class MapTools(
        val follow: Boolean = true,
        val showSigmets: Boolean = true,
        val showMetars: Boolean = true,
        val showRoute: Boolean = true,
        val showNav: Boolean = true,
        val showRose: Boolean = false,
        val radialMode: Boolean = false,
        val ref: com.freemetar.app.ui.screens.RefPoint? = null,
        val selRadial: Int? = null,
        val inbound: Boolean = true,
        val camera: org.maplibre.android.camera.CameraPosition? = null,
        val flight: Long? = null,
    )
    private val _mapTools = MutableStateFlow(MapTools())
    val mapTools = _mapTools.asStateFlow()
    fun updateMapTools(f: (MapTools) -> MapTools) = _mapTools.update { t ->
        f(t).let { n -> if (n == t) t else n.copy(flight = recorder.startedAt) }
    }
    /** Nowe nagranie: ustawienia z poprzedniego lotu wracają do domyślnych, ustawione przed startem przechodzą na ten lot. */
    private fun adoptMapTools(startedAt: Long) = _mapTools.update { t ->
        when (t.flight) {
            startedAt -> t
            null -> t.copy(flight = startedAt)
            else -> MapTools(camera = t.camera, flight = startedAt)
        }
    }
    /** Kadr nie zmienia lotu, do którego należą ustawienia. */
    fun saveMapCamera(cp: org.maplibre.android.camera.CameraPosition) = _mapTools.update { it.copy(camera = cp) }

    /** Rozmiar archiwum lotów na dysku (do pokazania w ustawieniach). */
    fun flightArchiveBytes(): Long = flightLogStore.sizeBytes()

    fun deleteFlightLog(id: String) = viewModelScope.launch {
        _flightLogs.value = _flightLogs.value.filterNot { it.id == id }
        flightLogStore.save(_flightLogs.value)
    }

    /** Ręczny znacznik w trwającym nagraniu (przycisk w kokpicie) — z czasem od startu w opisie. */
    fun markLive() {
        val started = recorder.startedAt
        val label = loc("Znacznik", "Marker") + (started?.let { " " + "%d:%02d".format((System.currentTimeMillis() - it) / 60_000, (System.currentTimeMillis() - it) / 1000 % 60) } ?: "")
        _flightMessage.value = if (recorder.mark(label)) loc("Dodano znacznik — opis zmienisz w dzienniku.", "Marker added — edit its note in the logbook.")
        else loc("Znaczniki dodaje się w trakcie zapisu lotu.", "Markers can be added while a flight is recording.")
    }

    private fun updateFlight(id: String, change: (FlightLog) -> FlightLog) = viewModelScope.launch {
        _flightLogs.value = _flightLogs.value.map { if (it.id == id) change(it) else it }
        flightLogStore.save(_flightLogs.value)
    }

    /** Znacznik w zapisanym locie (z odtwarzania); ten sam czas zastępuje poprzedni opis. */
    fun setFlightMarker(id: String, t: Long, label: String) = updateFlight(id) { f ->
        f.copy(markers = (f.markers.filterNot { it.t == t } + com.freemetar.app.data.link.FlightMarker(t, label)).sortedBy { it.t })
    }

    fun deleteFlightMarker(id: String, t: Long) = updateFlight(id) { f -> f.copy(markers = f.markers.filterNot { it.t == t }) }

    fun setFlightScenario(id: String, scenario: String?) = updateFlight(id) { it.copy(scenario = scenario) }

    private fun saveFlight(log: FlightLog?) {
        log ?: return
        viewModelScope.launch {
            // Ten sam id co szkic — wpis końcowy zastępuje szkic, nie dubluje go
            _flightLogs.value = listOf(log) + _flightLogs.value.filterNot { it.id == log.id }
            flightLogStore.clearDraft()
            flightLogStore.save(_flightLogs.value)
            val route = "${log.from ?: "?"} → ${log.to ?: "?"}"
            _flightMessage.value = loc("Zapisano lot $route (${log.durationMin} min).", "Flight $route saved (${log.durationMin} min).")
        }
    }

    private fun startLinkWatchers() {
        viewModelScope.launch {
            val logs = flightLogStore.load()
            // Lot przerwany zamknięciem aplikacji: szkic trafia do dziennika
            val draft = flightLogStore.takeDraft()
            _flightLogs.value = if (draft != null && logs.none { it.id == draft.id }) listOf(draft) + logs else logs
            if (draft != null) { flightLogStore.save(_flightLogs.value); flightLogStore.clearDraft() }
        }
        // Szkic trwającego lotu co 30 s; po 60 s bez danych z symulatora lot jest zamykany i zapisywany
        viewModelScope.launch {
            var noDataSince: Long? = null
            while (true) {
                kotlinx.coroutines.delay(30_000)
                if (recorder.recording) {
                    recorder.snapshot()?.let { flightLogStore.saveDraft(it) }
                    if (link.state.value == null) {
                        val since = noDataSince ?: System.currentTimeMillis().also { noDataSince = it }
                        // Zerwane Wi-Fi (np. po zablokowaniu telefonu) z trwającym ponownym łączeniem: dłużej czekamy na powrót
                        val limitMs = if (link.status.value is LinkStatus.Lost) 10 * 60_000 else 60_000
                        if (System.currentTimeMillis() - since >= limitMs) { saveFlight(recorder.finish()); noDataSince = null }
                    } else noDataSince = null
                } else noDataSince = null
            }
        }
        viewModelScope.launch { settings.collect { recorder.intervalMs = it.recordIntervalMs.toLong().coerceIn(250, 10_000) } }
        // Lotniska i pomoce nawigacyjne z MSFS wokół samolotu (do ND): po ~10 NM lotu albo co 3 min
        viewModelScope.launch {
            var lastLat = Double.NaN; var lastLon = Double.NaN; var lastAt = 0L
            while (true) {
                val st = link.state.value
                val lat = st?.position?.lat; val lon = st?.position?.lon
                if (lat != null && lon != null && nav.connected) {
                    val moved = lastLat.isNaN() || com.freemetar.app.domain.Geo.distanceNm(lastLat, lastLon, lat, lon) > 10
                    if (moved || System.currentTimeMillis() - lastAt > 180_000) {
                        findNearAircraft(lat, lon)
                        val found = nav.nearby(lat, lon, 80)
                        if (found.isNotEmpty() || moved) _nearbyNav.value = found
                        lastLat = lat; lastLon = lon; lastAt = System.currentTimeMillis()
                    }
                } else if (st == null) {
                    lastLat = Double.NaN
                    // Bez symulatora wracamy do lokalizacji telefonu
                    if ((_nearMe.value as? NearMe.Found)?.aircraft == true) _nearMe.value = NearMe.Idle
                } else if (lat != null && lon != null && (_nearMe.value as? NearMe.Found)?.aircraft != true) {
                    // Połączony bez MSFS-owych danych nawigacyjnych (np. symulacja) — lista lotnisk i tak wokół samolotu
                    findNearAircraft(lat, lon); lastLat = lat; lastLon = lon; lastAt = System.currentTimeMillis()
                }
                kotlinx.coroutines.delay(15_000)
            }
        }
        viewModelScope.launch {
            settings.map { listOf(it.calloutsEnabled, it.enabledCallouts, it.calloutsLanguage, it.calloutsSpeechRate) }.distinctUntilChanged().collect {
                val st = settings.value
                callouts.configure(st.calloutsEnabled, st.enabledCallouts, st.calloutsLanguage, st.calloutsSpeechRate)
            }
        }
        viewModelScope.launch {
            link.state.collect { st ->
                if (st != null) {
                    callouts.update(st)
                    saveFlight(recorder.onState(st))
                    if (recorder.recording != _recordingInfo.value.recording || recorder.holdUntilGround != _recordingInfo.value.held) publishRecording()
                    // Kopia toru tylko po dojściu nowego punktu, nie przy każdym stanie z Link
                    if (recorder.recording && recorder.pointCount != _track.value.size) _track.value = recorder.currentTrack
                    // Ocena zaraz po przyziemieniu — nie trzeba czekać na koniec lotu
                    recorder.lastLanding?.takeIf { it !== announcedLanding }?.let { l ->
                        announcedLanding = l
                        _flightMessage.value = loc("Lądowanie: ${l.score}/100, ${l.gradeText.lowercase()} — ${l.vsFpm} ft/min", "Landing: ${l.score}/100, ${l.gradeText.lowercase()} — ${l.vsFpm} ft/min") + (l.runway?.let { ", $it" } ?: "")
                    }
                }
            }
        }
        viewModelScope.launch {
            link.status.collect { s ->
                if (s !is LinkStatus.Connected && s !is LinkStatus.Lost) {
                    saveFlight(recorder.finish())
                    com.freemetar.app.data.link.FlightLinkService.stop(getApplication())
                } else com.freemetar.app.data.link.FlightLinkService.update(getApplication(), recorder.recording)
            }
        }
        // Po połączeniu z MSFS: dane nawigacyjne lotnisk trasy, ulubionych i otwartego briefingu
        viewModelScope.launch {
            link.status.collect { s ->
                if (s is LinkStatus.Connected && s.source == "msfs" || s is LinkStatus.Connected && s.source == "simulation") {
                    val ids = buildList {
                        _detail.value?.ident?.let(::add)
                        addAll(settings.value.favorites.take(6))
                    }.distinct()
                    ids.forEach { id -> runCatching { nav.airport(id) } }
                    _detail.value?.ident?.let { id ->
                        val cached = nav.cached(id)
                        _detail.update { d -> if (d?.ident == id) d.copy(nav = cached) else d }
                    }
                }
            }
        }
        // Automatyczne połączenie z komputerem, z którym telefon już się sparował
        viewModelScope.launch {
            link.servers.collect { list ->
                val last = prefs.lastLinkIp() ?: return@collect
                if (!userDisconnected && link.status.value is LinkStatus.Idle) {
                    list.firstOrNull { it.ip == last }?.let { link.connect(it.ip, it.port) }
                }
            }
        }
    }

    override fun onCleared() {
        callouts.shutdown()
        saveFlight(recorder.finish())
        link.disconnect()
        link.stopDiscovery()
        com.freemetar.app.data.link.FlightLinkService.stop(getApplication())
        super.onCleared()
    }

    private val _nearMe = MutableStateFlow<NearMe>(NearMe.Idle)
    val nearMe = _nearMe.asStateFlow()

    init {
        loadDb(force = false)
        startLinkWatchers()
        startAutoRefresh()
        checkForUpdate(silent = true)
        // Lista ulubionych zmieniona (dodanie, usunięcie) → pobierz brakujące METAR-y
        viewModelScope.launch {
            prefs.settings.map { it.favorites.toSet() }.distinctUntilChanged().collect { refreshFavorites() }
        }
    }

    /** Najbliższe lotniska z kodem ICAO (lądowiska typu „PL-0250” nie mają pogody ani odpowiednika w symulatorze). */
    private fun nearIcao(lat: Double, lon: Double) =
        db.nearby(lat, lon, radiusNm = 120.0, limit = 30).filter { (a, _) -> a.ident.length == 4 && a.ident.all { it.isLetter() } }.take(8)

    /** Lotniska najbliżej samolotu w symulatorze — zastępują listę „w pobliżu Ciebie”, dopóki jest połączenie. */
    private fun findNearAircraft(lat: Double, lon: Double) = viewModelScope.launch {
        if (!db.isLoaded) return@launch
        val list = nearIcao(lat, lon)
        val old = (_nearMe.value as? NearMe.Found)?.takeIf { it.aircraft }?.metars.orEmpty()
        _nearMe.value = NearMe.Found(list, old, aircraft = true, loaded = old.isNotEmpty())
        _nearMe.value = NearMe.Found(list, metarsFor(list.map { it.first.ident }), aircraft = true)
    }

    /** Lotniska najbliżej użytkownika (wymaga zgody na przybliżoną lokalizację). */
    fun findNearMe() = viewModelScope.launch {
        _nearMe.value = NearMe.Locating
        val pos = runCatching { Locator.locate(getApplication()) }.getOrNull()
        if (pos == null) {
            _nearMe.value = NearMe.Failed(loc("Nie udało się ustalić pozycji. Włącz lokalizację i spróbuj ponownie.", "Could not determine your position. Turn on location and try again."))
            return@launch
        }
        if (!db.isLoaded) {
            _nearMe.value = NearMe.Failed(loc("Baza lotnisk jeszcze się ładuje.", "The airport database is still loading."))
            return@launch
        }
        val list = nearIcao(pos.latitude, pos.longitude)
        _nearMe.value = NearMe.Found(list, emptyMap(), loaded = false)
        val metars = metarsFor(list.map { it.first.ident })
        _nearMe.value = NearMe.Found(list, metars)
    }

    fun checkForUpdate(silent: Boolean = false) = viewModelScope.launch {
        if (_update.value is UpdateState.Downloading) return@launch
        _update.value = UpdateState.Checking
        _update.value = try {
            Updater.check()?.let { UpdateState.Available(it) } ?: UpdateState.UpToDate
        } catch (e: Exception) {
            // Przy starcie brak sieci nie jest powodem do komunikatu
            if (silent) UpdateState.Idle else UpdateState.Error(e.message ?: loc("Nie udało się sprawdzić aktualizacji", "Could not check for updates"))
        }
    }

    fun downloadUpdate(info: UpdateInfo) = viewModelScope.launch {
        _update.value = UpdateState.Downloading(info, 0f)
        _update.value = try {
            val file = Updater.download(getApplication(), info) { p ->
                _update.value = UpdateState.Downloading(info, p)
            }
            UpdateState.Ready(info, file)
        } catch (e: Exception) {
            UpdateState.Error(loc("Pobieranie nie powiodło się: ${e.message}", "Download failed: ${e.message}"))
        }
    }

    fun dismissUpdatePrompt() { _updatePromptDismissed.value = true }

    fun loadDb(force: Boolean) = viewModelScope.launch {
        try {
            _db.value = DbState.Loading(loc("Przygotowanie bazy lotnisk…", "Preparing the airport database…"))
            db.ensureDownloaded(force) { _db.value = DbState.Loading(it) }
            _db.value = DbState.Loading(loc("Indeksowanie…", "Indexing…"))
            db.load()
            _db.value = DbState.Ready(db.airportCount, db.lastUpdated())
            _detail.value?.let { open(it.ident) }
        } catch (e: Exception) {
            _db.value = DbState.Error(e.message ?: loc("Nie udało się pobrać bazy lotnisk", "Could not download the airport database"))
        }
    }

    fun airportName(ident: String): String? = db.airport(ident)?.name
    fun airport(ident: String): Airport? = db.airport(ident)

    suspend fun metarsFor(ids: List<String>): Map<String, Metar> =
        runCatching { WeatherApi.metars(ids) }.getOrDefault(emptyList()).associateBy { it.station }

    fun refreshFavorites() = viewModelScope.launch {
        _favLoading.value = true
        try {
            val favs = prefs.settings.first().favorites
            _favMetars.value = WeatherApi.metars(favs).associateBy { it.station }
        } catch (_: Exception) {
        } finally {
            _favLoading.value = false
        }
    }

    private fun startAutoRefresh() {
        refreshJob?.cancel()
        refreshJob = viewModelScope.launch {
            while (isActive) {
                delay(5 * 60 * 1000L)
                if (settings.value.autoRefresh) {
                    refreshFavorites()
                    _detail.value?.let { refreshWeather(it.ident) }
                }
            }
        }
    }

    fun onQuery(q: String) {
        _search.value = if (db.isLoaded) db.search(q) else emptyList()
    }

    fun open(identRaw: String) {
        val ident = identRaw.uppercase().trim()
        val a = db.airport(ident)
        val key = a?.ident ?: ident
        _detail.value = AirportDetail(
            ident = key,
            airport = a,
            runways = db.runways(key),
            frequencies = db.frequencies(key),
            nearby = a?.let { db.nearby(it.lat, it.lon, exclude = key) }.orEmpty(),
            navaids = a?.let { db.navaidsNear(it.lat, it.lon) }.orEmpty(),
            weather = _detail.value?.takeIf { it.ident == key }?.weather,
        )
        viewModelScope.launch { prefs.addRecent(key) }
        refreshWeather(key)
        loadNav(key, refresh = false)
    }

    /** Dane nawigacyjne MSFS dla briefingu: najpierw kopia lokalna, potem (po połączeniu) pobranie brakujących. */
    fun loadNav(icao: String, refresh: Boolean) = viewModelScope.launch {
        val cached = nav.cached(icao)
        _detail.update { if (it?.ident == icao) it.copy(nav = cached, navLoading = nav.connected) else it }
        val fresh = runCatching { nav.airport(icao, refresh) }.getOrNull()
        _detail.update { if (it?.ident == icao) it.copy(nav = fresh ?: it.nav, navLoading = false) else it }
    }

    fun refreshWeather(ident: String) = viewModelScope.launch {
        _detail.update { it?.copy(loading = true, error = null) }
        try {
            val hist = async { WeatherApi.metars(listOf(ident), hours = 12) }
            val taf = async { runCatching { WeatherApi.taf(ident) }.getOrNull() }
            val history = hist.await().sortedByDescending { it.obsTimeEpoch }
            val bundle = WeatherBundle(history.firstOrNull(), history, taf.await(), System.currentTimeMillis())
            _detail.update { if (it?.ident == ident) it.copy(weather = bundle, loading = false) else it }
            // Pogoda na lotniskach w pobliżu — jedna paczka
            val near = _detail.value?.nearby?.map { it.first.ident }.orEmpty()
            if (near.isNotEmpty()) {
                val m = runCatching { WeatherApi.metars(near) }.getOrDefault(emptyList()).associateBy { it.station }
                _detail.update { if (it?.ident == ident) it.copy(nearbyMetars = m) else it }
            }
            loadHazardsAndAloft(ident)
        } catch (e: Exception) {
            _detail.update { if (it?.ident == ident) it.copy(loading = false, error = e.message ?: loc("Błąd sieci", "Network error")) else it }
        }
    }

    /** Zagrożenia (SIGMET, PIREP) i wiatr na wysokościach — dodatki do briefingu, ich brak nie jest błędem. */
    private suspend fun loadHazardsAndAloft(ident: String) = coroutineScope {
        val a = _detail.value?.airport
        val p = a?.let { LatLon(it.lat, it.lon) }
            ?: _detail.value?.weather?.metar?.let { m -> m.lat?.let { la -> m.lon?.let { lo -> LatLon(la, lo) } } }
            ?: return@coroutineScope
        val sig = async { runCatching { WeatherApi.sigmets() }.getOrDefault(emptyList()) }
        val pir = async { runCatching { WeatherApi.pireps(ident) }.getOrDefault(emptyList()) }
        val alo = async { runCatching { OpenMeteo.fetch(p.lat, p.lon) }.getOrNull() }
        val near = sig.await().map { it to it.distanceNm(p) }.filter { it.second <= 50 }.sortedBy { it.second }
        val reports = pir.await().sortedByDescending { it.obsTime }.take(15)
        val aloft = alo.await()
        _detail.update { if (it?.ident == ident) it.copy(sigmets = near, pireps = reports, aloft = aloft) else it }
    }

    // ---------- mapa ----------

    private val _mapMetars = MutableStateFlow<List<Metar>>(emptyList())
    val mapMetars = _mapMetars.asStateFlow()
    private val _mapSigmets = MutableStateFlow<List<Sigmet>>(emptyList())
    val mapSigmets = _mapSigmets.asStateFlow()
    private var mapJob: Job? = null

    /** Pogoda dla widocznego obszaru mapy; wywoływane po zatrzymaniu kamery. Pomija zbyt duże obszary (limit 400 stacji). */
    fun loadMapArea(south: Double, west: Double, north: Double, east: Double) {
        mapJob?.cancel()
        mapJob = viewModelScope.launch {
            delay(350) // odbicie: szybkie przesuwanie mapy nie robi serii zapytań
            if (north - south > 25 || east - west > 40) return@launch
            runCatching { WeatherApi.metarsInBox(south, west, north, east) }.onSuccess { _mapMetars.value = it }
            if (_mapSigmets.value.isEmpty()) runCatching { WeatherApi.sigmets() }.onSuccess { _mapSigmets.value = it }
        }
    }

    /** Punkt startowy mapy: ostatnio otwarte lotnisko albo pierwsze ulubione. */
    fun mapHome(): LatLon? {
        val id = _detail.value?.ident ?: settings.value.favorites.firstOrNull() ?: return null
        return db.airport(id)?.let { LatLon(it.lat, it.lon) }
    }

    fun toggleFavorite(ident: String) = viewModelScope.launch {
        prefs.toggleFavorite(ident)
        refreshFavorites()
    }

    fun moveFavorite(ident: String, delta: Int) = viewModelScope.launch { prefs.moveFavorite(ident, delta) }
    fun clearRecent() = viewModelScope.launch { prefs.clearRecent() }
    fun setPressure(u: com.freemetar.app.data.PressureUnit) = viewModelScope.launch { prefs.setPressure(u) }
    fun setAlt(u: com.freemetar.app.data.AltUnit) = viewModelScope.launch { prefs.setAlt(u) }
    fun setLanguage(v: String) = viewModelScope.launch { prefs.setLanguage(v) }
    fun setAircraftProfile(p: com.freemetar.app.domain.AircraftProfile?) = viewModelScope.launch { prefs.setAircraftProfile(p?.name) }
    fun setControlMode(m: String) = viewModelScope.launch { prefs.setControlMode(m) }
    fun setFloatingControl(v: Boolean) = viewModelScope.launch { prefs.setFloatingControl(v) }
    fun setPinJumpBar(v: Boolean) = viewModelScope.launch { prefs.setPinJumpBar(v) }
    fun setHideStatusBar(v: Boolean) = viewModelScope.launch { prefs.setHideStatusBar(v) }
    fun setLeversLeft(v: Boolean) = viewModelScope.launch { prefs.setLeversLeft(v) }
    fun setFloatingStyle(v: String) = viewModelScope.launch { prefs.setFloatingStyle(v) }
    fun setRecordInterval(ms: Int) = viewModelScope.launch { prefs.setRecordInterval(ms) }
    fun setTheme(m: com.freemetar.app.data.ThemeMode) = viewModelScope.launch { prefs.setTheme(m) }
    fun setAutoRefresh(v: Boolean) = viewModelScope.launch { prefs.setAutoRefresh(v) }
    fun setWelcomeShown() = viewModelScope.launch { prefs.setWelcomeShown(true) }

    // Callouts functions
    fun setCalloutsEnabled(enabled: Boolean) = viewModelScope.launch { prefs.setCalloutsEnabled(enabled) }
    fun toggleCallout(id: String, enabled: Boolean) = viewModelScope.launch { prefs.setCallout(id, enabled) }
    fun setCalloutsLanguage(lang: String) = viewModelScope.launch { prefs.setCalloutsLanguage(lang) }
    fun setCalloutsSpeechRate(rate: Float) = viewModelScope.launch { prefs.setCalloutsSpeechRate(rate) }

    // Checklists functions
    val checklistAircraft: List<Pair<String, String>> get() = checklistsManager.available

    /** Samolot z symulatora; bez połączenia zostaje ostatni wybrany ręcznie. */
    fun loadChecklistsForAircraft(aircraftTitle: String?) = viewModelScope.launch {
        if (aircraftTitle == null && _currentChecklists.value != null) return@launch
        _currentChecklists.value = checklistsManager.getChecklists(aircraftTitle)
    }

    fun pickChecklists(filename: String) = viewModelScope.launch {
        _currentChecklists.value = checklistsManager.load(filename)
    }
}
