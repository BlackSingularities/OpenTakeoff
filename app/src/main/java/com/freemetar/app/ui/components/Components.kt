package com.freemetar.app.ui.components

import com.freemetar.app.domain.loc

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AcUnit
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.Dehaze
import androidx.compose.material.icons.outlined.Thunderstorm
import androidx.compose.material.icons.outlined.Umbrella
import androidx.compose.material.icons.outlined.WbCloudy
import androidx.compose.material.icons.outlined.WbSunny
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.Metar
import com.freemetar.app.data.Runway
import com.freemetar.app.domain.Category
import com.freemetar.app.domain.MetarDecoder
import com.freemetar.app.domain.Wind
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.cos
import kotlin.math.sin

// ---------- układ ----------

/** Szerokość okna → liczba kolumn; telefon 1, składany/tablet 2–3. */
@Composable
fun windowColumns(): Int {
    val w = LocalConfiguration.current.screenWidthDp
    return when { w >= 1000 -> 3; w >= 600 -> 2; else -> 1 }
}

@Composable
fun isWide(): Boolean = LocalConfiguration.current.screenWidthDp >= 600

@Composable
fun Rule(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(1.dp).background(Fm.c.rule))
}

/**
 * Sekcja briefingu jako panel przyrządu: etykieta panelu u góry, dane pod spodem.
 * Ramka w kolorze `frame` wyróżnia panel ostrzeżeń (bursztyn/czerwień).
 */
@Composable
fun Section(
    title: String,
    icon: ImageVector,
    modifier: Modifier = Modifier,
    frame: Color? = null,
    trailing: @Composable RowScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = Fm.c
    val shape = RoundedCornerShape(10.dp)
    Column(
        modifier
            .fillMaxWidth()
            .padding(horizontal = 14.dp, vertical = 5.dp)
            .clip(shape)
            .background(c.card)
            .border(if (frame != null) 1.5.dp else 1.dp, frame ?: c.rule, shape)
            .padding(horizontal = 14.dp, vertical = 12.dp)
            .animateContentSize(),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.height(IntrinsicSize.Min)) {
            Icon(icon, null, tint = frame ?: c.ink2, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(8.dp))
            Text(title, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, color = frame ?: c.ink2, modifier = Modifier.weight(1f))
            trailing()
        }
        Spacer(Modifier.size(10.dp))
        content()
    }
}

/** Powierzchnia „kartonika” — pasek lotu, pole danych. */
@Composable
fun Panel(modifier: Modifier = Modifier, onClick: (() -> Unit)? = null, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(10.dp)
    val c = Fm.c
    Column(
        modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.card)
            .border(1.dp, c.rule, shape)
            .let { if (onClick != null) it.clickable(onClick = onClick) else it },
        content = content,
    )
}

// ---------- dane ----------

data class Fact(val label: String, val value: String, val accent: Color? = null, val sub: String? = null)

/**
 * Siatka par etykieta–wartość rozdzielona cienkimi liniami, jak pola na pasku
 * kontrolera. Wartości krojem kokpitowym.
 */
@Composable
fun FactGrid(facts: List<Fact>, columns: Int = 2, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth()) {
        Rule()
        facts.chunked(columns).forEach { row ->
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
                row.forEachIndexed { i, f ->
                    if (i > 0) Box(Modifier.width(1.dp).fillMaxHeight().background(Fm.c.rule))
                    Column(Modifier.weight(1f).padding(start = if (i > 0) 14.dp else 0.dp, end = 10.dp, top = 10.dp, bottom = 10.dp)) {
                        Text(f.label, style = MaterialTheme.typography.labelSmall, color = Fm.c.ink2, maxLines = 1)
                        Text(f.value, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 18.sp, color = f.accent ?: Fm.c.ink)
                        if (f.sub != null) Text(f.sub, style = MaterialTheme.typography.bodySmall, color = Fm.c.ink2)
                    }
                }
                repeat(columns - row.size) {
                    Box(Modifier.width(1.dp).fillMaxHeight().background(Fm.c.rule))
                    Spacer(Modifier.weight(1f))
                }
            }
            Rule()
        }
    }
}

