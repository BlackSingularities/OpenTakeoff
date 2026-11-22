package com.freemetar.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Route
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.data.LatLon
import com.freemetar.app.domain.AirportLayout
import com.freemetar.app.domain.AreaKind
import com.freemetar.app.domain.LineKind
import com.freemetar.app.domain.SpotKind
import com.freemetar.app.domain.TaxiInfo
import com.freemetar.app.domain.TaxiRoute
import com.freemetar.app.domain.loc
import com.freemetar.app.domain.remainingFrom
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.FmColors
import com.freemetar.app.ui.theme.Mono
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.PropertyValue
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.maps.Style
import org.maplibre.android.style.sources.GeoJsonOptions
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.LineString
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon
import kotlin.math.roundToInt

/*
 * Mapa lotniska na mapie telefonu. Od przybliżenia ok. 12,5 rysowane są płyty, pasy i drogi kołowania
 * z kafelków OpenFreeMap (warstwa „aeroway” z oznaczeniami dróg); gdy dla lotniska jest pobrana mapa
 * z OpenStreetMap (Overpass), zastępuje je pełniejszy rysunek: terminale, hangary, stanowiska, bramki
 * i miejsca oczekiwania. Do tego trasa kołowania i pierścienie odległości wokół samolotu.
 */

private const val OMT = "openmaptiles"
private const val SRC_LAY_AREA = "oab-lay-area"
private const val SRC_LAY_LINE = "oab-lay-line"
private const val SRC_LAY_SPOT = "oab-lay-spot"
private const val SRC_LAY_NAME = "oab-lay-name"
private const val SRC_TAXI_ROUTE = "oab-taxi-route"
private const val SRC_TAXI_MARK = "oab-taxi-mark"

/** Warstwy z kafelków (chowane, gdy jest pełniejsza mapa z Overpass). */
internal val OMT_AIRPORT_LAYERS = listOf("oab-omt-apron", "oab-omt-taxiway", "oab-omt-runway", "oab-omt-twy-label", "oab-omt-rwy-label")

/** Warstwy mapy lotniska z Overpass. */
internal val LAYOUT_LAYERS = listOf(
    "oab-lay-area", "oab-lay-taxilane", "oab-lay-taxiway", "oab-lay-runway", "oab-lay-twy-center", "oab-lay-rwy-center",
    "oab-lay-hold", "oab-lay-twy-label", "oab-lay-rwy-label", "oab-lay-spot", "oab-lay-spot-label", "oab-lay-name",
)

private fun hexc(c: Color) = String.format("#%06X", 0xFFFFFF and c.toArgb())

/** Barwy rysunku lotniska — takie same jak na Windows. */
internal class AptColors(night: Boolean) {
    val apron = if (night) "#262C34" else "#DCDFE4"
    val terminal = if (night) "#3A4352" else "#BFC6D1"
    val hangar = if (night) "#333A44" else "#C9CED6"
    val taxiway = if (night) "#4A525E" else "#A9AFB8"
    val runway = if (night) "#6B7480" else "#7A818B"
    val center = "#F2C230"
    val hold = "#E5484D"
    val stand = "#4FD1E8"
    val gate = "#FFB020"
    val labelText = if (night) "#FFD23F" else "#1A1A1A"
    val labelHalo = if (night) "#000000" else "#FFD23F"
    val nameText = if (night) "#C9D1DC" else "#3A4250"
    val nameHalo = if (night) "#0E1216" else "#FFFFFF"
}

private fun widthByZoom(z13: Float): Expression = Expression.interpolate(
    Expression.exponential(2f), Expression.zoom(),
    Expression.stop(12, z13 / 2), Expression.stop(13, z13), Expression.stop(15, z13 * 4), Expression.stop(17, z13 * 16), Expression.stop(19, z13 * 64),
)

private fun eqProp(k: String, v: String) = Expression.eq(Expression.get(k), v)

