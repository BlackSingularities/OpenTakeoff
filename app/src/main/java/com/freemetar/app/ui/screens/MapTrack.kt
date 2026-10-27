package com.freemetar.app.ui.screens

import com.freemetar.app.data.link.TrackPoint
import com.freemetar.app.domain.Geo
import org.maplibre.geojson.Feature
import org.maplibre.geojson.FeatureCollection
import org.maplibre.geojson.LineString
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon
import kotlin.math.cos
import kotlin.math.hypot

/** Wysokość w zapisie jak na wyświetlaczu: od 18 000 ft poziom lotu, niżej stopy. */
fun altLabel(ft: Int): String = if (ft >= 18_000) "FL%03d".format((ft + 50) / 100) else "${(ft + 50) / 100 * 100} ft"

/** Skala kolorów wysokości (ft → RGB): nisko zieleń, średnio cyjan, wyżej błękit, przelot magenta. */
internal val ALT_STOPS = listOf(
    0 to intArrayOf(63, 224, 122),
    8_000 to intArrayOf(79, 209, 232),
    22_000 to intArrayOf(79, 163, 255),
    38_000 to intArrayOf(242, 92, 208),
)

/** Kolor wysokości jako RGB — ta sama skala co wyrażenie warstw mapy. */
internal fun altRgb(ft: Int): IntArray {
    if (ft <= ALT_STOPS.first().first) return ALT_STOPS.first().second
    if (ft >= ALT_STOPS.last().first) return ALT_STOPS.last().second
    val i = ALT_STOPS.indexOfLast { it.first <= ft }
    val (a, ca) = ALT_STOPS[i]; val (b, cb) = ALT_STOPS[i + 1]
    val f = (ft - a).toDouble() / (b - a)
    return IntArray(3) { k -> (ca[k] + (cb[k] - ca[k]) * f).toInt() }
}

/**
 * Przewyższenie wysokości toru 3D: przelot na FL240 to 7 km nad trasą długą na setki km — w prawdziwej skali
 * linia leżałaby prawie na ziemi, więc jak w widokach profilu wysokość jest pokazana 3× większa.
 */
internal const val LINE3D_EXAGGERATION = 3.0

/** Rzadszy tor do rysowania: najwyżej [max] punktów, końce zawsze zostają. */
internal fun thin(track: List<TrackPoint>, max: Int = 1500): List<TrackPoint> =
    if (track.size <= max) track else track.filterIndexed { i, _ -> i % (track.size / max + 1) == 0 } + track.last()

/**
 * Tor jako jedna linia. Krótkie odcinki osobno znikałyby przy uproszczeniu geometrii na małym
 * przybliżeniu, więc kolor wysokości idzie gradientem wzdłuż linii ([trackGradient]).
 */
internal fun trackLine(track: List<TrackPoint>): FeatureCollection {
    val t = thin(track)
    if (t.size < 2) return FeatureCollection.fromFeatures(emptyList())
    return FeatureCollection.fromFeature(Feature.fromGeometry(LineString.fromLngLats(t.map { Point.fromLngLat(it.lon, it.lat) })))
}

/**
 * Przystanki gradientu: (ułamek długości toru 0…1, wysokość ft), najwyżej [n] — line-progress
 * w MapLibre liczy się po długości, więc ułamki są z sumy odległości, nie z liczby punktów.
 */
internal fun trackGradient(track: List<TrackPoint>, n: Int = 48): List<Pair<Double, Int>> {
    val t = thin(track)
    if (t.size < 2) return emptyList()
    val cum = DoubleArray(t.size)
    for (i in 1 until t.size) cum[i] = cum[i - 1] + Geo.distanceNm(t[i - 1].lat, t[i - 1].lon, t[i].lat, t[i].lon)
    val total = cum.last()
    if (total <= 0.0) return listOf(0.0 to t.first().altFt, 1.0 to t.last().altFt)
    val out = ArrayList<Pair<Double, Int>>()
    var j = 0
    for (k in 0..n) {
        val target = total * k / n
        while (j < t.size - 1 && cum[j + 1] < target) j++
        out += (target / total) to t[j].altFt
    }
    return out
}

/**
 * Tor w 3D jako cienka linia zawieszona na wysokości lotu (od [base] do [h], metry). Z [wall] — ściana od ziemi
 * do spodu linii ([trackWall3d]).
 *
 * Bryła MapLibre ma płaski wierzch, więc linia jest złożona z krótkich czworokątów między kolejnymi punktami,
 * a zmiana wysokości między punktami jest dzielona na stopnie najwyżej [stepFt] — wznoszenie wygląda jak gładka
 * rampa, nie schody. Sąsiednie czworokąty mają wspólne krawędzie (narożniki liczone raz dla całego toru),
 * więc na zakrętach nie ma szczelin.
 */
