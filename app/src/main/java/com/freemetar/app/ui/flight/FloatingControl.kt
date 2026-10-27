package com.freemetar.app.ui.flight

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.material3.Text
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.link.FlightState
import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Gamepad
import androidx.compose.material.icons.outlined.ScreenRotation
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.freemetar.app.data.link.LinkClient
import com.freemetar.app.domain.ControlMath
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.Fm
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlin.math.hypot
import kotlin.math.roundToInt

/**
 * Pływający przycisk sterowania nad całą aplikacją. Przytrzymanie = sterujesz:
 * w trybie żyroskopu przechylasz telefon (zero to pozycja z chwili naciśnięcia),
 * w trybie drążka przesuwasz kciuk od miejsca naciśnięcia. Puszczenie od razu oddaje stery.
 * Uchwyt pod przyciskiem służy do przestawienia go w inne miejsce ekranu.
 * Wariant z trymem: przy żyroskopie palec na przycisku jest wolny, więc trymuje się przeciągnięciem —
 * w dół = nos w górę, w górę = nos w dół (im dalej, tym szybciej). Przy drążku palec steruje,
 * więc trym ma osobne przyciski obok.
 */
@Composable
fun FloatingControl(link: LinkClient, modeName: String, s: FlightState? = null, withTrim: Boolean = false, modifier: Modifier = Modifier) {
    val c = Fm.c
    val haptic = LocalHapticFeedback.current
    val density = LocalDensity.current
    val tiltMode = modeName != StickMode.TOUCH.name     // kontroler sprzętowy nie potrzebuje przycisku — wtedy żyroskop
    var active by remember { mutableStateOf(false) }
    var x by remember { mutableFloatStateOf(0f) }        // −1…1 lotki
    var y by remember { mutableFloatStateOf(0f) }        // −1…1 ster wysokości, + = nos w górę
    var roll0 by remember { mutableDoubleStateOf(0.0) }
    var pitch0 by remember { mutableDoubleStateOf(0.0) }
    val tilt = rememberTilt(enabled = tiltMode)
    // Trym przeciągnięciem (żyroskop): −1 nos w dół, 0 nic, +1 nos w górę; siła 0…1 = tempo powtarzania
    var trimDir by remember { mutableIntStateOf(0) }
    var trimForce by remember { mutableFloatStateOf(0f) }
    val dragTrim = withTrim && tiltMode
    LaunchedEffect(trimDir) {
        if (trimDir == 0) return@LaunchedEffect
        while (isActive) {
            link.command(if (trimDir > 0) "ELEV_TRIM_UP" else "ELEV_TRIM_DN")
            delay((220 - 160 * trimForce).toLong().coerceIn(50, 220))
        }
    }
    val max = link.limits.maxDeflection

    // Jedna pozycja przycisku w całej aplikacji: ułamek okna (nie obszaru danego widoku), wspólny stan dla wszystkich
    // zakładek i widoków — przycisk jest w tym samym miejscu ekranu niezależnie od tego, gdzie jest rysowany.
    // Domyślnie lewa strona (z prawej są dźwignie mocy w widoku lotu), dolna część ekranu
    val context = androidx.compose.ui.platform.LocalContext.current
    val store = remember { context.getSharedPreferences("floating_control_pos", android.content.Context.MODE_PRIVATE) }
    remember { FloatingPos.load(store) }
    var fx by FloatingPos.x
    var fy by FloatingPos.y
    LaunchedEffect(fx, fy) {
        delay(300)   // zapis po zakończeniu przeciągania, nie co klatkę
        store.edit().putFloat("global.x", fx).putFloat("global.y", fy).apply()
    }
    val rootView = androidx.compose.ui.platform.LocalView.current.rootView
    var origin by remember { mutableStateOf(androidx.compose.ui.geometry.Offset.Zero) }

    fun axes(): Pair<Double, Double> = if (tiltMode) {
        ControlMath.axis(tilt.roll - roll0, 35.0, 2.0, 1.0, max) to ControlMath.axis(tilt.pitch - pitch0, 30.0, 2.0, 1.0, max)
    } else {
        expo(x.toDouble(), 0.4) * max to expo(y.toDouble(), 0.4) * max
    }

    LaunchedEffect(active) {
        if (!active) { link.releaseAxes(); return@LaunchedEffect }
        while (isActive && active) {
            val (a, e) = axes()
            link.axis(a, e, 0.0)
            delay(50)
        }
    }
    DisposableEffect(Unit) { onDispose { link.releaseAxes() } }

    BoxWithConstraints(modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInWindow() }) {
        val btn = 68.dp
        val trimW = if (withTrim && !tiltMode) 52.dp else 0.dp
        val total = btn + 22.dp   // przycisk + uchwyt
        val pad = 12.dp
        val padPx = with(density) { pad.toPx() }
        val bwPx = with(density) { (btn + trimW).toPx() }
        val bhPx = with(density) { total.toPx() }
        val boxW = with(density) { maxWidth.toPx() }
        val boxH = with(density) { maxHeight.toPx() }
        // Zakres ruchu liczony w oknie; w tym widoku przycisk jest dociskany do jego krawędzi
        val winW = rootView.width.toFloat().takeIf { it > 0 } ?: boxW
        val winH = rootView.height.toFloat().takeIf { it > 0 } ?: boxH
        val wPx = (winW - bwPx - padPx * 2).coerceAtLeast(1f)
        val hPx = (winH - bhPx - padPx * 2).coerceAtLeast(1f)
        fun localX(f: Float) = (padPx + f * wPx - origin.x).coerceIn(padPx, (boxW - bwPx - padPx).coerceAtLeast(padPx))
        fun localY(f: Float) = (padPx + f * hPx - origin.y).coerceIn(padPx, (boxH - bhPx - padPx).coerceAtLeast(padPx))
        val reach = with(density) { 56.dp.toPx() }

        Row(
            Modifier.offset { IntOffset(localX(fx).roundToInt(), localY(fy).roundToInt()) },
            verticalAlignment = Alignment.Top,
        ) {
        if (withTrim && !tiltMode) TrimColumn(link, s, Modifier.padding(end = 6.dp))
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Box(
                Modifier.size(btn)
                    .semantics {
                        contentDescription = if (dragTrim) loc("Sterowanie żyroskopem: przytrzymaj i przechylaj telefon; przeciągnij w dół lub w górę, aby trymować", "Gyro control: hold and tilt the phone; drag down or up to trim")
                        else if (tiltMode) loc("Sterowanie żyroskopem: przytrzymaj i przechylaj telefon", "Gyro control: hold and tilt the phone")
                        else loc("Drążek: przytrzymaj i przesuwaj kciuk", "Stick: hold and move your thumb")
                    }
                    .pointerInput(tiltMode, dragTrim) {
                        val dead = 16.dp.toPx(); val full = 70.dp.toPx()
                        awaitEachGesture {
                            val down = awaitFirstDown()
                            down.consume()
                            roll0 = tilt.roll; pitch0 = tilt.pitch
                            x = 0f; y = 0f
                            active = true
                            haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                            while (true) {
                                val ev = awaitPointerEvent()
                                val ch = ev.changes.firstOrNull { it.id == down.id } ?: break
                                if (!ch.pressed) break
                                ch.consume()
                                if (dragTrim) {
                                    // Palec w dół (dy > 0) = nos w górę
                                    val dy = ch.position.y - down.position.y
                                    val dir = if (dy > dead) 1 else if (dy < -dead) -1 else 0
                                    trimForce = ((kotlin.math.abs(dy) - dead) / (full - dead)).coerceIn(0f, 1f)
                                    if (dir != trimDir) { trimDir = dir; if (dir != 0) haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove) }
                                }
                                if (!tiltMode) {
                                    var dx = (ch.position.x - down.position.x) / reach
                                    var dy = (ch.position.y - down.position.y) / reach
                                    val len = hypot(dx, dy)
                                    if (len > 1f) { dx /= len; dy /= len }
                                    x = dx; y = dy   // kciuk w dół = drążek do siebie = nos w górę
                                }
                            }
                            active = false
                            trimDir = 0
                            x = 0f; y = 0f
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                val (ail, elv) = if (active) axes() else 0.0 to 0.0
                Canvas(Modifier.matchParentSize()) {
                    val r = size.minDimension / 2f
                    // Przy aktywnym drążku pokaż zasięg kciuka poza przyciskiem
                    if (active && !tiltMode) drawCircle(Color(0x33FFFFFF), reach, center, style = Stroke(2f))
                    drawCircle(Color.Black, r, center)
                    drawCircle(if (active) c.accent else Color(0x66FFFFFF), r - 1.5f, center, style = Stroke(if (active) 4f else 2f))
                    if (active) {
                        val k = if (tiltMode) r * 0.8f else reach
                        drawCircle(c.accent, 9.dp.toPx(), center + Offset((ail / max * k).toFloat().coerceIn(-k, k), (elv / max * k).toFloat().coerceIn(-k, k)))
                    }
                }
                if (!active) Icon(if (tiltMode) Icons.Outlined.ScreenRotation else Icons.Outlined.Gamepad, null, tint = c.ink, modifier = Modifier.size(28.dp))
                // Trym przeciągnięciem: strzałki nad i pod przyciskiem, aktywna podświetlona
                if (dragTrim) {
                    Text("▲", fontSize = 10.sp, color = if (trimDir < 0) c.accent else c.ink2, modifier = Modifier.align(Alignment.TopCenter).padding(top = 2.dp))
                    Text("▼", fontSize = 10.sp, color = if (trimDir > 0) c.accent else c.ink2, modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 2.dp))
                }
            }
            // Odczyt trymu pod przyciskiem (wariant z trymem przy żyroskopie)
            if (dragTrim) Text(s?.controls?.elevatorTrimPct?.let { "TRIM %+.0f".format(it) } ?: "TRIM", fontSize = 10.sp, fontWeight = FontWeight.Bold,
                color = if (trimDir != 0) c.accent else c.ink, modifier = Modifier.padding(top = 3.dp).clip(RoundedCornerShape(50)).background(Color.Black).padding(horizontal = 6.dp, vertical = 1.dp))
            Spacer(Modifier.height(6.dp))
            // Uchwyt do przestawiania
            Box(
                Modifier.width(36.dp).height(16.dp).clip(RoundedCornerShape(50)).background(c.terrain).border(1.dp, c.rule, RoundedCornerShape(50))
                    .semantics { contentDescription = loc("Uchwyt: przeciągnij, aby przesunąć przycisk", "Handle: drag to move the button") }
                    .pointerInput(wPx, hPx, origin, boxW, boxH) {
                        detectDragGestures { ch, d ->
                            ch.consume()
                            // Zapisywana jest pozycja widoczna (po dociśnięciu do krawędzi widoku), w ułamku okna
                            fx = ((localX(fx) + d.x + origin.x - padPx) / wPx).coerceIn(0f, 1f)
                            fy = ((localY(fy) + d.y + origin.y - padPx) / hPx).coerceIn(0f, 1f)
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                Box(Modifier.width(16.dp).height(3.dp).clip(CircleShape).background(c.ink2))
            }
        }
        }
    }
}

