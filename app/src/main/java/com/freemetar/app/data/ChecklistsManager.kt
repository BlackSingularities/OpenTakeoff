package com.freemetar.app.data

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * Checklists manager - ładowanie i matching checklistów do samolotów.
 */
class ChecklistsManager(private val context: Context) {

    private val json = Json { ignoreUnknownKeys = true }
    private val checklistsCache = mutableMapOf<String, AircraftChecklists>()

    /** Pliki checklistów w assets/checklists z nazwą samolotu do ręcznego wyboru. */
    val available = listOf(
        "airbus-a320.json" to "Airbus A320",
        "boeing-737-800.json" to "Boeing 737-800",
        "cessna-172.json" to "Cessna 172",
    )

    /** Checklisty z konkretnego pliku — ręczny wybór, gdy symulator nie podaje samolotu. */
    suspend fun load(filename: String): AircraftChecklists? = withContext(Dispatchers.IO) {
        runCatching {
            context.assets.open("checklists/$filename").bufferedReader().use { json.decodeFromString<AircraftChecklists>(it.readText()) }
        }.getOrNull()
    }

    /**
     * Załaduj checklisty dla samolotu z MSFS.
     *
     * @param aircraftTitle Nazwa samolotu z MSFS (np. "Boeing 737-800")
     * @return Checklisty lub null jeśli nie znaleziono
     */
    suspend fun getChecklists(aircraftTitle: String?): AircraftChecklists? = withContext(Dispatchers.IO) {
        if (aircraftTitle == null) return@withContext null

        // Sprawdź cache
        checklistsCache[aircraftTitle]?.let { return@withContext it }

        // Lista dostępnych checklistów
        val availableChecklists = available.map { it.first }

        // Znajdź matching checklist
        for (filename in availableChecklists) {
            try {
                val inputStream = context.assets.open("checklists/$filename")
                val jsonText = inputStream.bufferedReader().use { it.readText() }
                val checklists = json.decodeFromString<AircraftChecklists>(jsonText)

                // Sprawdź czy nazwa samolotu pasuje
                if (matchesAircraft(aircraftTitle, checklists.msfs_matches)) {
                    checklistsCache[aircraftTitle] = checklists
                    return@withContext checklists
                }
            } catch (e: Exception) {
                e.printStackTrace()
            }
        }

        // Fallback: próbuj dopasować po kategorii
        val category = guessCategory(aircraftTitle)
        return@withContext getFallbackChecklists(category)
    }

    /**
     * Sprawdź czy nazwa samolotu pasuje do wzorców.
     */
    private fun matchesAircraft(aircraftTitle: String, patterns: List<String>): Boolean {
        val normalized = aircraftTitle.lowercase()
        return patterns.any { pattern ->
            normalized.contains(pattern.lowercase())
        }
    }

    /**
     * Zgadnij kategorię samolotu po nazwie.
     */
    private fun guessCategory(aircraftTitle: String): String {
        val lower = aircraftTitle.lowercase()
        return when {
            lower.contains("737") || lower.contains("320") || lower.contains("airbus") || lower.contains("boeing") -> "jet_airliner"
            lower.contains("cessna") || lower.contains("172") || lower.contains("piper") -> "ga_single"
            lower.contains("king air") || lower.contains("tbm") || lower.contains("caravan") -> "turboprop"
            else -> "generic"
        }
    }

    /**
     * Zwróć fallback checklisty dla kategorii.
     */
    private suspend fun getFallbackChecklists(category: String): AircraftChecklists? {
        val fallbackFile = when (category) {
            "jet_airliner" -> "boeing-737-800.json"
            "ga_single" -> "cessna-172.json"
            else -> null
        }

        return fallbackFile?.let {
            try {
                val inputStream = context.assets.open("checklists/$it")
                val jsonText = inputStream.bufferedReader().use { it.readText() }
                json.decodeFromString<AircraftChecklists>(jsonText)
            } catch (e: Exception) {
                null
            }
        }
    }
}

@Serializable
data class AircraftChecklists(
    val aircraft: String,
    val msfs_matches: List<String>,
    val manufacturer: String,
    val category: String,
    val checklists: Map<String, Checklist>
)

@Serializable
data class Checklist(
    val name_pl: String,
    val name_en: String,
    val name_es: String = name_en,
    val phase: String,
    val items: List<ChecklistItem>
)

@Serializable
data class ChecklistItem(
    val item_pl: String,
    val item_en: String,
    val item_es: String = item_en,
    val expected: String,
    val checked: Boolean = false
)
