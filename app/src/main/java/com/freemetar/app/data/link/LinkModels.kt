package com.freemetar.app.data.link

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** Modele protokołu OpenTakeoff Link v1 (docs/link-protocol.md). Wszystkie pola opcjonalne — brak danych to null. */
val LinkJson = Json { ignoreUnknownKeys = true; explicitNulls = false; encodeDefaults = true; isLenient = true }

@Serializable
data class Announce(
    val v: Int = 1,
    val type: String = "",
    val name: String = "OpenTakeoff Link",
    val version: String? = null,
    val host: String? = null,
    val port: Int = 47810,
    val ips: List<String> = emptyList(),
    val source: String? = null,
)

/** Serwer znaleziony w sieci (adres z nadawcy datagramu). */
data class LinkServer(val name: String, val host: String, val ip: String, val port: Int, val source: String?, val version: String?, val lastSeen: Long)

@Serializable data class AircraftInfo(
    val title: String? = null, val onGround: Boolean? = null, val paused: Boolean? = null,
    /** piston / jet / turboprop / helicopter / none — z Link (protokół v1, pola dodatkowe). */
    val engineType: String? = null,
    val atcModel: String? = null,
    val atcType: String? = null,
    /** airliner / bizjet / turboprop / ga / helicopter. */
    val category: String? = null,
)

/** Ostatnie przyziemienie zatrzaśnięte przez Link; `seq` rośnie przy każdym (wykrywanie odbić). */
@Serializable data class Touchdown(
    val seq: Long = 0, val t: Long? = null, val vsFpm: Double? = null, val gForce: Double? = null,
    val pitchDeg: Double? = null, val bankDeg: Double? = null, val headingTrueDeg: Double? = null,
    val lat: Double? = null, val lon: Double? = null, val iasKt: Double? = null, val gsKt: Double? = null,
)
@Serializable data class Position(val lat: Double? = null, val lon: Double? = null, val altMslFt: Double? = null, val altAglFt: Double? = null, val altIndFt: Double? = null)
@Serializable data class Speed(val iasKt: Double? = null, val tasKt: Double? = null, val gsKt: Double? = null, val mach: Double? = null, val vsFpm: Double? = null)
@Serializable data class Attitude(val pitchDeg: Double? = null, val bankDeg: Double? = null, val hdgTrueDeg: Double? = null, val hdgMagDeg: Double? = null, val trackTrueDeg: Double? = null)
@Serializable data class ApModes(val hdg: Boolean = false, val nav: Boolean = false, val apr: Boolean = false, val loc: Boolean = false, val alt: Boolean = false, val vs: Boolean = false, val flc: Boolean = false)
@Serializable data class Autopilot(
    val master: Boolean = false, val fd: Boolean = false, val athr: Boolean = false,
    val hdgDeg: Double? = null, val altFt: Double? = null, val vsFpm: Double? = null, val spdKt: Double? = null,
    val modes: ApModes = ApModes(),
)
@Serializable data class Radio(val activeMhz: Double? = null, val standbyMhz: Double? = null)
@Serializable data class Transponder(val code: String? = null, val mode: String? = null, val ident: Boolean? = null)
@Serializable data class Radios(val com1: Radio? = null, val com2: Radio? = null, val nav1: Radio? = null, val nav2: Radio? = null, val transponder: Transponder? = null)
@Serializable data class Controls(
    val gearDown: Boolean? = null, val gearPct: Double? = null, val flapsIndex: Int? = null, val flapsPositions: Int? = null,
    val flapsPct: Double? = null, val spoilersPct: Double? = null, val spoilersArmed: Boolean? = null,
    val parkingBrake: Boolean? = null, val elevatorTrimPct: Double? = null,
    /** Dźwignie silnika 1, 0–100 % (Link ustawia wszystkie silniki naraz). */
    val throttlePct: Double? = null, val mixturePct: Double? = null, val propPct: Double? = null,
    /** Kamera MSFS: 2 kabina, 3 zewnętrzna, 4 dron, inne = pozostałe widoki. */
    val cameraState: Int? = null,
    /** Kąty kamery odczytane z MSFS (stopnie) — porównanie z wysyłanymi przy rozglądaniu. */
    val cameraPitchDeg: Double? = null, val cameraYawDeg: Double? = null,
)
@Serializable data class Lights(
    val beacon: Boolean? = null, val strobe: Boolean? = null, val nav: Boolean? = null, val landing: Boolean? = null,
    val taxi: Boolean? = null, val logo: Boolean? = null, val wing: Boolean? = null, val panel: Boolean? = null, val cabin: Boolean? = null,
)
@Serializable data class Fuel(val totalKg: Double? = null, val tanks: Map<String, Double> = emptyMap())
@Serializable data class Engine(
    val n1Pct: Double? = null, val n2Pct: Double? = null, val rpm: Double? = null, val egtC: Double? = null, val fuelFlowKgH: Double? = null,
    /** Dźwignia mocy tego silnika (starsze wersje Link jej nie wysyłają). */
    val throttlePct: Double? = null,
)
@Serializable data class Environment(val windDirDeg: Double? = null, val windSpdKt: Double? = null, val oatC: Double? = null, val qnhHpa: Double? = null)
@Serializable data class Axes(val active: Boolean = false)

