package com.freemetar.app.ui.flight

import com.freemetar.app.domain.loc

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.view.Surface
import android.view.WindowManager
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.domain.ControlMath
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

/** Bieżące pochylenie telefonu w stopniach (roll + = w prawo, pitch + = odchylony do siebie). */
class TiltState {
    var roll by mutableDoubleStateOf(0.0)
    var pitch by mutableDoubleStateOf(0.0)
}

/**
 * Czujnik orientacji (bez magnetometru — stabilniejszy przy elektronice w pobliżu), aktywny tylko,
 * gdy [enabled]. Układ współrzędnych zgodny z tym, jak użytkownik trzyma telefon.
 */
@Composable
fun rememberTilt(enabled: Boolean = true): TiltState {
    val ctx = LocalContext.current
    val tilt = remember { TiltState() }
    DisposableEffect(enabled) {
        if (!enabled) return@DisposableEffect onDispose { }
        val sm = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager
        val sensor = sm.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR) ?: sm.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)
        val r = FloatArray(9); val out = FloatArray(9); val o = FloatArray(3)
        @Suppress("DEPRECATION")
        val display = (ctx.getSystemService(Context.WINDOW_SERVICE) as WindowManager).defaultDisplay
        val listener = object : SensorEventListener {
            override fun onSensorChanged(e: SensorEvent) {
                SensorManager.getRotationMatrixFromVector(r, e.values)
                when (display.rotation) {
                    Surface.ROTATION_90 -> SensorManager.remapCoordinateSystem(r, SensorManager.AXIS_Y, SensorManager.AXIS_MINUS_X, out)
                    Surface.ROTATION_270 -> SensorManager.remapCoordinateSystem(r, SensorManager.AXIS_MINUS_Y, SensorManager.AXIS_X, out)
                    Surface.ROTATION_180 -> SensorManager.remapCoordinateSystem(r, SensorManager.AXIS_MINUS_X, SensorManager.AXIS_MINUS_Y, out)
                    else -> r.copyInto(out)
                }
                SensorManager.getOrientation(out, o)
                tilt.pitch = -Math.toDegrees(o[1].toDouble())
                tilt.roll = Math.toDegrees(o[2].toDouble())
            }
            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}
        }
        if (sensor != null) sm.registerListener(listener, sensor, SensorManager.SENSOR_DELAY_GAME)
        onDispose { sm.unregisterListener(listener) }
    }
    return tilt
}

/**
 * Wolant z przechylania telefonu. Steruje tylko podczas przytrzymania przycisku;
 * pozycja zerowa jest zapamiętywana w chwili naciśnięcia, a puszczenie natychmiast oddaje stery.
 */
@Composable
fun YokePanel(link: LinkClient, s: FlightState) {
    val c = Fm.c
    val haptic = LocalHapticFeedback.current
    var holding by remember { mutableStateOf(false) }
    val tilt = rememberTilt()
    var roll0 by remember { mutableDoubleStateOf(0.0) }
    var pitch0 by remember { mutableDoubleStateOf(0.0) }
    var sensitivity by rememberSaveable { mutableFloatStateOf(1f) }
    var invert by rememberSaveable { mutableStateOf(false) }
    var rudder by remember { mutableFloatStateOf(0f) }
    val max = link.limits.maxDeflection

    DisposableEffect(Unit) { onDispose { link.releaseAxes() } }

    val ail = if (holding) ControlMath.axis(tilt.roll - roll0, 35.0, 2.0, sensitivity.toDouble(), max) else 0.0
    val elv0 = if (holding) ControlMath.axis(tilt.pitch - pitch0, 30.0, 2.0, sensitivity.toDouble(), max) else 0.0
    val elv = if (invert) -elv0 else elv0

    // Wysyłanie 20 Hz podczas przytrzymania; po puszczeniu — natychmiastowe zwolnienie osi
    LaunchedEffect(holding) {
        if (!holding) { link.releaseAxes(); return@LaunchedEffect }
        while (isActive && holding) {
            link.axis(
                ControlMath.axis(tilt.roll - roll0, 35.0, 2.0, sensitivity.toDouble(), max),
                ControlMath.axis(tilt.pitch - pitch0, 30.0, 2.0, sensitivity.toDouble(), max).let { if (invert) -it else it },
                rudder.toDouble().coerceIn(-max, max),
            )
            delay(50)
        }
    }

    Text(
        loc("yoke.hold_the_phone_like_a_yoke") +
            loc("yoke.neutral_is_wherever_the_phone_is"),
        style = MaterialTheme.typography.bodySmall, color = c.ink2,
    )
    Spacer(Modifier.size(10.dp))
    Box(
        Modifier.fillMaxWidth().aspectRatio(1.4f).clip(RoundedCornerShape(16.dp))
            .background(if (holding) c.accent.copy(alpha = 0.12f) else c.terrain)
            .semantics { contentDescription = loc("yoke.tilt_control_pad_press_and_hold") }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown()
                    roll0 = tilt.roll; pitch0 = tilt.pitch
                    holding = true
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    waitForUpOrCancellation()
                    holding = false
                    rudder = 0f
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.fillMaxWidth().aspectRatio(1.4f)) {
            val ctr = Offset(size.width / 2, size.height / 2)
            val rad = size.minDimension * 0.4f
            drawCircle(c.rule, rad, ctr, style = Stroke(2f))
            drawCircle(c.rule, rad * (max.toFloat()), ctr, style = Stroke(1f))
            drawLine(c.rule, Offset(ctr.x - rad, ctr.y), Offset(ctr.x + rad, ctr.y), 1f)
            drawLine(c.rule, Offset(ctr.x, ctr.y - rad), Offset(ctr.x, ctr.y + rad), 1f)
            // Kropka: wychylenie (lotki w poziomie, ster wysokości w pionie: drążek do siebie = w dół ekranu)
            drawCircle(if (holding) c.accent else c.ink2, 16f, Offset(ctr.x + (ail * rad).toFloat(), ctr.y + (elv * rad).toFloat()))
        }
        Text(
            if (holding) loc("yoke.in_control") else loc("yoke.press_and_hold_to_fly"),
            fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp,
            color = if (holding) c.accent else c.ink2,
            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 10.dp),
        )
        if (s.axes.active && !holding) Text(loc("yoke.server_still_holds_the_axes"), fontSize = 11.sp, color = c.caution, modifier = Modifier.align(Alignment.TopCenter).padding(top = 8.dp))
    }
    Spacer(Modifier.size(8.dp))
    Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
        Text(loc("yoke.aileron_2f").format(ail), fontFamily = Mono, fontSize = 13.sp, color = c.ink)
        Text(loc("yoke.elevator_2f").format(elv), fontFamily = Mono, fontSize = 13.sp, color = c.ink)
        Text(loc("yoke.rudder_2f").format(rudder), fontFamily = Mono, fontSize = 13.sp, color = c.ink)
    }
    Spacer(Modifier.size(8.dp))
    Text(loc("yoke.rudder_only_while_flying"), style = MaterialTheme.typography.labelMedium, color = c.ink2)
    Slider(rudder, { rudder = it }, valueRange = -1f..1f, enabled = holding, onValueChangeFinished = { rudder = 0f })
    Column {
        Text(loc("yoke.sensitivity_1f").format(sensitivity), style = MaterialTheme.typography.labelMedium, color = c.ink2)
        Slider(sensitivity, { sensitivity = it }, valueRange = 0.4f..1.6f, steps = 11)
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.height(48.dp)) {
            Text(loc("yoke.invert_elevator"), style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.weight(1f))
            Switch(invert, { invert = it })
        }
    }
}