internal fun trackLine3d(track: List<TrackPoint>, halfWidthM: Double = 45.0, thicknessM: Double = 70.0, wall: Boolean = false, stepFt: Double = 25.0): FeatureCollection {
    val t = dedupe(thin(track, 1200))
    if (t.size < 2) return FeatureCollection.fromFeatures(emptyList())
    val (left, right) = edges(t, halfWidthM)
    // Budżet stopni na cały tor, żeby długi lot nie dał dziesiątek tysięcy brył
    var climb = 0.0
    for (i in 1 until t.size) climb += kotlin.math.abs(t[i].altFt - t[i - 1].altFt)
    val step = maxOf(stepFt, climb / 3000.0)
    val feats = ArrayList<Feature>(t.size * 2)
    fun lerp(a: Point, b: Point, f: Double) = Point.fromLngLat(a.longitude() + (b.longitude() - a.longitude()) * f, a.latitude() + (b.latitude() - a.latitude()) * f)
    for (i in 0 until t.size - 1) {
        val a0 = t[i].altFt.toDouble(); val a1 = t[i + 1].altFt.toDouble()
        val n = kotlin.math.ceil(kotlin.math.abs(a1 - a0) / step).toInt().coerceIn(1, 12)
        for (k in 0 until n) {
            val f0 = k.toDouble() / n; val f1 = (k + 1).toDouble() / n
            val l0 = lerp(left[i], left[i + 1], f0); val l1 = lerp(left[i], left[i + 1], f1)
            val r0 = lerp(right[i], right[i + 1], f0); val r1 = lerp(right[i], right[i + 1], f1)
            val alt = a0 + (a1 - a0) * (f0 + f1) / 2
            val h = alt * 0.3048 * LINE3D_EXAGGERATION
            feats += Feature.fromGeometry(Polygon.fromLngLats(listOf(listOf(l0, l1, r1, r0, l0)))).apply {
                addNumberProperty("alt", alt)
                // Ściana: od ziemi do spodu linii; linia: tylko jej grubość
                addNumberProperty("h", if (wall) (h - thicknessM / 2).coerceAtLeast(0.0) else h + thicknessM / 2)
                addNumberProperty("base", if (wall) 0.0 else (h - thicknessM / 2).coerceAtLeast(0.0))
            }
        }
    }
    return FeatureCollection.fromFeatures(feats)
}

/** Bez punktów leżących (prawie) w tym samym miejscu — dla nich nie da się wyznaczyć kierunku toru. */
private fun dedupe(pts: List<TrackPoint>): List<TrackPoint> {
    val out = ArrayList<TrackPoint>(pts.size)
    for (p in pts) {
        val q = out.lastOrNull()
        if (q == null || Geo.distanceNm(q.lat, q.lon, p.lat, p.lon) * 1852 >= 3.0) out += p
    }
    if (out.size == 1 && pts.size > 1) out += pts.last()
    return out
}

/**
 * Lewa i prawa krawędź wstęgi w każdym punkcie: przesunięcie prostopadle do dwusiecznej kierunków
 * sąsiednich odcinków, wydłużone na zakręcie (ścięcie ograniczone do 2×, żeby ostre nawroty nie strzelały kolcami).
 */
private fun edges(pts: List<TrackPoint>, half: Double): Pair<List<Point>, List<Point>> {
    val mPerLat = 111_320.0
    val left = ArrayList<Point>(pts.size); val right = ArrayList<Point>(pts.size)
    fun dir(a: TrackPoint, b: TrackPoint): Pair<Double, Double> {
        val mPerLon = 111_320.0 * cos(Math.toRadians((a.lat + b.lat) / 2))
        val dx = (b.lon - a.lon) * mPerLon; val dy = (b.lat - a.lat) * mPerLat
        val len = hypot(dx, dy).coerceAtLeast(1e-6)
        return dx / len to dy / len
    }
    pts.forEachIndexed { i, p ->
        val d1 = if (i > 0) dir(pts[i - 1], p) else null
        val d2 = if (i < pts.size - 1) dir(p, pts[i + 1]) else null
        val (ux, uy) = when {
            d1 != null && d2 != null -> (d1.first + d2.first) to (d1.second + d2.second)
            else -> d1 ?: d2!!
        }
        val ul = hypot(ux, uy)
        val (tx, ty) = if (ul < 1e-6) (d2 ?: d1!!) else (ux / ul) to (uy / ul)
        // Długość ścięcia: half / cos(połowy kąta zakrętu)
        val cosHalf = d2?.let { tx * it.first + ty * it.second } ?: 1.0
        val miter = half / cosHalf.coerceAtLeast(0.5)
        val mPerLon = 111_320.0 * cos(Math.toRadians(p.lat))
        val ox = -ty * miter / mPerLon; val oy = tx * miter / mPerLat
        left += Point.fromLngLat(p.lon + ox, p.lat + oy)
        right += Point.fromLngLat(p.lon - ox, p.lat - oy)
    }
    return left to right
}

