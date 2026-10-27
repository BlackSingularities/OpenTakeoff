package com.freemetar.app.ui.screens

import androidx.compose.foundation.border
import kotlin.math.roundToInt

import com.freemetar.app.domain.loc

import android.graphics.PointF
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.MyLocation
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Remove
import androidx.compose.material.icons.outlined.Explore
import androidx.compose.material.icons.outlined.TrackChanges
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.MainViewModel
import com.freemetar.app.data.Metar
import com.freemetar.app.data.Sigmet
import com.freemetar.app.domain.Geo
import com.freemetar.app.ui.flight.avionics.gpsLeg
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.geojson.LineString
import com.freemetar.app.ui.components.CategoryTag
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.FmColors
import com.freemetar.app.ui.theme.Mono
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.MapLibreMap
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.FillExtrusionLayer
import org.maplibre.android.style.layers.FillLayer
import org.maplibre.android.style.layers.LineLayer
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonOptions
import org.maplibre.android.style.sources.GeoJsonSource
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon

private const val SRC_METAR = "oab-metars"
private const val SRC_SIGMET = "oab-sigmets"
private const val LYR_METAR = "oab-metar-dots"
private const val SRC_ROUTE = "oab-route"
private const val SRC_ROUTE_PTS = "oab-route-points"
private const val SRC_PLANE = "oab-plane"
private const val SRC_PLANE3D = "oab-plane-3d"
private const val LYR_PLANE3D = "oab-plane-3d-layer"
private const val SRC_TRACK = "oab-track"
private const val IMG_PLANE = "oab-plane-icon"
private const val SRC_LINE3D = "oab-track-line3d"
private const val SRC_WALL3D = "oab-track-wall3d"
private const val LYR_WALL3D = "oab-track-wall3d-layer"
private const val SRC_TRACK_LBL = "oab-track-labels"
private const val LYR_LINE3D = "oab-track-line3d-layer"
private const val LYR_BUILDINGS = "oab-buildings-3d"
private const val LYR_TRACK = "oab-track-line"
private const val SRC_NAV = "oab-msfs-nav"
private const val SRC_ROSE = "oab-rose"
private const val SRC_ROSE_LBL = "oab-rose-labels"
private const val SRC_ROSE_TAG = "oab-rose-tags"
private const val SRC_RADIAL_DASH = "oab-radial-dash"
private const val SRC_REF = "oab-radial-ref"
private val METAR_LAYERS = listOf(LYR_METAR, "oab-metar-label")
private val ROUTE_LAYERS = listOf("oab-route-line", "oab-route-pts", "oab-route-label")
private val NAV_LAYERS = listOf("oab-nav-pts", "oab-nav-label")

/** Kolor wysokości: nisko zieleń, średnio cyjan, wyżej błękit, poziom przelotowy magenta. */
private fun altColor(): Expression = Expression.interpolate(
    Expression.linear(), Expression.get("alt"),
    *ALT_STOPS.map { (ft, c) -> Expression.stop(ft, Expression.rgb(c[0], c[1], c[2])) }.toTypedArray(),
)

/** Symbol samolotu (żółty jak na wyświetlaczach) jako bitmapa dla warstwy mapy. */
private fun planeBitmap(): android.graphics.Bitmap {
    val size = 64
    val bmp = android.graphics.Bitmap.createBitmap(size, size, android.graphics.Bitmap.Config.ARGB_8888)
    val cv = android.graphics.Canvas(bmp)
    val p = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFFFC21A.toInt() }
    val stroke = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
        color = 0xFF0E1216.toInt(); style = android.graphics.Paint.Style.STROKE; strokeWidth = 3f
    }
    val path = android.graphics.Path().apply {
        moveTo(32f, 4f); lineTo(36f, 24f); lineTo(60f, 36f); lineTo(60f, 42f); lineTo(36f, 36f); lineTo(35f, 50f)
        lineTo(44f, 56f); lineTo(44f, 60f); lineTo(32f, 57f); lineTo(20f, 60f); lineTo(20f, 56f); lineTo(29f, 50f)
        lineTo(28f, 36f); lineTo(4f, 42f); lineTo(4f, 36f); lineTo(28f, 24f); close()
    }
    cv.drawPath(path, p); cv.drawPath(path, stroke)
    return bmp
}

private fun hex(c: Color) = String.format("#%06X", 0xFFFFFF and c.toArgb())

/** GeoJSON stacji METAR z kolorem kategorii zapisanym w cechach (warstwa koloruje wg „color”). */
private fun metarFeatures(list: List<Metar>, c: FmColors): FeatureCollection =
    FeatureCollection.fromFeatures(list.mapNotNull { m ->
        val lat = m.lat ?: return@mapNotNull null
        val lon = m.lon ?: return@mapNotNull null
        Feature.fromGeometry(Point.fromLngLat(lon, lat)).apply {
            addStringProperty("id", m.station)
            addStringProperty("color", hex(c.category(m.flightCategory)))
        }
    })

private fun sigmetFeatures(list: List<Sigmet>): FeatureCollection =
    FeatureCollection.fromFeatures(list.filter { it.area.size >= 3 }.map { s ->
        val ring = s.area.map { Point.fromLngLat(it.lon, it.lat) }.let { if (it.first() != it.last()) it + it.first() else it }
        Feature.fromGeometry(Polygon.fromLngLats(listOf(ring))).apply { addStringProperty("id", s.id) }
    })

/** Bieżący odcinek planu lotu z GPS symulatora (poprzedni → następny punkt) jako linia. */
private fun legLine(leg: List<com.freemetar.app.data.RoutePoint>): FeatureCollection =
    if (leg.size < 2) FeatureCollection.fromFeatures(emptyList())
    else FeatureCollection.fromFeature(Feature.fromGeometry(LineString.fromLngLats(leg.map { Point.fromLngLat(it.lon!!, it.lat!!) })))