/** Ikona najbardziej znaczącego zjawiska pogodowego. */
fun weatherIcon(m: Metar?): ImageVector {
    if (m == null) return Icons.Outlined.Cloud
    val wx = m.wx.orEmpty()
    return when {
        "TS" in wx || m.clouds.any { it.type == "CB" } -> Icons.Outlined.Thunderstorm
        "SN" in wx || "FZ" in wx || "PL" in wx || "GR" in wx -> Icons.Outlined.AcUnit
        "RA" in wx || "DZ" in wx -> Icons.Outlined.Umbrella
        "FG" in wx || "BR" in wx || "HZ" in wx -> Icons.Outlined.Dehaze
        m.clouds.any { it.cover in setOf("BKN", "OVC", "VV") } -> Icons.Outlined.Cloud
        m.clouds.any { it.cover in setOf("FEW", "SCT") } -> Icons.Outlined.WbCloudy
        else -> Icons.Outlined.WbSunny
    }
}

/** Kategoria lotu z ikoną pogody na końcu wiersza — jak oznaczenie stanu na wyświetlaczu. */
@Composable
fun CategoryBlock(metar: Metar?, modifier: Modifier = Modifier, width: Dp = 64.dp) {
    val cat = metar?.flightCategory
    val col = Fm.c.category(cat)
    Column(
        modifier.width(width).fillMaxHeight().padding(vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(weatherIcon(metar), null, tint = col, modifier = Modifier.size(20.dp))
        Spacer(Modifier.size(5.dp))
        CategoryTag(cat)
    }
}

/** Kategoria w obwódce w jej kolorze (konwencja wyświetlaczy: kolor = znaczenie). */
@Composable
fun CategoryTag(cat: String?, modifier: Modifier = Modifier) {
    val col = Fm.c.category(cat)
    Text(
        cat ?: "—",
        color = col,
        fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 11.sp,
        modifier = modifier.clip(RoundedCornerShape(3.dp)).border(1.5.dp, col, RoundedCornerShape(3.dp)).padding(horizontal = 5.dp, vertical = 1.dp),
    )
}

@Composable
fun Pill(text: String, modifier: Modifier = Modifier, icon: ImageVector? = null, mono: Boolean = false, onClick: () -> Unit) {
    Row(
        modifier
            .clip(RoundedCornerShape(50))
            .background(Fm.c.terrain)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, null, tint = Fm.c.ink, modifier = Modifier.size(15.dp))
            Spacer(Modifier.width(6.dp))
        }
        Text(text, color = Fm.c.ink, fontFamily = if (mono) Mono else null, style = MaterialTheme.typography.labelMedium)
    }
}

/** Pusty stan: co się stało i co zrobić. */
@Composable
fun EmptyState(icon: ImageVector, title: String, sub: String? = null, action: (@Composable () -> Unit)? = null) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 28.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(56.dp).clip(CircleShape).background(Fm.c.terrain), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = Fm.c.ink, modifier = Modifier.size(26.dp))
        }
        Spacer(Modifier.size(12.dp))
        Text(title, style = MaterialTheme.typography.titleMedium, color = Fm.c.ink, textAlign = TextAlign.Center)
        if (sub != null) Text(sub, style = MaterialTheme.typography.bodySmall, color = Fm.c.ink2, textAlign = TextAlign.Center, modifier = Modifier.padding(top = 4.dp))
        if (action != null) { Spacer(Modifier.size(14.dp)); action() }
    }
}

// ---------- interaktywny METAR ----------

/**
 * METAR dużym krojem kokpitowym; każda grupa jest dotykalna, a jej znaczenie
 * pojawia się pod spodem. Element, który odróżnia OpenTakeoff od zwykłej listy.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun InteractiveMetar(raw: String, modifier: Modifier = Modifier, fontSize: Int = 19) {
    val groups = remember(raw) { MetarDecoder.decode(raw) }
    // Domyślnie zaznaczony wiatr — najczęściej pierwsza rzecz, o którą pyta pilot
    var sel by remember(raw) { mutableIntStateOf(groups.indexOfFirst { it.category == Category.WIND }.coerceAtLeast(0)) }
    val c = Fm.c
    Column(modifier) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            groups.forEachIndexed { i, g ->
                val danger = g.token.contains("CB") || g.token.contains("TS") || g.token.contains("FZ") || g.token.startsWith("+")
                val color = when {
                    danger -> c.ifr
                    g.category == Category.WIND -> c.blue
                    g.category == Category.REMARK -> c.ink2
                    else -> c.ink
                }
                val selected = i == sel
                Text(
                    g.token,
                    fontFamily = Mono, fontSize = fontSize.sp, color = color,
                    fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                    modifier = Modifier
                        .clip(RoundedCornerShape(3.dp))
                        .background(if (selected) c.ink.copy(alpha = 0.16f) else Color.Transparent)
                        .clickable(role = Role.Button) { sel = i }
                        .padding(horizontal = 2.dp, vertical = 1.dp),
                )
            }
        }
        Spacer(Modifier.size(10.dp))
        val g = groups.getOrNull(sel)
        AnimatedContent(g, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "metar-group") { cur ->
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(c.terrain).padding(horizontal = 14.dp, vertical = 12.dp)) {
                if (cur != null) {
                    Text(cur.token, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = c.ink)
                    Spacer(Modifier.size(2.dp))
                    Text(cur.text.takeIf { it != "—" && it.isNotBlank() } ?: loc("components.non_standard_group_no_explanation"), style = MaterialTheme.typography.bodyMedium, color = c.ink)
                }
            }
        }
    }
}

// ---------- schemat pasów ----------

/**
 * Schemat lotniska: pasy w skali, róża kierunków i strzałka wiatru.
 * Pasy rysowane względem środka lotniska, więc układ odpowiada rzeczywistości.
 */