/**
 * Półprzezroczysta ściana łącząca tor na ziemi z linią na wysokości lotu: bardzo wąska wstęga
 * wyciągnięta od ziemi do spodu linii 3D (te same kawałki toru, więc ściana idzie dokładnie pod linią).
 */
internal fun trackWall3d(track: List<TrackPoint>): FeatureCollection = trackLine3d(track, halfWidthM = 6.0, wall = true)

/**
 * Etykiety wysokości na torze: co ok. 1/10 długości oraz na szczycie wznoszenia,
 * pomijając miejsca, gdzie wysokość prawie się nie zmieniła od poprzedniej etykiety.
 */
internal fun trackLabels(track: List<TrackPoint>): FeatureCollection {
    val t = thin(track, 600)
    if (t.size < 4) return FeatureCollection.fromFeatures(emptyList())
    val picks = sortedSetOf<Int>()
    val every = (t.size / 10).coerceAtLeast(1)
    for (i in every until t.size - 1 step every) picks += i
    picks += t.indices.maxBy { t[it].altFt }
    var lastAlt = Int.MIN_VALUE
    val feats = picks.mapNotNull { i ->
        val p = t[i]
        if (kotlin.math.abs(p.altFt - lastAlt) < 500 && i != picks.last()) return@mapNotNull null
        lastAlt = p.altFt
        Feature.fromGeometry(Point.fromLngLat(p.lon, p.lat)).apply {
            addStringProperty("label", altLabel(p.altFt))
            addNumberProperty("alt", p.altFt)
        }
    }
    return FeatureCollection.fromFeatures(feats)
}

/**
 * Rozpiętość samolotu 3D w metrach: z bliska rzeczywista (ok. 36 m — samolot mieści się na drodze kołowania),
 * z daleka stała wielkość na ekranie, malejąca z przybliżeniem od 36 px (zoom ≤ 10) do 14 px (zoom ≥ 15).
 */
internal fun planeSpanM(zoom: Double, mPerPx: Double): Double {
    val px = 36.0 - (zoom - 10.0).coerceIn(0.0, 5.0) / 5.0 * 22.0
    return maxOf(36.0, px * mPerPx).coerceAtMost(6000.0)
}

/**
 * Samolot w 3D: płaska sylwetka (kadłub, skrzydła, usterzenie) obrócona zgodnie z kursem i zawieszona
 * na wysokości lotu — w tym samym przewyższeniu co linia toru, więc leży na jej końcu.
 * [spanM] — rozpiętość w metrach (dobierana do przybliżenia mapy, żeby symbol miał stałą wielkość na ekranie).
 */
internal fun planeShape3d(lat: Double, lon: Double, hdgTrueDeg: Double, altFt: Double, spanM: Double): FeatureCollection {
    // Sylwetka w jednostkach rozpiętości: x w prawo, y do przodu (nos = +0,5)
    val shape = listOf(
        0.0 to 0.5, 0.05 to 0.38, 0.05 to 0.1, 0.5 to -0.02, 0.5 to -0.08, 0.05 to -0.04, 0.04 to -0.32,
        0.18 to -0.42, 0.18 to -0.47, 0.0 to -0.44, -0.18 to -0.47, -0.18 to -0.42, -0.04 to -0.32, -0.05 to -0.04,
        -0.5 to -0.08, -0.5 to -0.02, -0.05 to 0.1, -0.05 to 0.38, 0.0 to 0.5,
    )
    val h = Math.toRadians(hdgTrueDeg)
    val mPerLon = 111_320.0 * cos(Math.toRadians(lat))
    val ring = shape.map { (x, y) ->
        val east = (x * cos(h) + y * kotlin.math.sin(h)) * spanM
        val north = (-x * kotlin.math.sin(h) + y * cos(h)) * spanM
        Point.fromLngLat(lon + east / mPerLon, lat + north / 111_320.0)
    }
    val top = altFt * 0.3048 * LINE3D_EXAGGERATION
    val thick = spanM * 0.08
    return FeatureCollection.fromFeature(Feature.fromGeometry(Polygon.fromLngLats(listOf(ring))).apply {
        addNumberProperty("alt", altFt)
        addNumberProperty("h", top + thick / 2)
        addNumberProperty("base", (top - thick / 2).coerceAtLeast(0.0))
    })
}
