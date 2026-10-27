package com.freemetar.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.TrackChanges
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.components.Fact
import com.freemetar.app.ui.components.FactGrid
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * Karta radiali na dole mapy: położenie względem punktu (radial, kurs do punktu, odległość),
 * wybór radialu IN/OUT i prowadzenie — odchylenie oraz kurs do lotu z kątem przechwycenia i poprawką na wiatr.
 */
@Composable
internal fun RadialCard(
    ref: RefPoint?, fix: RadialFix?, guide: RadialGuide?, selRadial: Int?, inbound: Boolean, magnetic: Boolean, hasAircraft: Boolean,
    gsKt: Double?,
    onInbound: (Boolean) -> Unit, onRadial: (Int?) -> Unit, onNewPoint: () -> Unit, onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Fm.c
    // Bez deklinacji z symulatora kursy są geograficzne — oznaczone „T”
    val u = if (magnetic) "°" else "°T"
    Column(
        modifier.navigationBarsPadding().padding(12.dp).widthIn(max = 560.dp).fillMaxWidth()
            .clip(RoundedCornerShape(12.dp)).background(c.card.copy(alpha = 0.96f)).border(1.dp, c.rule, RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.TrackChanges, null, tint = c.magenta, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(
                ref?.let { it.name ?: "%.3f %.3f".format(java.util.Locale.US, it.lat, it.lon) } ?: loc("Radiale", "Radials"),
                fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 17.sp, color = c.ink, maxLines = 1, modifier = Modifier.weight(1f),
            )
            if (ref != null) SmallKey(loc("Nowy punkt", "New point"), false, onNewPoint)
            Spacer(Modifier.width(6.dp))
            Icon(Icons.Outlined.Close, loc("Zamknij", "Close"), tint = c.ink2, modifier = Modifier.size(22.dp).clickable(onClick = onClose))
        }
        if (ref == null) {
            Text(
                loc("Dotknij mapy, lotniska albo pomocy nawigacyjnej, aby wybrać punkt odniesienia. Przytrzymanie mapy wybiera punkt w każdej chwili.",
                    "Tap the map, an airport or a navaid to pick the reference point. A long press picks a point at any time."),
                style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 6.dp),
            )
            return@Column
        }
        if (fix == null) {
            Text(loc("Brak pozycji samolotu — przesuń mapę, pomiar idzie od środka (+).", "No aircraft position — pan the map, measured from the centre (+)."),
                style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 6.dp))
        } else {
            val ete = gsKt?.takeIf { hasAircraft && it > 30 }?.let { fix.distNm / it * 60 }
            FactGrid(
                listOf(
                    Fact(loc("Na radialu", "On radial"), "R-${deg3(fix.radialMag)}", accent = c.magenta, sub = if (magnetic) "QDR" else loc("od punktu", "from point")),
                    Fact(loc("Do punktu", "To point"), "${deg3(fix.toMag)}$u", sub = if (magnetic) "QDM" else loc("kurs", "course")),
                    Fact(loc("Odległość", "Distance"), "%.1f NM".format(fix.distNm),
                        sub = ete?.let { "%d:%02d".format(it.toInt() / 60, it.roundToInt() % 60) } ?: if (!hasAircraft) loc("od środka mapy", "from map centre") else null),
                ),
                columns = 3, modifier = Modifier.padding(top = 6.dp),
            )
        }
        // Wybór radialu: kierunek, krok ±1/±10, „bieżący” = radial, na którym samolot jest teraz
        Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            SmallKey("IN", inbound) { onInbound(true) }
            SmallKey("OUT", !inbound) { onInbound(false) }
            Spacer(Modifier.weight(1f))
            fun step(d: Int) = onRadial(radialOf((selRadial ?: fix?.let { radialOf(it.radialMag) } ?: 360) + d.toDouble()))
            SmallKey("−10", false) { step(-10) }
            SmallKey("−1", false) { step(-1) }
            Text(selRadial?.let { "R-%03d".format(it) } ?: "R-–––", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 15.sp,
                color = if (selRadial != null) c.magenta else c.ink2)
            SmallKey("+1", false) { step(1) }
            SmallKey("+10", false) { step(10) }
        }
        if (fix != null) Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            SmallKey(loc("Bieżący radial", "Current radial"), false) { onRadial(radialOf(fix.radialMag)) }
            if (selRadial != null) SmallKey(loc("Wyczyść", "Clear"), false) { onRadial(null) }
        }
        if (guide != null) {
            val side = if (guide.xtkNm > 0) loc("P", "R") else loc("L", "L")
            val turn = if (hasAircraft) loc("HDG", "HDG") else "TRK"
            FactGrid(
                listOf(
                    Fact(loc("Kurs na radialu", "Radial course"), "${deg3(guide.course)}$u", sub = if (inbound) loc("do punktu (IN)", "inbound") else loc("od punktu (OUT)", "outbound")),
                    Fact(
                        loc("Odchylenie", "Off course"),
                        when { guide.behind -> loc("za punktem", "past point"); guide.established -> loc("na radialu", "on radial"); else -> "%.1f NM %s".format(abs(guide.xtkNm), side) },
                        accent = when { guide.established -> c.green; guide.behind || abs(guide.xtkNm) > 5 -> c.caution; else -> null },
                        sub = if (!guide.behind && !guide.established) loc("radial po ${if (guide.xtkNm > 0) "lewej" else "prawej"}", "radial to the ${if (guide.xtkNm > 0) "left" else "right"}") else null,
                    ),
                    Fact(
                        loc("Leć", "Fly"), "$turn ${deg3(guide.heading)}$u", accent = c.magenta,
                        sub = when {
                            guide.behind -> loc("najpierw do punktu", "to the point first")
                            else -> listOfNotNull(
                                guide.interceptDeg.takeIf { it >= 1 }?.let { loc("przechwycenie ${it.roundToInt()}°", "intercept ${it.roundToInt()}°") },
                                guide.wcaDeg.takeIf { abs(it) >= 1 }?.let { loc("wiatr %+d°", "wind %+d°").format(it.roundToInt()) },
                            ).joinToString(" · ").ifEmpty { null }
                        },
                    ),
                ),
                columns = 3, modifier = Modifier.padding(top = 8.dp),
            )
        }
        Text(
            loc("Dotknij mapy, aby wskazać radial · przytrzymaj, aby zmienić punkt", "Tap the map to set the radial · long-press to move the point"),
            style = MaterialTheme.typography.labelSmall, color = c.ink2, modifier = Modifier.padding(top = 6.dp),
        )
    }
}

/** Mały przycisk karty: wybrany — tło akcentu, inaczej obwódka. */
@Composable
private fun SmallKey(text: String, on: Boolean, onClick: () -> Unit) {
    val c = Fm.c
    Box(
        Modifier.clip(RoundedCornerShape(8.dp)).background(if (on) c.accent.copy(alpha = 0.22f) else c.card)
            .border(1.dp, if (on) c.accent else c.rule, RoundedCornerShape(8.dp))
            .clickable(role = Role.Button, onClick = onClick).padding(horizontal = 9.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, fontFamily = Mono, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = if (on) c.accent else c.ink, maxLines = 1)
    }
}