@Composable
fun RunwayWindDiagram(runways: List<Runway>, windDir: Int?, windKt: Int?, variable: Boolean, modifier: Modifier = Modifier) {
    val measurer = rememberTextMeasurer()
    val c = Fm.c
    Canvas(modifier.fillMaxWidth().aspectRatio(1f)) {
        val center = Offset(size.width / 2, size.height / 2)
        val r = size.minDimension / 2 * 0.94f

        drawCircle(c.rule, r, center, style = Stroke(2f))
        for (d in 0 until 360 step 10) {
            val len = if (d % 30 == 0) 12f else 5f
            val a = Math.toRadians(d.toDouble() - 90)
            drawLine(c.ink2, Offset(center.x + (r - len) * cos(a).toFloat(), center.y + (r - len) * sin(a).toFloat()),
                Offset(center.x + r * cos(a).toFloat(), center.y + r * sin(a).toFloat()), 1.5f)
        }
        listOf("N" to 0, "E" to 90, "S" to 180, "W" to 270).forEach { (s, d) ->
            val a = Math.toRadians(d.toDouble() - 90)
            val layout = measurer.measure(s, TextStyle(color = if (s == "N") c.ink else c.ink2, fontSize = 12.sp, fontWeight = FontWeight.Bold, fontFamily = Mono))
            drawText(layout, topLeft = Offset(center.x + (r - 26) * cos(a).toFloat() - layout.size.width / 2, center.y + (r - 26) * sin(a).toFloat() - layout.size.height / 2))
        }

        val coords = runways.flatMap { rw -> listOfNotNull(rw.le.lat?.let { la -> rw.le.lon?.let { lo -> la to lo } }, rw.he.lat?.let { la -> rw.he.lon?.let { lo -> la to lo } }) }
        val cLat = coords.map { it.first }.average().takeIf { !it.isNaN() }
        val cLon = coords.map { it.second }.average().takeIf { !it.isNaN() }
        val maxLen = (runways.maxOfOrNull { it.lengthFt ?: 0 } ?: 0).coerceAtLeast(1)
        val span = if (cLat != null && cLon != null) coords.maxOf { kotlin.math.hypot(it.first - cLat, (it.second - cLon) * cos(Math.toRadians(cLat))) } else 0.0
        val scale = if (span > 0) (r * 0.55f) / span.toFloat() else 0f

        runways.forEach { rw ->
            val hdg = Wind.headingOf(rw.le.ident, rw.le.headingTrue) ?: return@forEach
            val half = (rw.lengthFt ?: maxLen).toFloat() / maxLen * r * 0.55f
            val mid = if (scale > 0 && rw.le.lat != null && rw.he.lat != null && rw.le.lon != null && rw.he.lon != null && cLat != null && cLon != null) {
                val mLat = (rw.le.lat + rw.he.lat) / 2; val mLon = (rw.le.lon + rw.he.lon) / 2
                Offset(center.x + ((mLon - cLon) * cos(Math.toRadians(cLat)) * scale).toFloat(), center.y - ((mLat - cLat) * scale).toFloat())
            } else center
            rotate(hdg.toFloat(), mid) {
                val w = 14f
                drawRect(c.ink, Offset(mid.x - w / 2, mid.y - half), Size(w, half * 2))
                drawLine(c.paper, Offset(mid.x, mid.y - half + 8), Offset(mid.x, mid.y + half - 8), 1.5f,
                    pathEffect = PathEffect.dashPathEffect(floatArrayOf(9f, 9f)))
            }
            val a = Math.toRadians(hdg - 90)
            listOf(rw.le.ident to (half + 15), rw.he.ident to -(half + 15)).forEach { (id, dist) ->
                if (id.isBlank()) return@forEach
                val layout = measurer.measure(id, TextStyle(color = c.ink, fontSize = 11.sp, fontFamily = Mono, fontWeight = FontWeight.Bold))
                drawText(layout, topLeft = Offset(mid.x - (dist * cos(a)).toFloat() - layout.size.width / 2, mid.y - (dist * sin(a)).toFloat() - layout.size.height / 2))
            }
        }

        // Strzałka wskazuje, dokąd wieje wiatr; zaczyna się od strony, z której wieje
        if (windDir != null && (windKt ?: 0) > 0 && !variable) {
            val a = Math.toRadians(windDir.toDouble() - 90)
            val from = Offset(center.x + r * 0.97f * cos(a).toFloat(), center.y + r * 0.97f * sin(a).toFloat())
            val to = Offset(center.x + r * 0.4f * cos(a).toFloat(), center.y + r * 0.4f * sin(a).toFloat())
            drawLine(c.blue, from, to, 7f, cap = StrokeCap.Round)
            val ang = Math.atan2((to.y - from.y).toDouble(), (to.x - from.x).toDouble())
            val head = Path().apply {
                moveTo(to.x + 10 * cos(ang).toFloat(), to.y + 10 * sin(ang).toFloat())
                lineTo(to.x - 22 * cos(ang - 0.45).toFloat(), to.y - 22 * sin(ang - 0.45).toFloat())
                lineTo(to.x - 22 * cos(ang + 0.45).toFloat(), to.y - 22 * sin(ang + 0.45).toFloat())
                close()
            }
            drawPath(head, c.blue)
        }
    }
}