private fun legPoints(leg: List<com.freemetar.app.data.RoutePoint>): FeatureCollection =
    FeatureCollection.fromFeatures(leg.filter { it.ident.isNotBlank() }.map { p ->
        Feature.fromGeometry(Point.fromLngLat(p.lon!!, p.lat!!)).apply { addStringProperty("id", p.ident) }
    })

/** Mapa pogody: lotniska w kolorach kategorii lotu i obszary SIGMET; dotknięcie lotniska otwiera briefing. */
@Composable
fun MapScreen(vm: MainViewModel, onOpen: (String) -> Unit) {
    val ctx = LocalContext.current
    val c = Fm.c
    val metars by vm.mapMetars.collectAsStateWithLifecycle()
    val sigmets by vm.mapSigmets.collectAsStateWithLifecycle()
    val settings by vm.settings.collectAsStateWithLifecycle()
    val live by vm.flightState.collectAsStateWithLifecycle()
    val liveTrack by vm.track.collectAsStateWithLifecycle()
    val logs by vm.flightLogs.collectAsStateWithLifecycle()
    val mapFlightId by vm.mapFlight.collectAsStateWithLifecycle()
    // Zapisany lot wybrany w historii ma pierwszeństwo przed torem bieżącego lotu
    val shownFlight = logs.firstOrNull { it.id == mapFlightId }
    val track = shownFlight?.track ?: liveTrack
    var follow by rememberSaveable { mutableStateOf(true) }
    var showSigmets by rememberSaveable { mutableStateOf(true) }
    var showMetars by rememberSaveable { mutableStateOf(true) }
    var showRoute by rememberSaveable { mutableStateOf(true) }
    var showNav by rememberSaveable { mutableStateOf(true) }
    var layersMenu by remember { mutableStateOf(false) }
    // Róża kursów wokół samolotu i narzędzie radiali od wskazanego punktu
    var showRose by rememberSaveable { mutableStateOf(false) }
    var radialMode by rememberSaveable { mutableStateOf(false) }
    var ref by remember { mutableStateOf<RefPoint?>(null) }
    var selRadial by rememberSaveable { mutableStateOf<Int?>(null) }
    var inbound by rememberSaveable { mutableStateOf(true) }
    // Metry na 100 dp ekranu (w poziomie, w środku mapy) — róże mają stałą wielkość na ekranie
    var roseScale by remember { mutableStateOf(0.0) }
    var center by remember { mutableStateOf<LatLng?>(null) }
    val nearbyNav by vm.nearbyNav.collectAsStateWithLifecycle()
    // Kompas MapLibre w prawym dolnym rogu, nad paskiem nawigacji systemu
    val density = androidx.compose.ui.platform.LocalDensity.current
    val compassBottom = WindowInsets.navigationBars.getBottom(density) + with(density) { 18.dp.roundToPx() }
    val compassRight = with(density) { 14.dp.roundToPx() }
    // Mapa zawsze w 3D - domyślne pochylenie 50°
    val defaultTilt = 50.0
    val lastFollowAt = remember { longArrayOf(0L) }
    var picked by remember { mutableStateOf<Metar?>(null) }
    var style by remember { mutableStateOf<Style?>(null) }
    var wired by remember { mutableStateOf(false) }
    val openLatest by rememberUpdatedState(onOpen)
    val metarsLatest by rememberUpdatedState(metars)
    // Deklinacja z symulatora (E +); bez niej radiale i róże są w stopniach geograficznych
    val magVar = live?.let { s -> s.avionics?.magVarDeg ?: s.attitude.hdgTrueDeg?.let { t -> s.attitude.hdgMagDeg?.let { m -> norm180(t - m) } } }
    val mv = magVar ?: 0.0
    val mvLatest by rememberUpdatedState(mv)
    val planeLat = live?.position?.lat; val planeLon = live?.position?.lon
    // Bez samolotu radial liczony od środka mapy (krzyżyk)
    val needCenter by rememberUpdatedState(radialMode && (planeLat == null || planeLon == null))

    val mapView = remember {
        MapLibre.getInstance(ctx)
        MapView(ctx).apply { onCreate(null) }
    }

    // Cykl życia MapView musi iść za cyklem ekranu
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val obs = LifecycleEventObserver { _, e ->
            when (e) {
                Lifecycle.Event.ON_START -> mapView.onStart()
                Lifecycle.Event.ON_RESUME -> mapView.onResume()
                Lifecycle.Event.ON_PAUSE -> mapView.onPause()
                Lifecycle.Event.ON_STOP -> mapView.onStop()
                else -> {}
            }
        }
        lifecycle.addObserver(obs)
        onDispose {
            lifecycle.removeObserver(obs)
            mapView.onPause(); mapView.onStop(); mapView.onDestroy()
        }
    }

    val styleUrl = if (c.isNight) "https://tiles.openfreemap.org/styles/dark" else "https://tiles.openfreemap.org/styles/positron"
    LaunchedEffect(styleUrl) {
        mapView.getMapAsync { map ->
            if (map.cameraPosition.zoom < 2.0) {
                val home = vm.mapHome()
                map.cameraPosition = CameraPosition.Builder()
                    .target(LatLng(home?.lat ?: 52.0, home?.lon ?: 19.0))
                    .zoom(if (home != null) 6.5 else 5.0)
                    .tilt(defaultTilt)
                    .build()
            }
            // Tilt i rotate dwoma palcami
            map.uiSettings.isRotateGesturesEnabled = true
            map.uiSettings.isTiltGesturesEnabled = true
            map.uiSettings.compassGravity = android.view.Gravity.BOTTOM or android.view.Gravity.END
            map.uiSettings.setCompassMargins(0, 0, compassRight, compassBottom)
            map.uiSettings.setCompassFadeFacingNorth(true)
            map.setStyle(Style.Builder().fromUri(styleUrl)) { st ->
                addOverlay(st, c)
                set3dLayers(st, true) // Zawsze 3D
                style = st
            }
            if (wired) return@getMapAsync
            wired = true
            map.addOnCameraIdleListener { requestArea(map, vm) }
            fun measure() {
                val w = mapView.width; val h = mapView.height
                if (w == 0 || h == 0) return
                val px = with(density) { 100.dp.toPx() }
                val a = map.projection.fromScreenLocation(PointF(w / 2f, h / 2f))
                val b = map.projection.fromScreenLocation(PointF(w / 2f + px, h / 2f))
                val m = Geo.distanceNm(a.latitude, a.longitude, b.latitude, b.longitude) * 1852
                if (roseScale <= 0 || kotlin.math.abs(m - roseScale) / roseScale > 0.04) roseScale = m
                if (needCenter) center = a
            }
            map.addOnCameraMoveListener { measure() }
            map.addOnCameraIdleListener { measure() }
            mapView.post { measure() }
            // Punkt odniesienia: lotnisko, pomoc lub punkt trasy pod palcem (najbliższy), inaczej dokładnie to miejsce
            fun snap(ll: LatLng): RefPoint {
                val pt = map.projection.toScreenLocation(ll)
                val box = android.graphics.RectF(pt.x - 28, pt.y - 28, pt.x + 28, pt.y + 28)
                val f = map.queryRenderedFeatures(box, "oab-nav-pts", LYR_METAR, "oab-route-pts").minByOrNull {
                    val g = it.geometry() as? Point ?: return@minByOrNull Double.MAX_VALUE
                    val q = map.projection.toScreenLocation(LatLng(g.latitude(), g.longitude()))
                    kotlin.math.hypot((q.x - pt.x).toDouble(), (q.y - pt.y).toDouble())
                }
                val g = f?.geometry() as? Point
                return if (f != null && g != null) RefPoint(g.latitude(), g.longitude(), f.getStringProperty("ident") ?: f.getStringProperty("id"))
                else RefPoint(ll.latitude, ll.longitude, null)
            }
            map.addOnMapLongClickListener { ll ->
                radialMode = true; picked = null; selRadial = null
                ref = snap(ll)
                true
            }
            map.addOnMapClickListener { ll ->
                if (radialMode) {
                    val r = ref
                    // Bez punktu dotknięcie go wybiera; z punktem — ustawia radial, na którym leży dotknięte miejsce
                    if (r == null) ref = snap(ll)
                    else selRadial = radialOf(Geo.bearing(r.lat, r.lon, ll.latitude, ll.longitude) - mvLatest)
                    return@addOnMapClickListener true
                }
                val pt: PointF = map.projection.toScreenLocation(ll)
                val hit = map.queryRenderedFeatures(pt, LYR_METAR).firstOrNull()
                    ?: map.queryRenderedFeatures(android.graphics.RectF(pt.x - 24, pt.y - 24, pt.x + 24, pt.y + 24), LYR_METAR).firstOrNull()
                val id = hit?.getStringProperty("id")
                picked = id?.let { i -> metarsLatest.firstOrNull { it.station == i } }
                id != null
            }
            requestArea(map, vm)
        }
    }

    LaunchedEffect(style, metars, c) {
        (style?.getSource(SRC_METAR) as? GeoJsonSource)?.setGeoJson(metarFeatures(metars, c))
    }
    // Samolot z symulatora i przebyty tor
    LaunchedEffect(style, live?.position, live?.attitude) {
        val st = style ?: return@LaunchedEffect
        val lat = live?.position?.lat; val lon = live?.position?.lon
        val fc = if (lat == null || lon == null) FeatureCollection.fromFeatures(emptyList())
        else FeatureCollection.fromFeature(Feature.fromGeometry(Point.fromLngLat(lon, lat)).apply {
            addNumberProperty("hdg", live?.attitude?.hdgTrueDeg ?: 0.0)
            live?.position?.altMslFt?.let { addStringProperty("label", altLabel(it.roundToInt())) }
        })
        (st.getSource(SRC_PLANE) as? GeoJsonSource)?.setGeoJson(fc)
        // Samolot w 3D na wysokości lotu; z bliska w skali mapy, z daleka mały znak stałej wielkości
        val alt = live?.position?.altMslFt
        mapView.getMapAsync { m ->
            val plane3d = if (lat == null || lon == null || alt == null) FeatureCollection.fromFeatures(emptyList()) else {
                val zoom = m.cameraPosition.zoom
                val mPerPx = 156_543.03 * kotlin.math.cos(Math.toRadians(lat)) / Math.pow(2.0, zoom)
                planeShape3d(lat, lon, live?.attitude?.hdgTrueDeg ?: 0.0, alt, planeSpanM(zoom, mPerPx))
            }
            (st.getSource(SRC_PLANE3D) as? GeoJsonSource)?.setGeoJson(plane3d)
        }
        // Śledzenie: kamera najwyżej ~3 razy na sekundę, z płynnym przejściem — przesuwanie przy każdym stanie
        // (10–20 Hz) razem z perspektywą 3D przeciążało renderer, a i tak nie było widać różnicy
        val nowMs = System.currentTimeMillis()
        if (follow && lat != null && lon != null && nowMs - lastFollowAt[0] >= 330) {
            lastFollowAt[0] = nowMs
            mapView.getMapAsync { m ->
                val b = CameraPosition.Builder()
                    .target(LatLng(lat, lon))
                    .tilt(defaultTilt)
                // Kamera patrzy w kierunku lotu
                (live?.attitude?.trackTrueDeg ?: live?.attitude?.hdgTrueDeg)?.let { b.bearing(it) }
                m.easeCamera(CameraUpdateFactory.newCameraPosition(b.build()), 320)
            }
        }
    }
    // Zapisany lot: kadr na cały tor z pochyleniem
    val pendingFit = remember { arrayOfNulls<LatLngBounds>(1) }
    fun fitTrackCamera() = mapView.getMapAsync { m ->
        val fit = pendingFit[0]?.let { m.getCameraForLatLngBounds(it, intArrayOf(80, 240, 80, 160)) }
        pendingFit[0] = null
        val b = CameraPosition.Builder(fit ?: m.cameraPosition)
            .tilt(defaultTilt)
            .zoom((fit?.zoom ?: m.cameraPosition.zoom) - 0.7)
        m.animateCamera(CameraUpdateFactory.newCameraPosition(b.build()), 900)
    }
    // Warstwy 3D zawsze włączone
    LaunchedEffect(style) {
        val st = style ?: return@LaunchedEffect
        set3dLayers(st, true)
    }
    // Zapisany lot: kadr na cały tor
    LaunchedEffect(style, shownFlight?.id) {
        val f = shownFlight ?: return@LaunchedEffect
        if (style == null || f.track.size < 2) return@LaunchedEffect
        follow = false
        pendingFit[0] = LatLngBounds.Builder().apply { f.track.forEach { include(LatLng(it.lat, it.lon)) } }.build()
        fitTrackCamera()
    }
    LaunchedEffect(style, track.size, shownFlight?.id) {
        val st = style ?: return@LaunchedEffect
        (st.getSource(SRC_TRACK) as? GeoJsonSource)?.setGeoJson(trackLine(track))
        // Kolor wysokości wzdłuż linii (line-progress = ułamek długości)
        val stops = trackGradient(track)
        if (stops.size >= 2) (st.getLayer(LYR_TRACK) as? LineLayer)?.setProperties(PropertyFactory.lineGradient(
            Expression.interpolate(Expression.linear(), Expression.lineProgress(),
                *stops.map { (f, alt) -> val c = altRgb(alt); Expression.stop(f, Expression.rgb(c[0], c[1], c[2])) }.toTypedArray()),
        ))
        // Bryły toru 3D liczone poza wątkiem UI — przy długim locie to kilka tysięcy czworokątów
        val (line3d, wall3d) = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { trackLine3d(track) to trackWall3d(track) }
        (st.getSource(SRC_LINE3D) as? GeoJsonSource)?.setGeoJson(line3d)
        (st.getSource(SRC_WALL3D) as? GeoJsonSource)?.setGeoJson(wall3d)
        (st.getSource(SRC_TRACK_LBL) as? GeoJsonSource)?.setGeoJson(trackLabels(track))
    }
    // Plan lotu z symulatora: bieżący odcinek GPS
    val leg = live?.gpsLeg().orEmpty()
    LaunchedEffect(style, leg) {
        (style?.getSource(SRC_ROUTE) as? GeoJsonSource)?.setGeoJson(legLine(leg))
        (style?.getSource(SRC_ROUTE_PTS) as? GeoJsonSource)?.setGeoJson(legPoints(leg))
    }
    // Widoczność warstw z menu
    LaunchedEffect(style, showMetars, showRoute, showNav) {
        val st = style ?: return@LaunchedEffect
        fun vis(ids: List<String>, on: Boolean) = ids.forEach { st.getLayer(it)?.setProperties(PropertyFactory.visibility(if (on) "visible" else "none")) }
        vis(METAR_LAYERS, showMetars); vis(ROUTE_LAYERS, showRoute); vis(NAV_LAYERS, showNav)
    }
    // Lotniska i pomoce nawigacyjne z bazy MSFS wokół samolotu
    LaunchedEffect(style, nearbyNav) {
        (style?.getSource(SRC_NAV) as? GeoJsonSource)?.setGeoJson(FeatureCollection.fromFeatures(nearbyNav.map { f ->
            Feature.fromGeometry(Point.fromLngLat(f.lon, f.lat)).apply {
                addStringProperty("type", f.type)
                addStringProperty("ident", f.ident)
                // Kolor gotowy w danych (jak przy METAR) — wyrażenie „match” wywracało renderer na części GPU
                addStringProperty("color", when (f.type) { "airport" -> "#4FD1E8"; "vor" -> "#3FE07A"; "ndb" -> "#FFB020"; else -> "#9AA4AE" })
                addStringProperty("id", if (f.type == "vor" && f.freqMhz != null) "${f.ident} ${"%.2f".format(java.util.Locale.US, f.freqMhz)}" else f.ident)
            }
        }))
    }
    LaunchedEffect(style, sigmets, showSigmets) {
        (style?.getSource(SRC_SIGMET) as? GeoJsonSource)?.setGeoJson(sigmetFeatures(if (showSigmets) sigmets else emptyList()))
    }

    // Róża kursów na samolocie, róża punktu odniesienia i wybrany radial
    val ink = hex(c.ink); val ink2 = hex(c.ink2); val navHex = hex(c.magenta); val bugHex = hex(c.accent); val trkHex = hex(c.green)
    val origin = if (planeLat != null && planeLon != null) LatLng(planeLat, planeLon) else if (radialMode) center else null
    val fix = ref?.let { r -> origin?.let { radialFix(r, it.latitude, it.longitude, mv) } }
    val guide = fix?.let { f ->
        selRadial?.let { sel ->
            radialGuide(sel, inbound, f, live?.environment?.windDirDeg?.let { norm360(it - mv) }, live?.environment?.windSpdKt,
                live?.speed?.tasKt?.takeIf { planeLat != null })
        }
    }
    LaunchedEffect(style, roseScale, showRose, radialMode, ref, selRadial, inbound, origin, live?.attitude, live?.autopilot?.hdgDeg, mv, c) {
        val st = style ?: return@LaunchedEffect
        val lines = ArrayList<Feature>(); val labels = ArrayList<Feature>(); val tags = ArrayList<Feature>(); val dash = ArrayList<Feature>()
        val scale = roseScale
        val r = ref?.takeIf { radialMode }
        if (scale > 0 && showRose && planeLat != null && planeLon != null) {
            val rM = scale * 1.5
            val rose = compassRose(planeLat, planeLon, rM, mv, ink)
            lines += rose.lines; labels += rose.labels
            live?.attitude?.hdgTrueDeg?.let { h ->
                // Kreska kursu od samolotu za obwód róży i odczyt kursu nad nią
                lines += segment(destPoint(planeLat, planeLon, h, rM * 0.18), destPoint(planeLat, planeLon, h, rM * 1.12), "#FFC21A", 3f)
                tags += roseLabel(destPoint(planeLat, planeLon, h, rM * 1.28), "HDG ${deg3(h - mv)}", 0.0, ink, 13f)
            }
            // Ślad (zielony, do środka), nastawa HDG autopilota (cyjan, na zewnątrz), namiar na punkt odniesienia (magenta)
            live?.attitude?.trackTrueDeg?.takeIf { (live?.speed?.gsKt ?: 0.0) > 30 }?.let { t -> lines += roseMarker(planeLat, planeLon, rM, t, trkHex, outside = false) }
            live?.autopilot?.hdgDeg?.let { b -> lines += roseMarker(planeLat, planeLon, rM, b + mv, bugHex) }
            r?.let { lines += roseMarker(planeLat, planeLon, rM * 0.86, Geo.bearing(planeLat, planeLon, it.lat, it.lon), navHex) }
        }
        if (scale > 0 && r != null) {
            val rr = scale * 1.1
            val rose = compassRose(r.lat, r.lon, rr, mv, navHex, tickDeg = 10, labelSize = 11f)
            lines += rose.lines; labels += rose.labels
            r.name?.let { tags += roseLabel(destPoint(r.lat, r.lon, 180.0, rr * 0.3), it, 0.0, navHex, 12f) }
            if (origin != null && fix != null) {
                dash += segment(Point.fromLngLat(r.lon, r.lat), Point.fromLngLat(origin.longitude, origin.latitude), ink2, 1.8f)
                val mid = destPoint(r.lat, r.lon, Geo.bearing(r.lat, r.lon, origin.latitude, origin.longitude), fix.distNm * 1852 / 2)
                tags += roseLabel(mid, "R-${deg3(fix.radialMag)} · ${"%.1f".format(fix.distNm)} NM", 0.0, ink, 12f)
            }
            selRadial?.let { sel ->
                val brg = sel + mv
                val len = maxOf((fix?.distNm ?: 0.0) * 1852 * 1.3, rr * 4).coerceAtMost(750_000.0)
                lines += segment(destPoint(r.lat, r.lon, brg, rr * 0.15), destPoint(r.lat, r.lon, brg, len), navHex, 3.4f)
                // Grotki w kierunku lotu po radialu: IN do punktu, OUT od punktu
                val dir = if (inbound) brg + 180 else brg
                listOf(0.35, 0.7).forEach { f ->
                    val tip = destPoint(r.lat, r.lon, brg, len * f)
                    val a = rr * 0.16
                    lines += segment(destPoint(tip.latitude(), tip.longitude(), dir + 150, a), tip, navHex, 3.4f)
                    lines += segment(destPoint(tip.latitude(), tip.longitude(), dir - 150, a), tip, navHex, 3.4f)
                }
                val course = if (inbound) sel + 180.0 else sel.toDouble()
                tags += roseLabel(destPoint(r.lat, r.lon, brg, len * 0.52), "R-${deg3(sel.toDouble())} ${if (inbound) "IN" else "OUT"} · ${deg3(course)}°", 0.0, navHex, 13f)
            }
        }
        (st.getSource(SRC_ROSE) as? GeoJsonSource)?.setGeoJson(FeatureCollection.fromFeatures(lines))
        (st.getSource(SRC_ROSE_LBL) as? GeoJsonSource)?.setGeoJson(FeatureCollection.fromFeatures(labels))
        (st.getSource(SRC_ROSE_TAG) as? GeoJsonSource)?.setGeoJson(FeatureCollection.fromFeatures(tags))
        (st.getSource(SRC_RADIAL_DASH) as? GeoJsonSource)?.setGeoJson(FeatureCollection.fromFeatures(dash))
        (st.getSource(SRC_REF) as? GeoJsonSource)?.setGeoJson(
            if (r == null) FeatureCollection.fromFeatures(emptyList()) else FeatureCollection.fromFeature(Feature.fromGeometry(Point.fromLngLat(r.lon, r.lat))))
    }

    Box(Modifier.fillMaxSize()) {
        AndroidView({ mapView }, Modifier.fillMaxSize())
        // Bez samolotu radial mierzony od środka mapy
        if (radialMode && (planeLat == null || planeLon == null)) Text(
            "+", color = c.magenta, fontSize = 30.sp, fontWeight = FontWeight.Light, modifier = Modifier.align(Alignment.Center),
        )

        // Góra: legenda kategorii i (gdy jest) wyświetlany lot z dziennika; prawa strona: zwarta grupa przycisków
        Row(
            Modifier.statusBarsPadding().padding(start = 10.dp, top = 8.dp, end = 64.dp).fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically,
        ) {
            Legend()
            if (shownFlight != null) MapChip("${shownFlight.from ?: "?"} → ${shownFlight.to ?: "?"}", true, Icons.Outlined.Close) { vm.showFlightOnMap(null) }
        }

        // Przyciski mapy pod kompasem w jednej pigułce: warstwy, śledzenie samolotu, przybliżenie
        Column(
            Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(top = 104.dp, end = 10.dp)
                .clip(RoundedCornerShape(14.dp)).background(c.card.copy(alpha = 0.94f)).border(1.dp, c.rule, RoundedCornerShape(14.dp)),
        ) {
            Box {
                MapKey(Icons.Outlined.Layers, loc("Warstwy mapy", "Map layers"), layersMenu) { layersMenu = true }
                androidx.compose.material3.DropdownMenu(layersMenu, { layersMenu = false }) {
                    @Composable fun item(label: String, on: Boolean, toggle: () -> Unit) =
                        androidx.compose.material3.DropdownMenuItem(text = { Text((if (on) "✓  " else "     ") + label) }, onClick = toggle)
                    item(loc("Pogoda na lotniskach (METAR)", "Airport weather (METAR)"), showMetars) { showMetars = !showMetars }
                    item("SIGMET", showSigmets) { showSigmets = !showSigmets }
                    item(loc("Plan lotu z symulatora (GPS)", "Simulator flight plan (GPS)"), showRoute) { showRoute = !showRoute }
                    item(loc("Pomoce nawigacyjne z MSFS", "Navaids from MSFS") + if (nearbyNav.isEmpty()) loc(" (brak danych)", " (no data)") else "", showNav) { showNav = !showNav }
                }
            }
            if (live != null) MapKey(Icons.Outlined.MyLocation, loc("Śledź samolot", "Follow aircraft"), follow) { follow = !follow }
            if (live != null) MapKey(Icons.Outlined.Explore, loc("Róża kursów na samolocie", "Heading rose on the aircraft"), showRose) { showRose = !showRose }
            MapKey(Icons.Outlined.TrackChanges, loc("Radiale od punktu", "Radials from a point"), radialMode) {
                radialMode = !radialMode; picked = null
                if (!radialMode) { ref = null; selRadial = null }
            }
            MapKey(Icons.Outlined.Add, loc("Przybliż", "Zoom in"), false) { mapView.getMapAsync { it.animateCamera(CameraUpdateFactory.zoomIn(), 250) } }
            MapKey(Icons.Outlined.Remove, loc("Oddal", "Zoom out"), false) { mapView.getMapAsync { it.animateCamera(CameraUpdateFactory.zoomOut(), 250) } }
        }

        if (radialMode) RadialCard(
            ref, fix, guide, selRadial, inbound, magnetic = magVar != null, hasAircraft = planeLat != null && planeLon != null,
            gsKt = live?.speed?.gsKt,
            onInbound = { inbound = it; if (selRadial == null) selRadial = fix?.let { f -> radialOf(f.radialMag) } },
            onRadial = { selRadial = it },
            onNewPoint = { ref = null; selRadial = null },
            onClose = { radialMode = false; ref = null; selRadial = null },
            modifier = Modifier.align(Alignment.BottomCenter),
        )

        picked?.let { m ->
            Column(
                Modifier.align(Alignment.BottomCenter).padding(12.dp).fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp)).background(c.card.copy(alpha = 0.96f))
                    .clickable { openLatest(m.station) }.padding(14.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(m.station, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 20.sp, color = c.ink)
                    Spacer(Modifier.width(10.dp))
                    CategoryTag(m.flightCategory)
                    Spacer(Modifier.weight(1f))
                    Icon(Icons.Outlined.Close, loc("Zamknij", "Close"), tint = c.ink2, modifier = Modifier.size(22.dp).clickable { picked = null })
                }
                Text(m.name.orEmpty(), style = MaterialTheme.typography.bodySmall, color = c.ink2)
                Text(m.raw, fontFamily = Mono, fontSize = 12.sp, color = c.accent, modifier = Modifier.padding(top = 6.dp))
                Row(Modifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(loc("Dotknij, aby otworzyć briefing", "Tap to open the briefing"), style = MaterialTheme.typography.labelSmall, color = c.ink2, modifier = Modifier.weight(1f))
                    // Radiale od tego lotniska
                    Text(
                        loc("Radiale", "Radials"), style = MaterialTheme.typography.labelMedium, color = c.accent,
                        modifier = Modifier.clip(RoundedCornerShape(50)).border(1.dp, c.accent, RoundedCornerShape(50))
                            .clickable {
                                val lat = m.lat; val lon = m.lon
                                if (lat != null && lon != null) { ref = RefPoint(lat, lon, m.station); selRadial = null; radialMode = true; picked = null }
                            }.padding(horizontal = 12.dp, vertical = 6.dp),
                    )
                }
            }
        }
    }
}

