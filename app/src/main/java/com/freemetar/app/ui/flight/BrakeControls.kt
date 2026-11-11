package com.freemetar.app.ui.flight

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import com.freemetar.app.ui.theme.tr
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlin.math.roundToInt

/**
 * Oś hamulca aerodynamicznego (spoilery): zostaje w ustawionej pozycji jak dźwignia.
 * Podczas przeciągania pokazuje pozycję palca, potem stan z symulatora. [labelColor] — opis na płycie panelu.
 */
@Composable
internal fun SpeedBrakeAxis(link: LinkClient, s: FlightState, modifier: Modifier = Modifier, label: String = tr("brake_controls.speed_brake"), labelColor: Color = Fm.c.ink2) {
    var drag by remember { mutableStateOf<Float?>(null) }
    val sim = ((s.controls.spoilersPct ?: 0.0) / 100).toFloat().coerceIn(0f, 1f)
    val v = drag ?: sim
    val desc = tr("brake_controls.axis", label)
    Column(modifier) {
        AxisRow(label, "${(v * 100).roundToInt()}%", Fm.c.ink, labelColor)
        Slider(
            v, { x -> drag = x; link.brakes(speedbrake = x.toDouble()) },
            onValueChangeFinished = { drag?.let { link.brakes(speedbrake = it.toDouble()) }; drag = null },
            modifier = Modifier.semantics { contentDescription = desc },
        )
    }
}

/**
 * Oś hamulców kół: jak pedały — działa, dopóki trzymasz, puszczona wraca do zera; tylko na ziemi.
 * Wysyłana co 50 ms, dopóki wciśnięta; puszczenie (albo zamknięcie panelu) = jedno 0. Nic nie idzie,
 * dopóki jej nie dotkniesz — nie nadpisujemy pedałów podłączonych do symulatora.
 */
@Composable
internal fun WheelBrakeAxis(link: LinkClient, s: FlightState, modifier: Modifier = Modifier, labelColor: Color = Fm.c.ink2) {
    val c = Fm.c
    val onGround = s.aircraft.onGround == true
    var brake by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(brake > 0f) {
        if (brake <= 0f) return@LaunchedEffect
        try {
            while (isActive) {
                link.brakes(brake = brake.toDouble())
                delay(50)
            }
        } finally {
            link.brakes(brake = 0.0)
        }
    }
    // W powietrzu hamulce kół nie działają — suwak wraca do zera
    LaunchedEffect(onGround) { if (!onGround) brake = 0f }
    val desc = tr("brake_controls.wheel_brakes_axis_springs_back_on")
    Column(modifier) {
        AxisRow(
            tr("brake_controls.wheel_brakes"),
            if (onGround) "${(brake * 100).roundToInt()}%" else tr("brake_controls.ground_only"),
            if (brake > 0f) c.caution else c.ink2, labelColor,
        )
        Slider(brake, { brake = it }, enabled = onGround, onValueChangeFinished = { brake = 0f }, modifier = Modifier.semantics { contentDescription = desc })
    }
}

@Composable
private fun AxisRow(label: String, value: String, color: Color, labelColor: Color) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = MaterialTheme.typography.labelMedium, color = labelColor, modifier = Modifier.weight(1f))
        Text(value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = color)
    }
}