// ---------- wykres ----------

/** Linia trendu z 12 h (wartości w kolejności czasu). */
@Composable
fun TrendChart(label: String, values: List<Double>, unit: String, color: Color, modifier: Modifier = Modifier) {
    val measurer = rememberTextMeasurer()
    val c = Fm.c
    Column(modifier) {
        Row(verticalAlignment = Alignment.Bottom) {
            Text(label, style = MaterialTheme.typography.labelMedium, color = c.ink2, modifier = Modifier.weight(1f))
            values.lastOrNull()?.let { Text("${it.toInt()}$unit", fontFamily = Mono, fontWeight = FontWeight.Bold, color = c.ink, fontSize = 15.sp) }
        }
        Spacer(Modifier.size(6.dp))
        if (values.size < 2) {
            Text(loc("components.not_enough_observations"), style = MaterialTheme.typography.bodySmall, color = c.ink2)
            return@Column
        }
        Canvas(Modifier.fillMaxWidth().height(64.dp)) {
            val min = values.min(); val max = values.max()
            val spanV = (max - min).takeIf { it > 0.0 } ?: 1.0
            val pad = 12f
            fun pt(i: Int, v: Double) = Offset(i * size.width / (values.size - 1), pad + (size.height - 2 * pad) * (1 - ((v - min) / spanV)).toFloat())
            val line = Path(); val fill = Path()
            values.forEachIndexed { i, v ->
                val p = pt(i, v)
                if (i == 0) { line.moveTo(p.x, p.y); fill.moveTo(p.x, size.height); fill.lineTo(p.x, p.y) } else { line.lineTo(p.x, p.y); fill.lineTo(p.x, p.y) }
            }
            fill.lineTo(size.width, size.height); fill.close()
            drawLine(c.rule, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f)
            drawPath(fill, Brush.verticalGradient(listOf(color.copy(alpha = 0.22f), Color.Transparent)))
            drawPath(line, color, style = Stroke(3f, cap = StrokeCap.Round))
            drawCircle(color, 5f, pt(values.size - 1, values.last()))
            val st = TextStyle(color = c.ink2, fontSize = 10.sp, fontFamily = Mono)
            drawText(measurer.measure(max.toInt().toString(), st), topLeft = Offset(0f, -4f))
            drawText(measurer.measure(min.toInt().toString(), st), topLeft = Offset(0f, size.height - 16f))
        }
    }
}
