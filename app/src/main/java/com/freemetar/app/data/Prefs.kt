package com.freemetar.app.data

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

private val Context.dataStore by preferencesDataStore("settings")

enum class PressureUnit { HPA, INHG }
enum class AltUnit { FT, M }
enum class ThemeMode { SYSTEM, DAY, NIGHT }

data class Settings(
    val favorites: List<String> = listOf("EPWA", "EPKK", "EPGD"),
    val recent: List<String> = emptyList(),
    val pressureUnit: PressureUnit = PressureUnit.HPA,
    val altUnit: AltUnit = AltUnit.FT,
    val themeMode: ThemeMode = ThemeMode.SYSTEM,
    val autoRefresh: Boolean = true,
    /** Ręcznie wybrany profil panelu (null = automatycznie z danych samolotu). */
    val aircraftProfile: String? = null,
    /** SYSTEM / PL / EN */
    val language: String = "SYSTEM",
    /** Sterowanie samolotem: TILT (żyroskop, domyślnie) / TOUCH (drążek palcem) / GAMEPAD. */
    val controlMode: String = "TILT",
    /** Pływający przycisk sterowania nad wszystkimi zakładkami. */
    val floatingControl: Boolean = false,
    /** Pasek rozdziałów briefingu zawsze widoczny (inaczej pokazuje się tylko podczas przewijania). */
    val pinJumpBar: Boolean = false,
    /** SIMPLE — sam przycisk sterowania; TRIM — dodatkowo przyciski trymu wysokości. */
    val floatingStyle: String = "SIMPLE",
    /** Pasek stanu ukryty — aplikacja zajmuje jego miejsce (wysuwa się przeciągnięciem od góry). */
    val hideStatusBar: Boolean = false,
    /** Dźwignie mocy po lewej stronie PFD i ND (domyślnie po prawej). */
    val leversLeft: Boolean = false,
    /** Odstęp między punktami zapisu lotu (ms). */
    val recordIntervalMs: Int = 1000,
    /** Czy ekran powitalny został już pokazany. */
    val welcomeShown: Boolean = false,
    /** Zapowiedzi głosowe (TTS) w locie — domyślnie wyłączone. */
    val calloutsEnabled: Boolean = false,
    /** Włączone zapowiedzi (identyfikatory z [CalloutsList]). */
    val enabledCallouts: Set<String> = CalloutsList.ALL.map { it.first }.toSet(),
    val calloutsSpeechRate: Float = 1.0f,
)

class Prefs(private val context: Context) {
    private val FAV = stringPreferencesKey("favorites")
    private val RECENT = stringPreferencesKey("recent")
    private val PRESS = stringPreferencesKey("pressure")
    private val ALT = stringPreferencesKey("alt")
    private val THEME = stringPreferencesKey("theme")
    private val AUTO = booleanPreferencesKey("auto_refresh")
    private val PROFILE = stringPreferencesKey("aircraft_profile")
    private val LANGUAGE = stringPreferencesKey("language")
    private val LINK_TOKENS = stringPreferencesKey("link_tokens")   // ip=token;ip=token
    private val LINK_LAST = stringPreferencesKey("link_last_ip")
    private val CONTROL = stringPreferencesKey("control_mode")
    private val FLOATING = booleanPreferencesKey("floating_control")
    private val PIN_JUMPBAR = booleanPreferencesKey("pin_jump_bar")
    private val RECORD_MS = intPreferencesKey("record_interval_ms")
    private val FLOAT_STYLE = stringPreferencesKey("floating_style")
    private val HIDE_STATUS = booleanPreferencesKey("hide_status_bar")
    private val LEVERS_LEFT = booleanPreferencesKey("levers_left")
    private val WELCOME_SHOWN = booleanPreferencesKey("welcome_shown")
    private val CALLOUTS_ON = booleanPreferencesKey("callouts_enabled")
    private val CALLOUTS_LIST = stringPreferencesKey("callouts_list")
    private val CALLOUTS_RATE = floatPreferencesKey("callouts_rate")

    private fun list(v: String?) = v?.split(",")?.filter { it.isNotBlank() }

    val settings: Flow<Settings> = context.dataStore.data.map { p ->
        Settings(
            favorites = list(p[FAV]) ?: Settings().favorites,
            recent = list(p[RECENT]).orEmpty(),
            pressureUnit = p[PRESS]?.let { runCatching { PressureUnit.valueOf(it) }.getOrNull() } ?: PressureUnit.HPA,
            altUnit = p[ALT]?.let { runCatching { AltUnit.valueOf(it) }.getOrNull() } ?: AltUnit.FT,
            themeMode = p[THEME]?.let { runCatching { ThemeMode.valueOf(it) }.getOrNull() } ?: ThemeMode.SYSTEM,
            autoRefresh = p[AUTO] ?: true,
            aircraftProfile = p[PROFILE],
            language = p[LANGUAGE] ?: "SYSTEM",
            controlMode = p[CONTROL] ?: "TILT",
            floatingControl = p[FLOATING] ?: false,
            pinJumpBar = p[PIN_JUMPBAR] ?: false,
            floatingStyle = p[FLOAT_STYLE] ?: "SIMPLE",
            hideStatusBar = p[HIDE_STATUS] ?: false,
            leversLeft = p[LEVERS_LEFT] ?: false,
            recordIntervalMs = p[RECORD_MS] ?: 1000,
            welcomeShown = p[WELCOME_SHOWN] ?: false,
            calloutsEnabled = p[CALLOUTS_ON] ?: false,
            // Pusty zapis = wszystkie wyłączone; brak klucza = domyślna lista
            enabledCallouts = p[CALLOUTS_LIST]?.split(",")?.filter { it.isNotBlank() }?.toSet() ?: Settings().enabledCallouts,
            calloutsSpeechRate = p[CALLOUTS_RATE] ?: 1.0f,
        )
    }