private fun requestArea(map: MapLibreMap, vm: MainViewModel) {
    if (map.cameraPosition.zoom < 4.0) return
    val b = map.projection.visibleRegion.latLngBounds
    vm.loadMapArea(b.latitudeSouth, b.longitudeWest, b.latitudeNorth, b.longitudeEast)
}

private fun addOverlay(st: Style, c: FmColors) {
    st.addSource(GeoJsonSource(SRC_SIGMET))
    st.addLayer(FillLayer("oab-sigmet-fill", SRC_SIGMET).withProperties(
        PropertyFactory.fillColor(hex(c.caution)), PropertyFactory.fillOpacity(0.16f),
    ))
    st.addLayer(LineLayer("oab-sigmet-line", SRC_SIGMET).withProperties(
        PropertyFactory.lineColor(hex(c.caution)), PropertyFactory.lineWidth(1.6f), PropertyFactory.lineDasharray(arrayOf(3f, 2f)),
    ))
    // Bieżący odcinek planu lotu z GPS symulatora w magencie — kolor aktywnej nawigacji w awionice
    st.addSource(GeoJsonSource(SRC_ROUTE))
    st.addLayer(LineLayer("oab-route-line", SRC_ROUTE).withProperties(
        PropertyFactory.lineColor(hex(c.magenta)), PropertyFactory.lineWidth(3.5f),
    ))
    st.addSource(GeoJsonSource(SRC_ROUTE_PTS))
    st.addLayer(CircleLayer("oab-route-pts", SRC_ROUTE_PTS).withProperties(
        PropertyFactory.circleRadius(3.5f), PropertyFactory.circleColor(hex(c.magenta)),
    ))
    st.addLayer(SymbolLayer("oab-route-label", SRC_ROUTE_PTS).withProperties(
        PropertyFactory.textField(Expression.get("id")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(10f),
        PropertyFactory.textOffset(arrayOf(0f, -1.2f)),
        PropertyFactory.textColor(hex(c.magenta)),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(1.5f),
        PropertyFactory.textOptional(true),
    ).apply { minZoom = 5.5f })
    // Budynki 3D z kafli OpenMapTiles (OpenFreeMap) — tylko w widoku 3D i przy dużym przybliżeniu
    if (st.getSource("openmaptiles") != null) st.addLayer(FillExtrusionLayer(LYR_BUILDINGS, "openmaptiles").withSourceLayer("building").withProperties(
        PropertyFactory.fillExtrusionColor(if (c.isNight) "#2A2A2A" else "#D5DBE1"),
        PropertyFactory.fillExtrusionHeight(Expression.coalesce(Expression.get("render_height"), Expression.literal(8))),
        PropertyFactory.fillExtrusionBase(Expression.coalesce(Expression.get("render_min_height"), Expression.literal(0))),
        PropertyFactory.fillExtrusionOpacity(0.85f),
    ).apply { minZoom = 14f })
    // Lotniska i pomoce z MSFS: kolor wg typu jak na ND (lotniska cyjan, VOR zieleń, NDB bursztyn, punkty szarość)
    st.addSource(GeoJsonSource(SRC_NAV))
    val navColor = Expression.get("color")
    st.addLayer(CircleLayer("oab-nav-pts", SRC_NAV).withProperties(
        PropertyFactory.circleRadius(4f), PropertyFactory.circleColor(hex(c.paper)),
        PropertyFactory.circleStrokeColor(navColor), PropertyFactory.circleStrokeWidth(2f),
    ).apply { minZoom = 6f })
    st.addLayer(SymbolLayer("oab-nav-label", SRC_NAV).withProperties(
        PropertyFactory.textField(Expression.get("id")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(10f),
        PropertyFactory.textOffset(arrayOf(0f, 1.1f)),
        PropertyFactory.textColor(navColor),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(1.5f),
        PropertyFactory.textOptional(true),
    ).apply { minZoom = 8f })
    // Tor lotu w kolorze wysokości, w 3D także cienka linia na wysokości lotu; samolot nad wszystkim
    // Bez upraszczania: wąskie wstęgi znikałyby przy małym przybliżeniu
    // Tor w 3D: półprzezroczysta ściana od toru na ziemi do linii w powietrzu, nad nią cienka linia na wysokości lotu
    st.addSource(GeoJsonSource(SRC_WALL3D, GeoJsonOptions().withTolerance(0f)))
    st.addLayer(FillExtrusionLayer(LYR_WALL3D, SRC_WALL3D).withProperties(
        PropertyFactory.fillExtrusionColor(altColor()),
        PropertyFactory.fillExtrusionHeight(Expression.get("h")),
        PropertyFactory.fillExtrusionBase(Expression.get("base")),
        PropertyFactory.fillExtrusionOpacity(0.28f),
    ))
    st.addSource(GeoJsonSource(SRC_LINE3D, GeoJsonOptions().withTolerance(0f)))
    st.addLayer(FillExtrusionLayer(LYR_LINE3D, SRC_LINE3D).withProperties(
        PropertyFactory.fillExtrusionColor(altColor()),
        PropertyFactory.fillExtrusionHeight(Expression.get("h")),
        PropertyFactory.fillExtrusionBase(Expression.get("base")),
        PropertyFactory.fillExtrusionOpacity(0.95f),
    ))
    st.addSource(GeoJsonSource(SRC_TRACK, GeoJsonOptions().withLineMetrics(true)))
    st.addLayer(LineLayer(LYR_TRACK, SRC_TRACK).withProperties(
        PropertyFactory.lineColor(hex(c.ink)), PropertyFactory.lineWidth(3.5f),
        PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"),
    ))
    st.addSource(GeoJsonSource(SRC_TRACK_LBL))
    st.addLayer(SymbolLayer("oab-track-label", SRC_TRACK_LBL).withProperties(
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(11f),
        PropertyFactory.textOffset(arrayOf(0f, -1.1f)),
        PropertyFactory.textColor(altColor()),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(1.5f),
        // Wysokości toru są ważniejsze niż nazwy miejscowości podkładu
        PropertyFactory.textAllowOverlap(true),
        PropertyFactory.textIgnorePlacement(true),
    ))
    // Róże kursów i radiale: na ziemi, pod samolotem i stacjami METAR
    st.addSource(GeoJsonSource(SRC_RADIAL_DASH))
    st.addLayer(LineLayer("oab-radial-dash", SRC_RADIAL_DASH).withProperties(
        PropertyFactory.lineColor(Expression.get("color")), PropertyFactory.lineWidth(Expression.get("w")),
        PropertyFactory.lineDasharray(arrayOf(2.5f, 2f)),
    ))
    st.addSource(GeoJsonSource(SRC_ROSE, GeoJsonOptions().withTolerance(0.1f)))
    st.addLayer(LineLayer("oab-rose-line", SRC_ROSE).withProperties(
        PropertyFactory.lineColor(Expression.get("color")), PropertyFactory.lineWidth(Expression.get("w")),
        PropertyFactory.lineCap("round"), PropertyFactory.lineJoin("round"), PropertyFactory.lineOpacity(0.92f),
    ))
    st.addSource(GeoJsonSource(SRC_ROSE_LBL))
    // Opisy róży leżą na mapie, obrócone na zewnątrz jak na tarczy kompasu
    st.addLayer(SymbolLayer("oab-rose-label", SRC_ROSE_LBL).withProperties(
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(Expression.get("size")),
        PropertyFactory.textRotate(Expression.get("rot")),
        PropertyFactory.textRotationAlignment("map"),
        PropertyFactory.textPitchAlignment("map"),
        PropertyFactory.textColor(Expression.get("color")),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(1.5f),
        PropertyFactory.textAllowOverlap(true),
        PropertyFactory.textIgnorePlacement(true),
    ))
    st.addSource(GeoJsonSource(SRC_REF))
    st.addLayer(CircleLayer("oab-radial-ref", SRC_REF).withProperties(
        PropertyFactory.circleRadius(5f), PropertyFactory.circleColor(hex(c.paper)),
        PropertyFactory.circleStrokeColor(hex(c.magenta)), PropertyFactory.circleStrokeWidth(2.5f),
    ))
    st.addSource(GeoJsonSource(SRC_ROSE_TAG))
    st.addLayer(SymbolLayer("oab-rose-tag", SRC_ROSE_TAG).withProperties(
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(Expression.get("size")),
        PropertyFactory.textColor(Expression.get("color")),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(2f),
        PropertyFactory.textAllowOverlap(true),
        PropertyFactory.textIgnorePlacement(true),
    ))
    st.addImage(IMG_PLANE, planeBitmap())
    st.addSource(GeoJsonSource(SRC_PLANE))
    st.addSource(GeoJsonSource(SRC_METAR))
    // Okrąg w kolorze kategorii, jak symbol lotniska na wyświetlaczu nawigacyjnym
    st.addLayer(CircleLayer(LYR_METAR, SRC_METAR).withProperties(
        PropertyFactory.circleRadius(Expression.interpolate(Expression.linear(), Expression.zoom(),
            Expression.stop(4, 3f), Expression.stop(8, 6f), Expression.stop(11, 9f))),
        PropertyFactory.circleColor(hex(c.paper)),
        PropertyFactory.circleStrokeColor(Expression.get("color")),
        PropertyFactory.circleStrokeWidth(2.5f),
    ))
    st.addLayer(SymbolLayer("oab-metar-label", SRC_METAR).withProperties(
        PropertyFactory.textField(Expression.get("id")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(11f),
        PropertyFactory.textOffset(arrayOf(0f, 1.3f)),
        PropertyFactory.textColor(hex(c.ink)),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(1.5f),
        PropertyFactory.textOptional(true),
    ).apply { minZoom = 6.5f })
    // Samolot w 3D zawieszony na wysokości lotu (symbol przy ziemi zostaje jako cień z wysokością)
    st.addSource(GeoJsonSource(SRC_PLANE3D, GeoJsonOptions().withTolerance(0f)))
    st.addLayer(FillExtrusionLayer(LYR_PLANE3D, SRC_PLANE3D).withProperties(
        PropertyFactory.fillExtrusionColor(hex(c.accent)),
        PropertyFactory.fillExtrusionHeight(Expression.get("h")),
        PropertyFactory.fillExtrusionBase(Expression.get("base")),
        PropertyFactory.fillExtrusionOpacity(1f),
    ))
    st.addLayer(SymbolLayer("oab-plane-layer", SRC_PLANE).withProperties(
        PropertyFactory.iconImage(IMG_PLANE),
        PropertyFactory.iconOpacity(0.4f),
        PropertyFactory.iconRotate(Expression.get("hdg")),
        PropertyFactory.iconRotationAlignment("map"),
        PropertyFactory.iconAllowOverlap(true),
        PropertyFactory.iconIgnorePlacement(true),
        // Z daleka mały znak, od zoomu 14 rośnie z mapą (×2 na poziom), aż zbliży się do rzeczywistej wielkości
        PropertyFactory.iconSize(Expression.interpolate(Expression.exponential(2f), Expression.zoom(),
            Expression.stop(10, 0.55f), Expression.stop(14, 0.45f), Expression.stop(16, 0.8f), Expression.stop(18, 3.2f))),
        // Bieżąca wysokość pod symbolem samolotu
        PropertyFactory.textField(Expression.get("label")),
        PropertyFactory.textFont(arrayOf("Noto Sans Regular")),
        PropertyFactory.textSize(11f),
        PropertyFactory.textOffset(arrayOf(0f, 2.0f)),
        PropertyFactory.textColor(hex(c.ink)),
        PropertyFactory.textHaloColor(hex(c.paper)),
        PropertyFactory.textHaloWidth(1.5f),
        PropertyFactory.textAllowOverlap(true),
    ))
}

/** Linia toru 3D i budynki są widoczne tylko w widoku 3D. */
private fun set3dLayers(st: Style, on: Boolean) {
    val v = PropertyFactory.visibility(if (on) "visible" else "none")
    st.getLayer(LYR_LINE3D)?.setProperties(v)
    st.getLayer(LYR_WALL3D)?.setProperties(v)
    st.getLayer(LYR_PLANE3D)?.setProperties(v)
    st.getLayer(LYR_BUILDINGS)?.setProperties(v)
}

@Composable
private fun MapChip(text: String, on: Boolean, icon: androidx.compose.ui.graphics.vector.ImageVector, onClick: () -> Unit) {
    val c = Fm.c
    // Włączony: pełny blok w kolorze tekstu; wyłączony: półprzezroczysta pigułka z obwódką
    Row(
        Modifier.clip(RoundedCornerShape(50)).background(if (on) c.ink else c.card.copy(alpha = 0.94f))
            .border(1.dp, if (on) c.ink else c.rule, RoundedCornerShape(50))
            .clickable(role = Role.Switch, onClick = onClick).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = if (on) c.paper else c.ink2, modifier = Modifier.size(16.dp))
        Spacer(Modifier.width(6.dp))
        Text(text, style = MaterialTheme.typography.labelMedium, color = if (on) c.paper else c.ink, maxLines = 1)
    }
}

@Composable
private fun Legend() {
    val c = Fm.c
    Row(
        Modifier.clip(RoundedCornerShape(50)).background(c.card.copy(alpha = 0.9f)).padding(horizontal = 10.dp, vertical = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically,
    ) {
        listOf("VFR", "MVFR", "IFR", "LIFR").forEach { k ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(9.dp).clip(CircleShape).background(c.category(k)))
                Spacer(Modifier.width(3.dp))
                Text(k, fontFamily = Mono, fontSize = 10.sp, color = c.ink2)
            }
        }
    }
}

/** Przycisk mapy (40 dp) w pigułce po prawej: włączony — wypełnione kółko pod ikoną. */
@Composable
private fun MapKey(icon: androidx.compose.ui.graphics.vector.ImageVector, desc: String, on: Boolean, onClick: () -> Unit) {
    val c = Fm.c
    Box(
        Modifier.size(42.dp).clickable(role = Role.Button, onClickLabel = desc, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(32.dp).clip(CircleShape).background(if (on) c.ink else Color.Transparent), contentAlignment = Alignment.Center) {
            Icon(icon, desc, tint = if (on) c.paper else c.ink, modifier = Modifier.size(19.dp))
        }
    }
}
