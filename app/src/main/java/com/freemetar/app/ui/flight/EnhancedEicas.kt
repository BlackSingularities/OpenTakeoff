package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.drawText
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.domain.AircraftProfile
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.roundToInt

/**
 * EICAS/ECAM z przełączaniem widoków systemów.
 *
 * Widoki:
 * - ENG: Silniki (N1/N2, EGT, paliwo, przepływ) - domyślny
 * - ELEC: Elektryka (generatory, bateria, voltaż, ampery)
 * - HYD: Hydraulika (ciśnienie układów A/B/C)
 * - FUEL: Paliwo (zbiorniki, pompy, zawory)
 * - DOOR: Drzwi i podwozie (status drzwi, podwozia, klapy cargo)
 * - STATUS: Podsumowanie (aktywne systemy, ostrzeżenia)
 *
 * Adaptacja do samolotu:
 * - Cessna 172: tylko ENG (podstawowy)
 * - Boeing 737: ENG, ELEC, HYD, FUEL
 * - Airbus A320: wszystkie widoki (ECAM)
 */

enum class EicasView {
    ENG,     // Silniki
    ELEC,    // Elektryka
    HYD,     // Hydraulika
    FUEL,    // Paliwo
    DOOR,    // Drzwi i podwozie
    STATUS   // Status systemów
}

@Composable
fun EnhancedEicas(
    state: FlightState,
    profile: AircraftProfile,
    modifier: Modifier = Modifier
) {
    var currentView by remember { mutableStateOf(EicasView.ENG) }
    val c = Fm.c

    // Dostępne widoki zależnie od profilu samolotu
    val availableViews = when (profile) {
        AircraftProfile.C172, AircraftProfile.R44, AircraftProfile.GLIDER -> listOf(EicasView.ENG) // Tylko silniki
        AircraftProfile.G1000, AircraftProfile.ATR72 -> listOf(EicasView.ENG, EicasView.FUEL)
        else -> listOf(EicasView.ENG, EicasView.ELEC, EicasView.HYD, EicasView.FUEL, EicasView.DOOR, EicasView.STATUS)
    }

    Column(modifier) {
        // Przyciski przełączania widoków (tylko jeśli więcej niż 1)
        if (availableViews.size > 1) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                availableViews.forEach { view ->
                    EicasViewButton(
                        label = when (view) {
                            EicasView.ENG -> "ENG"
                            EicasView.ELEC -> "ELEC"
                            EicasView.HYD -> "HYD"
                            EicasView.FUEL -> "FUEL"
                            EicasView.DOOR -> "DOOR"
                            EicasView.STATUS -> "STS"
                        },
                        active = currentView == view,
                        onClick = { currentView = view },
                        modifier = Modifier.weight(1f)
                    )
                }
            }
        }

        // Główny wyświetlacz EICAS
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(1.3f)
                .clip(RoundedCornerShape(12.dp))
                .background(Color(0xFF05070A))
        ) {
            when (currentView) {
                EicasView.ENG -> EicasEngineView(state, profile)
                EicasView.ELEC -> EicasElecView(state)
                EicasView.HYD -> EicasHydView(state)
                EicasView.FUEL -> EicasFuelView(state)
                EicasView.DOOR -> EicasDoorView(state)
                EicasView.STATUS -> EicasStatusView(state)
            }
        }
    }
}

@Composable
private fun EicasViewButton(
    label: String,
    active: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    Box(
        modifier
            .height(32.dp)
            .clip(RoundedCornerShape(4.dp))
            .background(if (active) Color(0xFF4FD1E8) else Color(0xFF1A1E24))
            .clickable(onClick = onClick)
            .padding(4.dp),
        contentAlignment = Alignment.Center
    ) {
        Text(
            label,
            color = if (active) Color.Black else Color(0xFF00FF00),
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = Mono
        )
    }
}

/**
 * Widok ENG - silniki (istniejący kod z EnginePanel.kt)
 */
@Composable
private fun EicasEngineView(state: FlightState, profile: AircraftProfile) {
    // Użyj istniejącą funkcję Eicas
    Eicas(state, profile)
}

/**
 * Widok ELEC - elektryka (placeholder)
 */