/** Dodaje źródła i warstwy mapy lotniska (pod trasą, śladem i samolotem). */
internal fun addAirportLayers(st: Style, c: FmColors) {
    val a = AptColors(c.isNight)
    // Kafelki OpenFreeMap: warstwa „aeroway” (class: runway, taxiway, apron…; ref: oznaczenie)
    if (st.getSource(OMT) != null) {
        st.addLayer(FillLayer("oab-omt-apron", OMT).withSourceLayer("aeroway")
            .withFilter(eqProp("class", "apron"))
            .withProperties(PropertyFactory.fillColor(a.apron), PropertyFactory.fillOpacity(0.9f)).apply { minZoom = 12f })
        st.addLayer(LineLayer("oab-omt-taxiway", OMT).withSourceLayer("aeroway")
            .withFilter(eqProp("class", "taxiway"))
            .withProperties(PropertyFactory.lineColor(a.taxiway), PropertyFactory.lineWidth(widthByZoom(1.9f)), PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"))
            .apply { minZoom = 12f })
        st.addLayer(LineLayer("oab-omt-runway", OMT).withSourceLayer("aeroway")
            .withFilter(eqProp("class", "runway"))
            .withProperties(PropertyFactory.lineColor(a.runway), PropertyFactory.lineWidth(widthByZoom(3.7f)))
            .apply { minZoom = 11f })
        st.addLayer(SymbolLayer("oab-omt-twy-label", OMT).withSourceLayer("aeroway")
            .withFilter(Expression.all(eqProp("class", "taxiway"), Expression.has("ref")))
            .withProperties(*twyLabelProps(a)).apply { minZoom = 14f })
        st.addLayer(SymbolLayer("oab-omt-rwy-label", OMT).withSourceLayer("aeroway")
            .withFilter(Expression.all(eqProp("class", "runway"), Expression.has("ref")))
            .withProperties(*rwyLabelProps()).apply { minZoom = 13f })
    }
    // Mapa z Overpass
    st.addSource(GeoJsonSource(SRC_LAY_AREA, GeoJsonOptions().withTolerance(0.2f)))
    st.addLayer(FillLayer("oab-lay-area", SRC_LAY_AREA).withProperties(
        PropertyFactory.fillColor(Expression.get("color")), PropertyFactory.fillOpacity(0.95f),
    ).apply { minZoom = 12f })
    st.addSource(GeoJsonSource(SRC_LAY_LINE, GeoJsonOptions().withTolerance(0.2f)))
    st.addLayer(LineLayer("oab-lay-taxilane", SRC_LAY_LINE).withFilter(eqProp("k", "taxilane")).withProperties(
        PropertyFactory.lineColor(a.taxiway), PropertyFactory.lineWidth(widthByZoom(1.1f)), PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"),
    ).apply { minZoom = 12f })
    st.addLayer(LineLayer("oab-lay-taxiway", SRC_LAY_LINE).withFilter(eqProp("k", "taxiway")).withProperties(
        PropertyFactory.lineColor(a.taxiway), PropertyFactory.lineWidth(widthByZoom(1.9f)), PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"),
    ).apply { minZoom = 12f })
    st.addLayer(LineLayer("oab-lay-runway", SRC_LAY_LINE).withFilter(eqProp("k", "runway")).withProperties(
        PropertyFactory.lineColor(a.runway), PropertyFactory.lineWidth(widthByZoom(3.7f)),
    ).apply { minZoom = 11f })
    st.addLayer(LineLayer("oab-lay-twy-center", SRC_LAY_LINE)
        .withFilter(Expression.any(eqProp("k", "taxiway"), eqProp("k", "taxilane"))).withProperties(
            PropertyFactory.lineColor(a.center), PropertyFactory.lineWidth(1.2f), PropertyFactory.lineOpacity(0.9f),
        ).apply { minZoom = 15f })
    st.addLayer(LineLayer("oab-lay-rwy-center", SRC_LAY_LINE).withFilter(eqProp("k", "runway")).withProperties(
        PropertyFactory.lineColor("#FFFFFF"), PropertyFactory.lineWidth(1.4f), PropertyFactory.lineDasharray(arrayOf(6f, 5f)), PropertyFactory.lineOpacity(0.85f),
    ).apply { minZoom = 14f })
    st.addLayer(LineLayer("oab-lay-hold", SRC_LAY_LINE).withFilter(eqProp("k", "hold")).withProperties(
        PropertyFactory.lineColor(a.hold), PropertyFactory.lineWidth(2.6f),
    ).apply { minZoom = 15f })
    st.addLayer(SymbolLayer("oab-lay-twy-label", SRC_LAY_LINE).withFilter(Expression.has("lbl")).withProperties(*twyLabelProps(a, "lbl")).apply { minZoom = 14f })
    st.addLayer(SymbolLayer("oab-lay-rwy-label", SRC_LAY_LINE).withFilter(Expression.has("rlbl")).withProperties(*rwyLabelProps("rlbl")).apply { minZoom = 13f })
    st.addSource(GeoJsonSource(SRC_LAY_SPOT))
    st.addLayer(CircleLayer("oab-lay-spot", SRC_LAY_SPOT).withProperties(
        PropertyFactory.circleRadius(Expression.interpolate(Expression.linear(), Expression.zoom(), Expression.stop(15, 2.2f), Expression.stop(18, 5f))),
        PropertyFactory.circleColor(Expression.get("color")),
        PropertyFactory.circleStrokeColor(c.paperHex()), PropertyFactory.circleStrokeWidth(1f),
    ).apply { minZoom = 15f })
    st.addLayer(SymbolLayer("oab-lay-spot-label", SRC_LAY_SPOT).withFilter(Expression.has("label")).withProperties(
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(10f),
        PropertyFactory.textOffset(arrayOf(0f, 1.0f)),
        PropertyFactory.textColor(Expression.get("color")),
        PropertyFactory.textHaloColor(c.paperHex()),
        PropertyFactory.textHaloWidth(1.4f),
        PropertyFactory.textOptional(true),
    ).apply { minZoom = 16f })
    st.addSource(GeoJsonSource(SRC_LAY_NAME))
    st.addLayer(SymbolLayer("oab-lay-name", SRC_LAY_NAME).withProperties(
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(11f),
        PropertyFactory.textColor(a.nameText),
        PropertyFactory.textHaloColor(a.nameHalo),
        PropertyFactory.textHaloWidth(1.4f),
        PropertyFactory.textOptional(true),
    ).apply { minZoom = 15f })
    // Trasa kołowania: magenta z obwódką, znaczniki przecięć pasów (czerwone) i celu
    st.addSource(GeoJsonSource(SRC_TAXI_ROUTE))
    st.addLayer(LineLayer("oab-taxi-casing", SRC_TAXI_ROUTE).withProperties(
        PropertyFactory.lineColor(c.paperHex()), PropertyFactory.lineWidth(8f), PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"), PropertyFactory.lineOpacity(0.8f),
    ))
    st.addLayer(LineLayer("oab-taxi-line", SRC_TAXI_ROUTE).withProperties(
        PropertyFactory.lineColor(hexc(c.magenta)), PropertyFactory.lineWidth(4.5f), PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"),
    ))
    st.addSource(GeoJsonSource(SRC_TAXI_MARK))
    st.addLayer(CircleLayer("oab-taxi-mark", SRC_TAXI_MARK).withProperties(
        PropertyFactory.circleRadius(6f), PropertyFactory.circleColor(Expression.get("color")),
        PropertyFactory.circleStrokeColor(c.paperHex()), PropertyFactory.circleStrokeWidth(2f),
    ))
    st.addLayer(SymbolLayer("oab-taxi-mark-label", SRC_TAXI_MARK).withProperties(
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(12f),
        PropertyFactory.textOffset(arrayOf(0f, -1.4f)),
        PropertyFactory.textColor(Expression.get("color")),
        PropertyFactory.textHaloColor(c.paperHex()),
        PropertyFactory.textHaloWidth(2f),
        PropertyFactory.textAllowOverlap(true),
        PropertyFactory.textIgnorePlacement(true),
    ))
}

private fun FmColors.paperHex() = hexc(paper)

/** Oznaczenie drogi kołowania wzdłuż linii: jak tabliczka — żółte na czarnym albo czarne na żółtym. */
private fun twyLabelProps(a: AptColors, field: String = "ref") = arrayOf<PropertyValue<*>>(
    PropertyFactory.symbolPlacement("line"),
    PropertyFactory.symbolSpacing(220f),
    PropertyFactory.textField(Expression.get(field)),
    PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
    PropertyFactory.textSize(Expression.interpolate(Expression.linear(), Expression.zoom(), Expression.stop(14, 11f), Expression.stop(17, 14f))),
    PropertyFactory.textColor(a.labelText),
    PropertyFactory.textHaloColor(a.labelHalo),
    PropertyFactory.textHaloWidth(2.2f),
    PropertyFactory.textMaxAngle(40f),
    PropertyFactory.textPitchAlignment("viewport"),
)

private fun rwyLabelProps(field: String = "ref") = arrayOf<PropertyValue<*>>(
    PropertyFactory.symbolPlacement("line"),
    PropertyFactory.symbolSpacing(400f),
    PropertyFactory.textField(Expression.get(field)),
    PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
    PropertyFactory.textSize(12f),
    PropertyFactory.textColor("#FFFFFF"),
    PropertyFactory.textHaloColor("#000000"),
    PropertyFactory.textHaloWidth(1.8f),
    PropertyFactory.textPitchAlignment("viewport"),
)

private fun pts(list: List<LatLon>) = list.map { Point.fromLngLat(it.lon, it.lat) }

/** Rysunek map lotnisk (wszystkich wczytanych) w źródłach GeoJSON. */
internal class LayoutData(val areas: FeatureCollection, val lines: FeatureCollection, val spots: FeatureCollection, val names: FeatureCollection)

internal fun layoutData(layouts: Collection<AirportLayout>, night: Boolean): LayoutData {
    val a = AptColors(night)
    val areas = ArrayList<Feature>(); val lines = ArrayList<Feature>(); val spots = ArrayList<Feature>(); val names = ArrayList<Feature>()
    // Kolejność: płyty pod budynkami
    val order = listOf(AreaKind.APRON, AreaKind.RUNWAY, AreaKind.HANGAR, AreaKind.TERMINAL)
    for (l in layouts) {
        for (k in order) for (ar in l.areas) {
            if (ar.kind != k || ar.ring.size < 4) continue
            areas += Feature.fromGeometry(Polygon.fromLngLats(listOf(pts(ar.ring)))).apply {
                addStringProperty("color", when (k) { AreaKind.APRON -> a.apron; AreaKind.RUNWAY -> a.runway; AreaKind.HANGAR -> a.hangar; AreaKind.TERMINAL -> a.terminal })
            }
            if (ar.name.isNotBlank() && (k == AreaKind.TERMINAL || k == AreaKind.HANGAR)) {
                val la = ar.ring.sumOf { it.lat } / ar.ring.size; val lo = ar.ring.sumOf { it.lon } / ar.ring.size
                names += Feature.fromGeometry(Point.fromLngLat(lo, la)).apply { addStringProperty("label", ar.name) }
            }
        }
        for (ln in l.lines) {
            if (ln.pts.size < 2) continue
            lines += Feature.fromGeometry(LineString.fromLngLats(pts(ln.pts))).apply {
                addStringProperty("k", when (ln.kind) { LineKind.RUNWAY -> "runway"; LineKind.TAXIWAY -> "taxiway"; LineKind.TAXILANE -> "taxilane"; LineKind.HOLD -> "hold" })
                if (ln.ref.isNotBlank()) when (ln.kind) {
                    LineKind.TAXIWAY, LineKind.TAXILANE -> addStringProperty("lbl", ln.ref)
                    LineKind.RUNWAY -> addStringProperty("rlbl", ln.ref)
                    LineKind.HOLD -> {}
                }
            }
        }
        for (s in l.spots) {
            spots += Feature.fromGeometry(Point.fromLngLat(s.lon, s.lat)).apply {
                addStringProperty("color", when (s.kind) { SpotKind.HOLD -> a.hold; SpotKind.STAND -> a.stand; SpotKind.GATE -> a.gate })
                val label = when (s.kind) {
                    SpotKind.HOLD -> s.ref.takeIf { it.isNotBlank() }
                    SpotKind.STAND -> s.ref.takeIf { it.isNotBlank() }
                    SpotKind.GATE -> s.ref.takeIf { it.isNotBlank() }?.let { "${loc("taxi.gate")} $it" }
                }
                if (label != null) addStringProperty("label", label)
                // Stanowisko lub bramka: dotknięcie na mapie ustawia je jako cel trasy kołowania
                if (s.kind != SpotKind.HOLD && s.ref.isNotBlank()) addStringProperty("dest", s.ref)
            }
        }
    }
    return LayoutData(
        FeatureCollection.fromFeatures(areas), FeatureCollection.fromFeatures(lines),
        FeatureCollection.fromFeatures(spots), FeatureCollection.fromFeatures(names),
    )
}

internal fun applyLayoutData(st: Style, d: LayoutData) {
    (st.getSource(SRC_LAY_AREA) as? GeoJsonSource)?.setGeoJson(d.areas)
    (st.getSource(SRC_LAY_LINE) as? GeoJsonSource)?.setGeoJson(d.lines)
    (st.getSource(SRC_LAY_SPOT) as? GeoJsonSource)?.setGeoJson(d.spots)
    (st.getSource(SRC_LAY_NAME) as? GeoJsonSource)?.setGeoJson(d.names)
}

/** Trasa kołowania na mapie (albo pusta). */
internal fun applyTaxiRoute(st: Style, r: TaxiRoute?, c: FmColors) {
    val line = if (r == null || r.pts.size < 2) FeatureCollection.fromFeatures(emptyList())
    else FeatureCollection.fromFeature(Feature.fromGeometry(LineString.fromLngLats(pts(r.pts))))
    val marks = ArrayList<Feature>()
    r?.crossings?.forEach { m ->
        marks += Feature.fromGeometry(Point.fromLngLat(m.lon, m.lat)).apply {
            addStringProperty("color", hexc(c.danger)); addStringProperty("label", "HOLD SHORT ${m.label}")
        }
    }
    r?.end?.let { m ->
        marks += Feature.fromGeometry(Point.fromLngLat(m.lon, m.lat)).apply {
            addStringProperty("color", if (m.runway) hexc(c.danger) else hexc(c.magenta))
            addStringProperty("label", if (m.runway) "RWY ${m.label}" else m.label)
        }
    }
    (st.getSource(SRC_TAXI_ROUTE) as? GeoJsonSource)?.setGeoJson(line)
    (st.getSource(SRC_TAXI_MARK) as? GeoJsonSource)?.setGeoJson(FeatureCollection.fromFeatures(marks))
}

internal fun setLayersVisible(st: Style, ids: List<String>, on: Boolean) {
    val v = PropertyFactory.visibility(if (on) "visible" else "none")
    ids.forEach { id -> st.getLayer(id)?.setProperties(v) }
}

/** Pierścienie 5, 10 i 20 NM wokół samolotu (linie i podpisy w źródłach róży). */
internal fun rangeRings(lat: Double, lon: Double, color: String, labelColor: String): Pair<List<Feature>, List<Feature>> {
    val lines = ArrayList<Feature>(); val labels = ArrayList<Feature>()
    for (nm in listOf(5, 10, 20)) {
        val r = nm * 1852.0
        val ring = (0..72).map { destPoint(lat, lon, it * 5.0, r) }
        lines += Feature.fromGeometry(LineString.fromLngLats(ring)).apply { addStringProperty("color", color); addNumberProperty("w", 1.3f) }
        labels += roseLabel(destPoint(lat, lon, 135.0, r), "$nm NM", 0.0, labelColor, 11f)
    }
    return lines to labels
}

// ---------- interfejs ----------

private fun fmtDist(m: Double): String = if (m >= 1000) "%.1f km".format(m / 1000) else "${(m / 10).roundToInt() * 10} m"

/** Pasek kołowania na górze mapy: na której drodze jest samolot, co dalej, ostrzeżenie przed pasem. */
@Composable
internal fun TaxiStrip(info: TaxiInfo?, route: TaxiRoute?, lat: Double?, lon: Double?, modifier: Modifier = Modifier) {
    val c = Fm.c
    val remaining = if (route != null && lat != null && lon != null) route.remainingFrom(lat, lon) else null
    val danger = info?.onRunway != null || (info?.runwayAheadM ?: Double.MAX_VALUE) < 150 || (info?.holdAheadM ?: Double.MAX_VALUE) < 40
    Column(
        modifier.padding(horizontal = 10.dp).widthIn(max = 520.dp).fillMaxWidth()
            .clip(RoundedCornerShape(12.dp)).background(c.card.copy(alpha = 0.95f))
            .border(if (danger) 2.dp else 1.dp, if (danger) c.danger else c.rule, RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        val main = when {
            info == null -> loc("taxi.taxiing")
            info.onRunway != null -> loc("taxi.on_runway", info.onRunway)
            info.on != null -> loc("taxi.on", info.on)
            else -> loc("taxi.off_taxiway")
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(main, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = if (info?.onRunway != null) c.danger else c.ink, maxLines = 1, modifier = Modifier.weight(1f))
            if (info?.next != null && info.nextM != null) Text(
                loc("taxi.next", info.next, fmtDist(info.nextM)), fontFamily = Mono, fontSize = 13.sp, color = c.accent, maxLines = 1,
            )
        }
        if (info?.runwayAhead != null && info.runwayAheadM != null) Text(
            loc("taxi.runway_ahead", info.runwayAhead, fmtDist(info.runwayAheadM)),
            fontWeight = FontWeight.Bold, fontSize = 13.sp, color = if (info.runwayAheadM < 150) c.danger else c.caution,
        )
        if (info?.holdAheadM != null) Text(loc("taxi.hold_ahead", fmtDist(info.holdAheadM)), fontSize = 13.sp, color = c.caution)
        if (remaining != null) Text(loc("taxi.to_go", fmtDist(remaining)), fontSize = 12.sp, color = c.ink2)
    }
}

/** Karta trasy kołowania: wpisane drogi i cel, przyciski pasów, wynik (drogi, długość, przecinane pasy). */
@Composable
internal fun TaxiCard(
    layout: AirportLayout?, text: String, route: TaxiRoute?, error: String?, loading: Boolean,
    onPlan: (String) -> Unit, onClear: () -> Unit, onClose: () -> Unit, modifier: Modifier = Modifier,
) {
    val c = Fm.c
    val focus = LocalFocusManager.current
    var input by remember { mutableStateOf(text) }
    LaunchedEffect(text) { if (text != input && text.isNotEmpty()) input = text }
    Column(
        modifier.navigationBarsPadding().padding(12.dp).widthIn(max = 560.dp).fillMaxWidth()
            .clip(RoundedCornerShape(12.dp)).background(c.card.copy(alpha = 0.97f)).border(1.dp, c.rule, RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Route, null, tint = c.magenta, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(
                listOfNotNull(loc("taxi.title"), layout?.ident).joinToString(" · "),
                fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = c.ink, maxLines = 1, modifier = Modifier.weight(1f),
            )
            Icon(Icons.Outlined.Close, loc("common.close"), tint = c.ink2, modifier = Modifier.size(22.dp).clickable(onClick = onClose))
        }
        if (layout == null) {
            Text(
                if (loading) loc("taxi.loading") else loc("taxi.zoom_to_airport"),
                style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 6.dp),
            )
            return@Column
        }
        OutlinedTextField(
            input, { input = it }, singleLine = true,
            placeholder = { Text(loc("taxi.hint"), fontSize = 13.sp) },
            textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = Mono),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Characters, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(onGo = { focus.clearFocus(); onPlan(input) }),
            modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
        )
        // Pasy jako cel: zamieniają (albo dopisują) ostatnie słowo trasy
        val ends = layout.runwayEndKeys
        Row(
            Modifier.padding(top = 6.dp).horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically,
        ) {
            ends.forEach { k ->
                Chip("RWY $k") {
                    val words = input.trim().split(Regex("\\s+")).filter { it.isNotBlank() }
                    val keep = if (words.isNotEmpty() && com.freemetar.app.domain.runwayKey(words.last()) != null) words.dropLast(1) else words
                    val t = (keep + k).joinToString(" ")
                    input = t; focus.clearFocus(); onPlan(t)
                }
            }
            Chip(loc("taxi.show")) { focus.clearFocus(); onPlan(input) }
            if (route != null || error != null) Chip(loc("taxi.clear")) { input = ""; onClear() }
        }
        if (error != null) Text(error, fontSize = 13.sp, color = c.danger, modifier = Modifier.padding(top = 6.dp))
        if (route != null) {
            val via = route.via.joinToString(" → ").ifBlank { "—" }
            Text(via, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = c.magenta, modifier = Modifier.padding(top = 6.dp))
            // Czas przy typowej prędkości kołowania 15 kt
            val min = (route.lengthM / (15 * 1852.0 / 60)).roundToInt().coerceAtLeast(1)
            Text(loc("taxi.length", fmtDist(route.lengthM), min), fontSize = 13.sp, color = c.ink2)
            if (route.crossings.isNotEmpty()) Text(
                loc("taxi.crossings", route.crossings.joinToString(", ") { it.label }),
                fontWeight = FontWeight.Bold, fontSize = 13.sp, color = c.danger,
            )
            route.end?.takeIf { it.runway }?.let { Text(loc("taxi.hold_short_end", it.label), fontSize = 13.sp, color = c.caution) }
        }
    }
}

@Composable
private fun Chip(text: String, onClick: () -> Unit) {
    val c = Fm.c
    Box(
        Modifier.clip(RoundedCornerShape(8.dp)).background(c.card).border(1.dp, c.rule, RoundedCornerShape(8.dp))
            .clickable(role = Role.Button, onClick = onClick).padding(horizontal = 10.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, fontFamily = Mono, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = c.ink, maxLines = 1)
    }
}
