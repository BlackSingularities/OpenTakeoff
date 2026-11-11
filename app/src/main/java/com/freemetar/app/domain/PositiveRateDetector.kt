package com.freemetar.app.domain

import kotlin.math.min

/**
 * Wykrywa moment zapowiedzi „Positive rate”: wznoszenie po oderwaniu się od pasa.
 * Uzbraja się dopiero po rozpędzaniu na ziemi, więc odbicie przy lądowaniu jej nie wywoła.
 */
class PositiveRateDetector {
    private var lowestGroundIas: Double? = null
    private var armed = false

    /** Zwraca true dokładnie raz na start. */
    fun update(onGround: Boolean?, ias: Double?, vs: Double?): Boolean {
        when (onGround) {
            true -> {
                if (ias != null) {
                    val lowest = min(lowestGroundIas ?: ias, ias)
                    lowestGroundIas = lowest
                    if (ias >= lowest + MIN_ACCELERATION_KT) armed = true
                }
                return false
            }
            false -> {
                lowestGroundIas = null
                if (!armed || vs == null) return false
                if (vs < -CLIMB_THRESHOLD_FPM) armed = false
                if (vs > CLIMB_THRESHOLD_FPM) {
                    armed = false
                    return true
                }
                return false
            }
            null -> return false
        }
    }

    private companion object {
        const val MIN_ACCELERATION_KT = 10.0
        const val CLIMB_THRESHOLD_FPM = 100.0
    }
}