    suspend fun toggleFavorite(ident: String) = context.dataStore.edit { p ->
        val cur = list(p[FAV]) ?: Settings().favorites
        p[FAV] = (if (ident in cur) cur - ident else cur + ident).joinToString(",")
    }

    suspend fun moveFavorite(ident: String, delta: Int) = context.dataStore.edit { p ->
        val cur = (list(p[FAV]) ?: Settings().favorites).toMutableList()
        val i = cur.indexOf(ident)
        val j = i + delta
        if (i >= 0 && j in cur.indices) {
            cur.removeAt(i); cur.add(j, ident)
            p[FAV] = cur.joinToString(",")
        }
    }

    suspend fun addRecent(ident: String) = context.dataStore.edit { p ->
        val cur = list(p[RECENT]).orEmpty()
        p[RECENT] = (listOf(ident) + (cur - ident)).take(10).joinToString(",")
    }

    suspend fun clearRecent() = context.dataStore.edit { it.remove(RECENT) }
    suspend fun setPressure(u: PressureUnit) = context.dataStore.edit { it[PRESS] = u.name }
    suspend fun setAlt(u: AltUnit) = context.dataStore.edit { it[ALT] = u.name }
    suspend fun setTheme(m: ThemeMode) = context.dataStore.edit { it[THEME] = m.name }
    suspend fun setAutoRefresh(v: Boolean) = context.dataStore.edit { it[AUTO] = v }
    private fun tokenMap(v: String?): Map<String, String> =
        v?.split(";")?.mapNotNull { e -> e.split("=", limit = 2).takeIf { it.size == 2 }?.let { it[0] to it[1] } }?.toMap().orEmpty()

    /** Zapis tokenów parowania OpenTakeoff Link — osobno dla każdego komputera. */
    val linkTokens = object : com.freemetar.app.data.link.LinkTokenStore {
        override suspend fun token(ip: String): String? = tokenMap(context.dataStore.data.first()[LINK_TOKENS])[ip]
        override suspend fun saveToken(ip: String, token: String) {
            context.dataStore.edit { p ->
                p[LINK_TOKENS] = (tokenMap(p[LINK_TOKENS]) + (ip to token)).entries.joinToString(";") { "${it.key}=${it.value}" }
                p[LINK_LAST] = ip
            }
        }
        override suspend fun clearToken(ip: String) {
            context.dataStore.edit { p -> p[LINK_TOKENS] = (tokenMap(p[LINK_TOKENS]) - ip).entries.joinToString(";") { "${it.key}=${it.value}" } }
        }
    }

    suspend fun lastLinkIp(): String? = context.dataStore.data.first()[LINK_LAST]

    suspend fun setControlMode(v: String) = context.dataStore.edit { it[CONTROL] = v }
    suspend fun setFloatingControl(v: Boolean) = context.dataStore.edit { it[FLOATING] = v }
    suspend fun setPinJumpBar(v: Boolean) = context.dataStore.edit { it[PIN_JUMPBAR] = v }
    suspend fun setHideStatusBar(v: Boolean) = context.dataStore.edit { it[HIDE_STATUS] = v }
    suspend fun setLeversLeft(v: Boolean) = context.dataStore.edit { it[LEVERS_LEFT] = v }
    suspend fun setFloatingStyle(v: String) = context.dataStore.edit { it[FLOAT_STYLE] = v }
    suspend fun setRecordInterval(ms: Int) = context.dataStore.edit { it[RECORD_MS] = ms }
    suspend fun setLanguage(v: String) = context.dataStore.edit { it[LANGUAGE] = v }
    suspend fun setAircraftProfile(name: String?) = context.dataStore.edit { if (name == null) it.remove(PROFILE) else it[PROFILE] = name }
    suspend fun setWelcomeShown(shown: Boolean) = context.dataStore.edit { it[WELCOME_SHOWN] = shown }
    suspend fun setCalloutsEnabled(v: Boolean) = context.dataStore.edit { it[CALLOUTS_ON] = v }
    suspend fun setCallout(id: String, on: Boolean) = context.dataStore.edit { p ->
        val cur = p[CALLOUTS_LIST]?.split(",")?.filter { it.isNotBlank() }?.toSet() ?: Settings().enabledCallouts
        p[CALLOUTS_LIST] = (if (on) cur + id else cur - id).joinToString(",")
    }
    suspend fun setCalloutsSpeechRate(v: Float) = context.dataStore.edit { it[CALLOUTS_RATE] = v }
}
