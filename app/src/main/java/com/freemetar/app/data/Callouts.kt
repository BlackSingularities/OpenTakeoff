package com.freemetar.app.data

import android.content.Context
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.domain.PositiveRateDetector
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.util.*
import kotlin.math.abs

/**
 * Callouts audio - automatyczne zapowiedzi głosowe podczas lotu.
 *
 * Typy callouts:
 * - Takeoff: "80 knots", "Positive rate"
 * - Landing: "1000", "500", "100", "50", "40", "30", "20", "10", "Retard"
 * - Warnings: "Stall", "Overspeed", "Terrain"
 * - Autopilot: "Autopilot", "Autothrottle"
 *
 * Konfiguracja w Settings:
 * - Włącz/wyłącz całość
 * - Wybór poszczególnych callouts
 * - Głośność
 *
 * Zapowiedzi są zawsze czytane po angielsku.
 */
class Callouts(private val context: Context) {

    private var tts: TextToSpeech? = null
    private var ttsReady = false
    private val _isEnabled = MutableStateFlow(false)
    val isEnabled: StateFlow<Boolean> = _isEnabled

    // Ostatni stan - do detekcji zmian
    private var lastAltAgl: Double? = null
    private var lastIas: Double? = null
    private var lastApMaster: Boolean? = null
    private var lastAthr: Boolean? = null

    // Cooldown - nie powtarzaj tego samego callout w ciągu 5 sekund
    private val calledRecently = mutableMapOf<String, Long>()
    private val cooldownMs = 5000L

    // Konfiguracja - które callouts są włączone
    private val enabledCallouts = mutableSetOf(
        "1000", "500", "100", "50", "40", "30", "20", "10",
        "Positive rate",
        "Retard",
        "Autopilot", "Autothrottle"
    )
    private val positiveRate = PositiveRateDetector()
    private var speechRate = 1.0f

    /** Silnik mowy startuje dopiero po włączeniu zapowiedzi — wyłączone nie kosztują nic. */
    private fun initializeTts() {
        if (tts != null) return
        tts = TextToSpeech(context) { status ->
            if (status == TextToSpeech.SUCCESS) {
                tts?.language = Locale.US
                tts?.setSpeechRate(speechRate)
                tts?.setPitch(1.0f)
                ttsReady = true

                tts?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                    override fun onStart(utteranceId: String?) {}
                    override fun onDone(utteranceId: String?) {}
                    override fun onError(utteranceId: String?) {}
                })
            }
        }
    }

    /**
     * Aktualizuj callouts na podstawie bieżącego stanu lotu.
     * Wywołuj co ~100ms z FlightRecorder lub LinkClient.
     */
    fun update(state: FlightState) {
        if (!_isEnabled.value || !ttsReady) return

        val agl = state.position.altAglFt
        val ias = state.speed.iasKt
        val vs = state.speed.vsFpm
        val onGround = state.aircraft.onGround
        val apMaster = state.autopilot.master
        val athr = state.autopilot.athr

        // Landing callouts (gdy zniżamy się)
        if (agl != null && vs != null && vs < -100 && onGround == false) {
            checkAltitudeCallout(agl)
        }

        // Takeoff callouts
        if (onGround == true && ias != null) {
            checkTakeoffCallout(ias)
        }
        if (positiveRate.update(onGround, ias, vs)) {
            call("Positive rate")
        }

        // Autopilot/Autothrottle
        if (apMaster != lastApMaster && apMaster == true) {
            call("Autopilot")
        }
        if (athr != lastAthr && athr == true) {
            call("Autothrottle")
        }

        // Zapisz ostatni stan
        lastAltAgl = agl
        lastIas = ias
        lastApMaster = apMaster
        lastAthr = athr
    }

    /**
     * Callouts wysokości przy lądowaniu.
     */
    private fun checkAltitudeCallout(agl: Double) {
        val callout = when {
            agl in 990.0..1010.0 -> "1000"
            agl in 490.0..510.0 -> "500"
            agl in 95.0..105.0 -> "100"
            agl in 45.0..55.0 -> "50"
            agl in 35.0..45.0 -> "40"
            agl in 25.0..35.0 -> "30"
            agl in 15.0..25.0 -> "20"
            agl in 5.0..15.0 -> "10"
            agl in 0.0..5.0 && lastAltAgl != null && lastAltAgl!! > 10.0 -> "Retard"
            else -> null
        }

        callout?.let { call(it) }
    }

    /**
     * Callouts podczas startu.
     */
    private fun checkTakeoffCallout(ias: Double) {
        val callout = when {
            ias in 78.0..82.0 && (lastIas == null || lastIas!! < 78.0) -> "80 knots"
            else -> null
        }

        callout?.let { call(it) }
    }

    /**
     * Wykonaj callout (jeśli nie był niedawno).
     */
    private fun call(text: String) {
        if (!enabledCallouts.contains(text)) return

        val now = System.currentTimeMillis()
        val lastCalled = calledRecently[text] ?: 0L

        if (now - lastCalled > cooldownMs) {
            speak(text)
            calledRecently[text] = now
        }
    }

    /**
     * Wypowiedz tekst przez TTS.
     */
    private fun speak(text: String) {
        tts?.speak(text, TextToSpeech.QUEUE_ADD, null, text)
    }

    /**
     * Włącz/wyłącz callouts.
     */
    fun setEnabled(enabled: Boolean) {
        _isEnabled.value = enabled
        if (enabled) initializeTts() else tts?.stop()
    }

    /** Ustawienia zapisane w [Prefs] — wywoływane przy każdej ich zmianie. */
    fun configure(enabled: Boolean, callouts: Set<String>, rate: Float) {
        enabledCallouts.clear(); enabledCallouts.addAll(callouts)
        setSpeechRate(rate)
        setEnabled(enabled)
    }

    /**
     * Włącz/wyłącz konkretny callout.
     */
    fun setCalloutEnabled(callout: String, enabled: Boolean) {
        if (enabled) {
            enabledCallouts.add(callout)
        } else {
            enabledCallouts.remove(callout)
        }
    }

    /**
     * Zmień prędkość mowy.
     */
    fun setSpeechRate(rate: Float) {
        speechRate = rate
        tts?.setSpeechRate(rate)
    }

    /**
     * Cleanup.
     */
    fun shutdown() {
        tts?.stop()
        tts?.shutdown()
        tts = null
        ttsReady = false
    }
}

/**
 * Lista wszystkich dostępnych callouts.
 */
object CalloutsList {
    val LANDING = listOf(
        "1000" to "1000 feet",
        "500" to "500 feet",
        "100" to "100 feet",
        "50" to "50 feet",
        "40" to "40 feet",
        "30" to "30 feet",
        "20" to "20 feet",
        "10" to "10 feet",
        "Retard" to "Retard thrust"
    )

    val TAKEOFF = listOf(
        "80 knots" to "80 knots",
        "Positive rate" to "Positive rate - gear up"
    )

    val AUTOPILOT = listOf(
        "Autopilot" to "Autopilot engaged",
        "Autothrottle" to "Autothrottle engaged"
    )

    val ALL = LANDING + TAKEOFF + AUTOPILOT
}
