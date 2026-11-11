package com.freemetar.app.domain

/** Panel autopilota jak w danej kabinie; NONE — samolot bez autopilota (śmigłowiec R44, szybowiec). */
enum class AutopilotKind { FCU, MCP, MCP_787, ATR_FGCP, PRIMUS, GFC700, KAP140, NONE }

/** Dźwignie inne niż moc — ustawiane naraz dla wszystkich silników. */
enum class Lever { PROP, MIXTURE, CONDITION }

enum class Light { BEACON, STROBE, NAV, LANDING, TAXI, LOGO, WING, PANEL, CABIN }

/**
 * Profil samolotu = jeden zestaw wyświetlaczy (PFD i ND/MFD) i wszystko, co do niego pasuje w kabinie:
 * autopilot, radia, systemy, liczba silników i dźwignie. Jeden profil na każdy styl wyświetlaczy.
 */
enum class AircraftProfile(
    private val labelOf: () -> String,
    val short: String,
    val autopilot: AutopilotKind,
    /** Liczba silników, gdy symulator jej nie podaje. */
    val engines: Int,
    val jet: Boolean,
    val levers: List<Lever>,
    /** Zapadki dźwigni mocy (0–100 %) z podpisami. */
    val detents: List<Pair<Int, String>>,
    val hasAutothrottle: Boolean,
    val hasReverser: Boolean,
    /** Spoilery / hamulce aerodynamiczne. */
    val hasSpoilers: Boolean,
    val retractableGear: Boolean,
    val hasFlaps: Boolean,
    val hasTrim: Boolean,
    val com2: Boolean,
    val nav: Boolean,
    val nav2: Boolean,
    val lights: List<Light>,
) {
    A320({ "Airbus A320" }, "A320", AutopilotKind.FCU, 2, true, emptyList(),
        listOf(0 to "IDLE", 70 to "CL", 86 to "FLX", 100 to "TOGA"), true, true, true, true, true, true, true, true, true, Light.entries),
    B737({ "Boeing 737" }, "737", AutopilotKind.MCP, 2, true, emptyList(),
        listOf(0 to "IDLE", 100 to "MAX"), true, true, true, true, true, true, true, true, true, Light.entries),
    B787({ "Boeing 787" }, "787", AutopilotKind.MCP_787, 2, true, emptyList(),
        listOf(0 to "IDLE", 100 to "MAX"), true, true, true, true, true, true, true, true, true, Light.entries),
    ATR72({ "ATR 72" }, "ATR", AutopilotKind.ATR_FGCP, 2, false, listOf(Lever.CONDITION),
        listOf(0 to "FI", 72 to "CLB", 86 to "MCT", 100 to "RAMP"), false, true, false, true, true, true, true, true, true, Light.entries - Light.CABIN),
    E190({ "Embraer E190" }, "E190", AutopilotKind.PRIMUS, 2, true, emptyList(),
        listOf(0 to "IDLE", 75 to "CLB", 100 to "TOGA"), true, true, true, true, true, true, true, true, true, Light.entries),
    G1000({ loc("aircraft_profile.garmin_g1000_glass_cockpit") }, "G1000", AutopilotKind.GFC700, 1, false, listOf(Lever.PROP, Lever.MIXTURE),
        listOf(0 to "IDLE", 100 to "FULL"), false, false, false, true, true, true, true, true, true,
        listOf(Light.BEACON, Light.STROBE, Light.NAV, Light.LANDING, Light.TAXI, Light.PANEL, Light.CABIN)),
    C172({ loc("aircraft_profile.cessna_172_analog_gauges") }, "C172", AutopilotKind.KAP140, 1, false, listOf(Lever.MIXTURE),
        listOf(0 to "IDLE", 100 to "FULL"), false, false, false, false, true, true, true, true, true,
        listOf(Light.BEACON, Light.STROBE, Light.NAV, Light.LANDING, Light.TAXI, Light.PANEL)),
    R44({ loc("aircraft_profile.helicopter_r44") }, "R44", AutopilotKind.NONE, 1, false, listOf(Lever.MIXTURE),
        listOf(0 to "IDLE", 100 to "FULL"), false, false, false, false, false, false, true, true, false,
        listOf(Light.BEACON, Light.STROBE, Light.NAV, Light.LANDING, Light.PANEL)),
    GLIDER({ loc("aircraft_profile.glider") }, "GLD", AutopilotKind.NONE, 0, false, emptyList(),
        emptyList(), false, false, true, false, true, true, false, false, false, listOf(Light.NAV)),
    ;

    /** Nazwa w bieżącym języku (lambda, bo enum powstaje raz). */
    val label: String get() = labelOf()

    /** Czy jest dźwignia mocy (szybowiec jej nie ma). */
    val hasThrottle get() = engines > 0

    companion object {
        private val airbus = Regex("""\b(airbus|a3[0-9]{2}|a2[0-9]{2}|a220|fenix|fbw|flybywire)""", RegexOption.IGNORE_CASE)
        private val heavyBoeing = Regex("""\b(b?78[789]|b?77[0-9w]|b?74[0-9]|dreamliner)""", RegexOption.IGNORE_CASE)
        private val boeing = Regex("""\b(boeing|b?7[0-9]7|pmdg|md-?11|md-?8[0-9])""", RegexOption.IGNORE_CASE)
        private val atr = Regex("""\b(atr\s?-?\s?[47]2)""", RegexOption.IGNORE_CASE)
        private val primus = Regex("""\b(e-?1[79]0|e-?1[79]5|erj|embraer|crj|q400|dash ?8|dhc-?8)""", RegexOption.IGNORE_CASE)
        private val glider = Regex("""\b(glider|szybowiec|ask ?2[1-3]|ls ?[468]|discus|dg-?[0-9]{3}|arcus|duo|ventus|nimbus|swift s1|astir|jantar|pirat|bocian|junior|grob 10[23])""", RegexOption.IGNORE_CASE)
        /** Samoloty z zegarami analogowymi w MSFS (bez szklanego kokpitu). */
        private val classic = Regex("""\b(classic|steam|analog|c152|cessna 152|cub|j-?3|super cub|citabria|decathlon|extra ?330|cap ?10|zlin|savage|husky|aviat|pitts|dr1|stearman|spitfire|p-?51|vl3|icon a5|wilga|an-?2)""", RegexOption.IGNORE_CASE)

        /**
         * Wybór z danych Link: śmigłowiec i szybowiec po kategorii, potem typ rozpoznany po nazwie,
         * na końcu kategoria i typ silnika. Bez danych — G1000 jako najczęstszy kokpit GA.
         */
        fun detect(title: String?, atcModel: String?, atcType: String?, category: String?, engineType: String?): AircraftProfile {
            val text = listOfNotNull(title, atcModel, atcType).joinToString(" ")
            return when {
                category == "helicopter" || engineType == "helicopter" -> R44
                glider.containsMatchIn(text) || (engineType == "none" && category == "ga") -> GLIDER
                airbus.containsMatchIn(text) -> A320
                heavyBoeing.containsMatchIn(text) -> B787
                boeing.containsMatchIn(text) -> B737
                atr.containsMatchIn(text) -> ATR72
                primus.containsMatchIn(text) -> E190
                category == "airliner" -> if (engineType == "turboprop") ATR72 else A320
                classic.containsMatchIn(text) -> C172
                else -> G1000
            }
        }

        /** Zapisany wybór; nazwy dawnych profili (AIRBUS, BOEING) przechodzą na nowe, pozostałe na automat. */
        fun fromName(name: String?): AircraftProfile? = when (name) {
            "AIRBUS" -> A320
            "BOEING" -> B737
            else -> entries.firstOrNull { it.name == name }
        }
    }
}