/** Prędkości charakterystyczne samolotu z symulatora (kt; brak = null albo 0). */
@Serializable data class SpeedLimits(val vs0Kt: Double? = null, val vs1Kt: Double? = null, val vcKt: Double? = null, val vmoKt: Double? = null)
/** Odbiornik NAV: odchylenia w kropkach (−2,5…2,5; CDI + = kurs w prawo, GS + = ścieżka powyżej), kursy magnetyczne. */
@Serializable data class NavReceiver(
    val hasNav: Boolean = false, val hasLoc: Boolean = false, val hasGs: Boolean = false, val hasDme: Boolean = false,
    val ident: String? = null, val obsDeg: Double? = null, val locCourseDeg: Double? = null, val radialDeg: Double? = null,
    val cdiDots: Double? = null, val gsDots: Double? = null, val dmeNm: Double? = null, val toFrom: String? = null,
)
@Serializable data class AdfReceiver(val hasSignal: Boolean = false, val relBearingDeg: Double? = null)
@Serializable data class GpsData(
    val activeFlightPlan: Boolean = false, val approachActive: Boolean = false, val drivesNav1: Boolean = false,
    val nextId: String? = null, val prevId: String? = null, val nextDistNm: Double? = null, val nextBearingMagDeg: Double? = null,
    val desiredTrackMagDeg: Double? = null, val crossTrackNm: Double? = null, val nextEteSec: Double? = null, val destEteSec: Double? = null,
    /** Punkty bieżącego odcinka (0/0 = brak) i miejsce w planie lotu symulatora. */
    val nextLat: Double? = null, val nextLon: Double? = null, val prevLat: Double? = null, val prevLon: Double? = null,
    val wpIndex: Int? = null, val wpCount: Int? = null,
)
/** Polecenia dyrektora lotu: pochylenie + nos w górę, przechylenie + w prawo. */
@Serializable data class FlightDirector(val active: Boolean = false, val pitchDeg: Double? = null, val bankDeg: Double? = null)
@Serializable data class ApModesExtra(
    val altArmed: Boolean = false, val gsArmed: Boolean = false, val gsActive: Boolean = false, val backCourse: Boolean = false,
    val wingLeveler: Boolean = false, val pitchHold: Boolean = false, val airspeedHold: Boolean = false, val machHold: Boolean = false,
    val toga: Boolean = false, val yawDamper: Boolean = false, val machVar: Double? = null,
)