@Composable
private fun EicasElecView(state: FlightState) {
    val m = rememberTextMeasurer()
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val center = Offset(w / 2, h / 2)

        // TODO: Implement electrical system view
        // - Generatory (GEN 1, GEN 2, APU GEN)
        // - Bateria (BAT 1, BAT 2)
        // - Voltaż i ampery
        // - Bus ties

        drawText(
            m.measure("ELEC", androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 14.sp, fontFamily = Mono)),
            topLeft = Offset(center.x - 20, 20f)
        )
        drawText(
            m.measure(loc("common.in_development"), androidx.compose.ui.text.TextStyle(color = Color.Gray, fontSize = 11.sp)),
            topLeft = Offset(center.x - 50, center.y)
        )
    }
}

/**
 * Widok HYD - hydraulika (placeholder)
 */
@Composable
private fun EicasHydView(state: FlightState) {
    val m = rememberTextMeasurer()
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val center = Offset(w / 2, h / 2)

        // TODO: Implement hydraulic system view
        // - System A/B/C pressure (PSI)
        // - Pumps status
        // - Fluid quantity

        drawText(
            m.measure("HYD", androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 14.sp, fontFamily = Mono)),
            topLeft = Offset(center.x - 15, 20f)
        )
        drawText(
            m.measure(loc("common.in_development"), androidx.compose.ui.text.TextStyle(color = Color.Gray, fontSize = 11.sp)),
            topLeft = Offset(center.x - 50, center.y)
        )
    }
}

/**
 * Widok FUEL - paliwo (placeholder)
 */
@Composable
private fun EicasFuelView(state: FlightState) {
    val m = rememberTextMeasurer()
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val center = Offset(w / 2, h / 2)

        // TODO: Implement fuel system view
        // - Zbiorniki (Left, Center, Right)
        // - Pompy (Fuel pumps 1/2)
        // - Zawory (Crossfeed)
        // - Total fuel remaining

        drawText(
            m.measure("FUEL", androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 14.sp, fontFamily = Mono)),
            topLeft = Offset(center.x - 20, 20f)
        )
        drawText(
            m.measure(loc("common.in_development"), androidx.compose.ui.text.TextStyle(color = Color.Gray, fontSize = 11.sp)),
            topLeft = Offset(center.x - 50, center.y)
        )
    }
}

/**
 * Widok DOOR - drzwi i podwozie (placeholder)
 */
@Composable
private fun EicasDoorView(state: FlightState) {
    val m = rememberTextMeasurer()
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val center = Offset(w / 2, h / 2)

        // TODO: Implement doors & gear view
        // - Landing gear (nose, left, right)
        // - Doors (cabin, cargo, avionics)
        // - All green = all closed/secure

        drawText(
            m.measure("DOOR", androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 14.sp, fontFamily = Mono)),
            topLeft = Offset(center.x - 20, 20f)
        )

        // Pokazuj podwozie (już mamy dane)
        val gearDown = state.controls.gearDown
        val gearPct = state.controls.gearPct
        val gearColor = if (gearDown == true && (gearPct ?: 0.0) >= 99) Color.Green else Color(0xFFFFB020)

        drawText(
            m.measure("GEAR", androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 12.sp, fontFamily = Mono)),
            topLeft = Offset(30f, center.y - 40)
        )
        drawText(
            m.measure(
                if (gearDown == true) "DOWN" else "UP",
                androidx.compose.ui.text.TextStyle(color = gearColor, fontSize = 11.sp, fontFamily = Mono, fontWeight = FontWeight.Bold)
            ),
            topLeft = Offset(30f, center.y - 20)
        )
        drawText(
            m.measure("${gearPct?.roundToInt() ?: "—"}%", androidx.compose.ui.text.TextStyle(color = gearColor, fontSize = 10.sp, fontFamily = Mono)),
            topLeft = Offset(30f, center.y)
        )
    }
}

/**
 * Widok STATUS - podsumowanie (placeholder)
 */
@Composable
private fun EicasStatusView(state: FlightState) {
    val m = rememberTextMeasurer()
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width
        val h = size.height
        val center = Offset(w / 2, h / 2)

        // TODO: Implement status view
        // - Lista aktywnych systemów
        // - Ostrzeżenia
        // - Cautions
        // - Advisories

        drawText(
            m.measure("STATUS", androidx.compose.ui.text.TextStyle(color = Color.White, fontSize = 14.sp, fontFamily = Mono)),
            topLeft = Offset(center.x - 30, 20f)
        )
        drawText(
            m.measure(loc("common.in_development"), androidx.compose.ui.text.TextStyle(color = Color.Gray, fontSize = 11.sp)),
            topLeft = Offset(center.x - 50, center.y)
        )
    }
}