/** Trym wysokości obok pływającego przycisku: dwa klawisze z powtarzaniem i bieżący trym w %. */
@Composable
private fun TrimColumn(link: LinkClient, s: FlightState?, modifier: Modifier = Modifier) {
    val c = Fm.c
    Column(
        modifier.width(46.dp).clip(RoundedCornerShape(23.dp)).background(Color.Black).border(1.5.dp, Color(0x66FFFFFF), RoundedCornerShape(23.dp)).padding(vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TrimKey("▲", loc("Trym: nos w dół", "Trim nose down")) { link.command("ELEV_TRIM_DN") }
        Text(s?.controls?.elevatorTrimPct?.let { "%+.0f".format(it) } ?: "TRIM", fontSize = 10.sp, fontWeight = FontWeight.Bold, color = c.ink,
            modifier = Modifier.padding(vertical = 2.dp))
        TrimKey("▼", loc("Trym: nos w górę", "Trim nose up")) { link.command("ELEV_TRIM_UP") }
    }
}

/** Klawisz trymu: działa raz po dotknięciu, potem powtarza co 80 ms, dopóki trzymasz. */
@Composable
private fun TrimKey(label: String, desc: String, action: () -> Unit) {
    val c = Fm.c
    val haptic = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val act by rememberUpdatedState(action)
    var pressed by remember { mutableStateOf(false) }
    Box(
        Modifier.size(38.dp).clip(CircleShape).background(if (pressed) c.accent else Color(0xFF1C1C1C))
            .semantics { contentDescription = desc; role = Role.Button; onClick { act(); true } }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown().consume()
                    pressed = true
                    haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                    act()
                    val job = scope.launch { delay(350); while (true) { act(); delay(80) } }
                    waitForUpOrCancellation()
                    job.cancel()
                    pressed = false
                }
            },
        contentAlignment = Alignment.Center,
    ) { Text(label, fontSize = 15.sp, color = if (pressed) Color.Black else c.ink) }
}

/** Wspólna pozycja pływającego przycisku (ułamek okna); odczyt z ustawień raz na proces. */
private object FloatingPos {
    private var loaded = false
    val x = androidx.compose.runtime.mutableFloatStateOf(0f)
    val y = androidx.compose.runtime.mutableFloatStateOf(0.72f)

    fun load(store: android.content.SharedPreferences) {
        if (loaded) return
        loaded = true
        // Starsze wersje pamiętały pozycję osobno dla każdego widoku — punktem wyjścia jest ta z widoku lotu
        x.floatValue = store.getFloat("global.x", store.getFloat("flight.x", 0f))
        y.floatValue = store.getFloat("global.y", store.getFloat("flight.y", 0.72f))
    }
}