/** Dane dla wyświetlaczy PFD/MFD z Link (od wersji z blokiem `avionics`); starsze wersje go nie wysyłają. */
@Serializable data class Avionics(
    val baroHpa: Double? = null, val baroStd: Boolean = false, val pressureAltFt: Double? = null,
    val radioAltFt: Double? = null, val decisionHeightFt: Double? = null,
    /** Kulka −1 (lewo) … 1 (prawo), zakręt °/s (+ w prawo). */
    val slipBall: Double? = null, val turnRateDps: Double? = null, val gForce: Double? = null, val aoaDeg: Double? = null,
    val attIndPitchDeg: Double? = null, val attIndBankDeg: Double? = null, val hdgIndDeg: Double? = null,
    val suctionInHg: Double? = null, val magVarDeg: Double? = null, val tatC: Double? = null,
    val speeds: SpeedLimits = SpeedLimits(), val stallWarning: Boolean = false, val overspeedWarning: Boolean = false,
    /** none / outer / middle / inner */
    val marker: String? = null,
    val nav1: NavReceiver = NavReceiver(), val nav2: NavReceiver = NavReceiver(), val adf: AdfReceiver = AdfReceiver(),
    val gps: GpsData = GpsData(), val fd: FlightDirector = FlightDirector(), val ap: ApModesExtra = ApModesExtra(),
    val brakeLeftPct: Double? = null, val brakeRightPct: Double? = null,
)

/** Pełny stan lotu z jednego komunikatu `state`. */
@Serializable
data class FlightState(
    val source: String? = null,
    val aircraft: AircraftInfo = AircraftInfo(),
    val simTimeUtc: String? = null,
    val position: Position = Position(),
    val speed: Speed = Speed(),
    val attitude: Attitude = Attitude(),
    val autopilot: Autopilot = Autopilot(),
    val radios: Radios = Radios(),
    val controls: Controls = Controls(),
    val lights: Lights = Lights(),
    val fuel: Fuel = Fuel(),
    val engines: List<Engine> = emptyList(),
    val environment: Environment = Environment(),
    val axes: Axes = Axes(),
    val touchdown: Touchdown? = null,
    val avionics: Avionics? = null,
)

/** Koperta przychodzącej wiadomości — pola wszystkich typów, czytamy te, które pasują do `type`. */
@Serializable
data class Incoming(
    val v: Int = 1,
    val type: String = "",
    val id: String? = null,
    val seq: Long? = null,
    val t: Long? = null,
    val data: FlightState? = null,
    val token: String? = null,
    val server: String? = null,
    val version: String? = null,
    val source: String? = null,
    val rateHz: Int? = null,
    val simConnected: Boolean? = null,
    val message: String? = null,
    val code: String? = null,
    val name: String? = null,
    val limits: Limits? = null,
    /** Funkcje serwera dodane po pierwszym wydaniu protokołu (np. `logbook_sync`). */
    val features: List<String>? = null,
)

@Serializable data class Limits(val maxDeflection: Double = 0.8, val axisTimeoutMs: Int = 500)

@Serializable data class ClientInfo(val name: String, val app: String = "OpenTakeoff", val version: String? = null)

@Serializable
data class Hello(
    val v: Int = 1, val type: String = "hello", val id: String = "1",
    val code: String? = null, val token: String? = null,
    val client: ClientInfo, val rateHz: Int = 10,
)

@Serializable data class Ping(val v: Int = 1, val type: String = "ping", val id: String, val t: Long)

@Serializable data class Command(val v: Int = 1, val type: String = "command", val id: String, val name: String, val value: Double? = null)

@Serializable
data class AxisMessage(
    val v: Int = 1, val type: String = "axis",
    val aileron: Double? = null, val elevator: Double? = null, val rudder: Double? = null,
    /** Hamulec aerodynamiczny (spoilery) i hamulce kół, 0…1 — osobny właściciel, nie przejmują lotek. */
    val speedbrake: Double? = null, val brake: Double? = null,
    val release: Boolean? = null,
)
