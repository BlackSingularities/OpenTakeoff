package com.freemetar.app.domain

import com.freemetar.app.data.LatLon
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.roundToLong
import kotlin.math.sqrt

/*
 * Mapa lotniska z OpenStreetMap (odpowiedź Overpass API): pasy, drogi kołowania z oznaczeniami, płyty,
 * terminale, stanowiska, bramki i miejsca oczekiwania. Z dróg kołowania i pasów powstaje graf, po którym
 * liczona jest trasa kołowania („A B2 H 27” albo sam cel: pas lub stanowisko) i położenie samolotu
 * („na A, przed tobą B2”). Ta sama logika jest w programie na Windows (`aero/layout.rs`).
 */

enum class LineKind { RUNWAY, TAXIWAY, TAXILANE, HOLD }
enum class AreaKind { APRON, TERMINAL, HANGAR, RUNWAY }
enum class SpotKind { HOLD, STAND, GATE }

data class LayoutLine(val kind: LineKind, val ref: String, val pts: List<LatLon>, val widthM: Double?)
data class LayoutArea(val kind: AreaKind, val name: String, val ring: List<LatLon>)
data class LayoutSpot(val kind: SpotKind, val ref: String, val lat: Double, val lon: Double)

/** Punkt opisany na trasie: przecięcie pasa (zatrzymaj się przed) albo cel. */
data class RouteMark(val lat: Double, val lon: Double, val label: String, val runway: Boolean)

data class TaxiRoute(
    val pts: List<LatLon>,
    val lengthM: Double,
    /** Kolejne drogi kołowania na trasie (bez powtórzeń). */
    val via: List<String>,
    /** Pasy przecinane po drodze — przed każdym trzeba się zatrzymać. */
    val crossings: List<RouteMark>,
    val end: RouteMark?,
)

sealed class RouteResult {
    data class Ok(val route: TaxiRoute) : RouteResult()
    data class Error(val message: String) : RouteResult()
}

/** Położenie samolotu na lotnisku: droga, na której stoi, najbliższe skrzyżowanie i ostrzeżenie przed pasem. */
data class TaxiInfo(
    val on: String?,
    val onRunway: String?,
    val next: String?,
    val nextM: Double?,
    val runwayAhead: String?,
    val runwayAheadM: Double?,
    val holdAheadM: Double?,
)

private const val M_PER_DEG = 111_320.0

internal fun metersBetween(a: LatLon, b: LatLon): Double {
    val dy = (b.lat - a.lat) * M_PER_DEG
    val dx = (b.lon - a.lon) * M_PER_DEG * cos(Math.toRadians((a.lat + b.lat) / 2))
    return sqrt(dx * dx + dy * dy)
}

/** Oznaczenie końca pasa bez zer wiodących i przedrostka: „RWY 09L” → „9L”; null, gdy to nie pas. */
internal fun runwayKey(s: String): String? {
    val m = Regex("^(?:RWY|RW)?0*(\\d{1,2})([LRCW]?)$").find(s.trim().uppercase()) ?: return null
    val n = m.groupValues[1].toIntOrNull() ?: return null
    if (n !in 1..36) return null
    return "$n${m.groupValues[2]}"
}

/** Końce pasa z oznaczenia OSM („09/27”, „09L-27R”). */
internal fun runwayEnds(ref: String): List<String> = ref.split('/', '-', ' ', ';').mapNotNull(::runwayKey)

/** Kąt między kierunkami (0–180°). */
internal fun angleDiff(a: Double, b: Double): Double {
    val d = abs(((a - b) % 360 + 540) % 360 - 180)
    return d
}

class AirportLayout(
    val ident: String,
    val lines: List<LayoutLine>,
    val areas: List<LayoutArea>,
    val spots: List<LayoutSpot>,
) {
    val isEmpty: Boolean get() = lines.isEmpty() && areas.isEmpty() && spots.isEmpty()

    /** Środek i promień (m) obszaru z danymi — do wyboru mapy lotniska pod samolotem. */
    val center: LatLon? by lazy {
        val all = lines.flatMap { it.pts } + areas.flatMap { it.ring }
        if (all.isEmpty()) null else LatLon(all.sumOf { it.lat } / all.size, all.sumOf { it.lon } / all.size)
    }

    /** Pasy z oznaczeniami obu końców (do przycisków celu trasy). */
    val runwayEndKeys: List<String> by lazy {
        lines.filter { it.kind == LineKind.RUNWAY }.flatMap { runwayEnds(it.ref) }.distinct().sortedWith(compareBy({ it.filter(Char::isDigit).toIntOrNull() ?: 0 }, { it }))
    }

    // ---------- graf ----------

    private class Edge(val to: Int, val line: Int, val m: Double)

    private class Graph(val nodes: List<LatLon>, val adj: List<List<Edge>>, val nodeLines: List<Set<Int>>, val index: Map<Pair<Long, Long>, Int>)

    private fun key(p: LatLon) = (p.lat * 1e7).roundToLong() to (p.lon * 1e7).roundToLong()

    private val graph: Graph by lazy {
        val nodes = ArrayList<LatLon>()
        val index = HashMap<Pair<Long, Long>, Int>()
        val adj = ArrayList<MutableList<Edge>>()
        val nodeLines = ArrayList<MutableSet<Int>>()
        fun node(p: LatLon): Int = index.getOrPut(key(p)) {
            nodes.add(p); adj.add(ArrayList<Edge>()); nodeLines.add(HashSet<Int>()); nodes.size - 1
        }
        lines.forEachIndexed { li, l ->
            if (l.kind == LineKind.HOLD) return@forEachIndexed
            var prev = -1
            for (p in l.pts) {
                val n = node(p)
                nodeLines[n] += li
                if (prev >= 0 && prev != n) {
                    val m = metersBetween(nodes[prev], nodes[n])
                    adj[prev] += Edge(n, li, m); adj[n] += Edge(prev, li, m)
                }
                prev = n
            }
        }
        Graph(nodes, adj, nodeLines, index)
    }

    private fun lineName(li: Int): String {
        val l = lines[li]
        return if (l.kind == LineKind.RUNWAY) "RWY ${l.ref}" else l.ref
    }

    /** Najbliższy odcinek drogi (bez pasów, chyba że [runways]): (linia, indeks odcinka, rzut, odległość). */
    private data class Proj(val line: Int, val seg: Int, val p: LatLon, val d: Double)

    private fun project(a: LatLon, b: LatLon, p: LatLon): Pair<LatLon, Double> {
        val k = cos(Math.toRadians(p.lat))
        val ax = a.lon * k; val ay = a.lat; val bx = b.lon * k; val by = b.lat; val px = p.lon * k; val py = p.lat
        val dx = bx - ax; val dy = by - ay
        val len2 = dx * dx + dy * dy
        val t = if (len2 <= 0) 0.0 else (((px - ax) * dx + (py - ay) * dy) / len2).coerceIn(0.0, 1.0)
        val q = LatLon(a.lat + (b.lat - a.lat) * t, a.lon + (b.lon - a.lon) * t)
        return q to metersBetween(q, p)
    }

    private fun nearest(p: LatLon, accept: (LayoutLine) -> Boolean): Proj? {
        var best: Proj? = null
        lines.forEachIndexed { li, l ->
            if (!accept(l)) return@forEachIndexed
            for (i in 0 until l.pts.size - 1) {
                val (q, d) = project(l.pts[i], l.pts[i + 1], p)
                if (best == null || d < best!!.d) best = Proj(li, i, q, d)
            }
        }
        return best
    }

    // ---------- trasa kołowania ----------

    private sealed class Dest {
        object None : Dest()
        data class Runway(val key: String) : Dest()
        data class Stand(val spot: LayoutSpot) : Dest()
    }

    /**
     * Trasa od punktu [fromLat]/[fromLon] (samolot albo środek mapy). [text]: drogi kołowania po kolei i na końcu
     * cel — pas („27”, „RWY 09L”) albo stanowisko/bramka; sam cel liczy najkrótszą trasę.
     */
    fun route(fromLat: Double, fromLon: Double, text: String): RouteResult {
        val g = graph
        if (g.nodes.isEmpty()) return RouteResult.Error(loc("taxi.no_taxiways"))
        val filler = setOf("VIA", "TWY", "TAXI", "TO", "DO", "PRZEZ", "RWY", "RW", "PAS", "HOLD", "SHORT", "STAND", "GATE", "A:", "POR")
        val tokens = text.uppercase().split(Regex("[\\s,;>→]+")).map { it.trim() }.filter { it.isNotEmpty() && it !in filler }
        if (tokens.isEmpty()) return RouteResult.Error(loc("taxi.enter_route"))
        val twyRefs = lines.filter { (it.kind == LineKind.TAXIWAY || it.kind == LineKind.TAXILANE) && it.ref.isNotBlank() }.map { it.ref.uppercase() }.toSet()
        val last = tokens.last()
        var dest: Dest = Dest.None
        var via = tokens
        val rk = runwayKey(last)
        if (rk != null && runwayEndKeys.contains(rk)) {
            dest = Dest.Runway(rk); via = tokens.dropLast(1)
        } else if (last !in twyRefs) {
            val spot = spots.firstOrNull { it.kind != SpotKind.HOLD && it.ref.equals(last, ignoreCase = true) }
                ?: return RouteResult.Error(loc("taxi.unknown", last))
            dest = Dest.Stand(spot); via = tokens.dropLast(1)
        }
        via.firstOrNull { it !in twyRefs }?.let { return RouteResult.Error(loc("taxi.unknown", it)) }
        if (via.isEmpty() && dest == Dest.None) return RouteResult.Error(loc("taxi.enter_route"))

        val from = LatLon(fromLat, fromLon)
        val start = nearest(from) { it.kind == LineKind.TAXIWAY || it.kind == LineKind.TAXILANE || it.kind == LineKind.RUNWAY }
        if (start == null || start.d > 500) return RouteResult.Error(loc("taxi.too_far"))
        val sl = lines[start.line]
        val sa = g.index[key(sl.pts[start.seg])] ?: return RouteResult.Error(loc("taxi.no_taxiways"))
        val sb = g.index[key(sl.pts[start.seg + 1])] ?: return RouteResult.Error(loc("taxi.no_taxiways"))

        val auto = via.isEmpty()
        val nL = via.size
        // Warstwy: 0 – dojazd do pierwszej drogi, 1..nL – kolejne drogi, nL+1 – dojazd z ostatniej drogi do stanowiska
        val layers = if (auto) 1 else nL + 2
        val n = g.nodes.size
        val dist = DoubleArray(layers * n) { Double.POSITIVE_INFINITY }
        val prev = IntArray(layers * n) { -1 }
        val prevLine = IntArray(layers * n) { -1 }
        val standNode = (dest as? Dest.Stand)?.let { s ->
            val p = LatLon(s.spot.lat, s.spot.lon)
            g.nodes.indices.filter { i -> g.nodeLines[i].any { lines[it].kind != LineKind.RUNWAY } }
                .minByOrNull { metersBetween(g.nodes[it], p) }
                ?.takeIf { metersBetween(g.nodes[it], p) < 300 }
                ?: return RouteResult.Error(loc("taxi.stand_unreachable", s.spot.ref))
        }

        fun touches(u: Int, ref: String) = g.nodeLines[u].any { lines[it].kind != LineKind.RUNWAY && lines[it].ref.equals(ref, ignoreCase = true) }
        fun touchesRunway(u: Int, k: String) = g.nodeLines[u].any { lines[it].kind == LineKind.RUNWAY && runwayEnds(lines[it].ref).contains(k) }
        fun cost(layer: Int, e: Edge): Double? {
            val l = lines[e.line]
            if (auto) return when (l.kind) {
                LineKind.RUNWAY -> e.m * 6
                LineKind.TAXIWAY, LineKind.TAXILANE -> e.m
                else -> null
            }
            return if (layer == 0 || layer == nL + 1) {
                when {
                    l.kind == LineKind.RUNWAY -> null
                    l.ref.isBlank() || l.kind == LineKind.TAXILANE -> e.m
                    else -> e.m * 3
                }
            } else if (l.kind != LineKind.RUNWAY && l.ref.equals(via[layer - 1], ignoreCase = true)) e.m else null
        }

        val heap = java.util.PriorityQueue<Pair<Double, Int>>(compareBy { it.first })
        fun relax(state: Int, d: Double, src: Int, line: Int) {
            if (d < dist[state]) {
                dist[state] = d; prev[state] = src; prevLine[state] = line
                heap.add(d to state)
            }
        }
        val dA = metersBetween(start.p, g.nodes[sa]); val dB = metersBetween(start.p, g.nodes[sb])
        relax(sa, dA, -1, start.line); relax(sb, dB, -1, start.line)
        var found = -1
        while (heap.isNotEmpty()) {
            val (d, s) = heap.poll()!!
            if (d > dist[s]) continue
            val layer = s / n; val u = s % n
            val done = when (val t = dest) {
                is Dest.Runway -> (auto || layer == nL) && touchesRunway(u, t.key)
                is Dest.Stand -> u == standNode && (auto || layer >= nL)
                Dest.None -> false
            }
            if (done) { found = s; break }
            if (!auto) {
                if (layer < nL && touches(u, via[layer])) relax((layer + 1) * n + u, d, s, -1)
                if (layer == nL && dest is Dest.Stand) relax((nL + 1) * n + u, d, s, -1)
            }
            for (e in g.adj[u]) {
                val c = cost(layer, e) ?: continue
                relax(layer * n + e.to, d + c, s, e.line)
            }
        }
        if (found < 0 && dest == Dest.None) {
            // Bez celu: do końca ostatniej drogi (najdalszy osiągalny punkt na niej)
            var best = -1
            for (u in 0 until n) {
                val s = nL * n + u
                if (dist[s].isFinite() && (best < 0 || dist[s] > dist[best])) best = s
            }
            found = best
        }
        if (found < 0) return RouteResult.Error(
            when (val t = dest) {
                is Dest.Runway -> loc("taxi.no_path_runway", t.key)
                is Dest.Stand -> loc("taxi.stand_unreachable", t.spot.ref)
                Dest.None -> loc("taxi.no_path")
            },
        )

        // Odtworzenie: węzły i linie od startu do celu
        val chain = ArrayList<Pair<Int, Int>>() // (węzeł, linia, którą do niego dojechano)
        var s = found
        while (s >= 0) {
            val u = s % n
            if (chain.isEmpty() || chain.last().first != u) chain += u to prevLine[s]
            else if (prevLine[s] >= 0) chain[chain.size - 1] = u to prevLine[s]
            s = prev[s]
        }
        chain.reverse()
        val pts = ArrayList<LatLon>()
        pts += start.p
        chain.forEach { pts += g.nodes[it.first] }
        (dest as? Dest.Stand)?.let { pts += LatLon(it.spot.lat, it.spot.lon) }
        val length = pts.zipWithNext { a, b -> metersBetween(a, b) }.sum()
        val viaOut = ArrayList<String>()
        chain.forEach { (_, li) ->
            if (li < 0) return@forEach
            val name = lines[li].takeIf { it.kind != LineKind.RUNWAY }?.ref?.takeIf { it.isNotBlank() } ?: return@forEach
            if (viaOut.lastOrNull() != name) viaOut += name
        }

        // Pasy po drodze: znacznik ok. 50 m przed osią pasa, po trasie
        fun backOff(idx: Int, m: Double): LatLon {
            var left = m
            var i = idx + 1 // indeks w pts (pts[0] = rzut startu)
            while (i > 0) {
                val a = pts[i]; val b = pts[i - 1]
                val seg = metersBetween(a, b)
                if (seg >= left && seg > 0) { val f = left / seg; return LatLon(a.lat + (b.lat - a.lat) * f, a.lon + (b.lon - a.lon) * f) }
                left -= seg; i--
            }
            return pts.first()
        }
        val crossings = ArrayList<RouteMark>()
        val endIdx = if (dest is Dest.Runway) chain.size - 1 else chain.size
        val startOnRunway = sl.kind == LineKind.RUNWAY
        for (i in chain.indices) {
            if (i >= endIdx) break
            val u = chain[i].first
            val rw = g.nodeLines[u].firstOrNull { lines[it].kind == LineKind.RUNWAY } ?: continue
            if (i == 0 && startOnRunway) continue
            val ref = lines[rw].ref
            val prevMark = crossings.lastOrNull()
            if (prevMark != null && prevMark.label == ref && metersBetween(LatLon(prevMark.lat, prevMark.lon), g.nodes[u]) < 250) continue
            val p = backOff(i, 50.0)
            crossings += RouteMark(p.lat, p.lon, ref, true)
        }
        val end = when (val t = dest) {
            is Dest.Runway -> backOff(chain.size - 1, 50.0).let { RouteMark(it.lat, it.lon, t.key, true) }
            is Dest.Stand -> RouteMark(t.spot.lat, t.spot.lon, t.spot.ref, false)
            Dest.None -> pts.last().let { RouteMark(it.lat, it.lon, viaOut.lastOrNull().orEmpty(), false) }
        }
        return RouteResult.Ok(TaxiRoute(pts, length, viaOut, crossings, end))
    }

    // ---------- położenie samolotu ----------

    /** Gdzie jest samolot: droga pod nim, następne skrzyżowanie w kierunku jazdy, pas i miejsce oczekiwania przed nim. */
    fun taxiInfo(lat: Double, lon: Double, hdgTrue: Double): TaxiInfo? {
        val g = graph
        if (g.nodes.isEmpty()) return null
        val p = LatLon(lat, lon)
        val rw = nearest(p) { it.kind == LineKind.RUNWAY }
        val onRunway = rw?.takeIf { it.d < ((lines[it.line].widthM ?: 45.0) / 2).coerceAtLeast(15.0) }
        val twy = nearest(p) { it.kind == LineKind.TAXIWAY || it.kind == LineKind.TAXILANE }
        val cur = onRunway ?: twy?.takeIf { it.d < 35 } ?: return TaxiInfo(null, null, null, null, null, null, holdAhead(p, hdgTrue))
        val ref = lines[cur.line].ref
        var li = cur.line
        var pts = lines[li].pts
        val a = pts[cur.seg]; val b = pts[cur.seg + 1]
        var forward = angleDiff(Geo.bearing(a.lat, a.lon, b.lat, b.lon), hdgTrue) <= 90
        var j = if (forward) cur.seg + 1 else cur.seg
        var d = metersBetween(cur.p, pts[j])
        var next: String? = null; var nextM: Double? = null
        var rwyAhead: String? = null; var rwyAheadM: Double? = null
        var hops = 0
        while (d < 2500) {
            val u = g.index[key(pts[j])]
            if (u != null && d > 3) {
                val others = g.nodeLines[u].filter { it != li && lines[it].ref.isNotBlank() && !lines[it].ref.equals(ref, ignoreCase = true) && lines[it].kind != LineKind.HOLD }
                if (others.isNotEmpty()) {
                    val rwOther = others.firstOrNull { lines[it].kind == LineKind.RUNWAY }
                    if (rwOther != null && onRunway == null && rwyAhead == null) { rwyAhead = lines[rwOther].ref; rwyAheadM = d }
                    if (next == null) { next = others.map { lineName(it) }.distinct().joinToString(" / "); nextM = d }
                    if (rwyAhead != null || d > 600) break
                }
            }
            var nj = if (forward) j + 1 else j - 1
            if (nj !in pts.indices) {
                // Koniec kawałka drogi w OSM: dalej ta sama droga (to samo oznaczenie) zaczynająca się w tym węźle
                if (u == null || ref.isBlank() || hops >= 8) break
                val here = pts[j]
                val cont = g.nodeLines[u].firstOrNull { it != li && lines[it].ref.equals(ref, ignoreCase = true) && (lines[it].pts.first() == here || lines[it].pts.last() == here) } ?: break
                hops++
                li = cont; pts = lines[li].pts
                forward = pts.first() == here
                j = if (forward) 0 else pts.size - 1
                nj = if (forward) 1 else pts.size - 2
                if (nj !in pts.indices) break
            }
            d += metersBetween(pts[j], pts[nj])
            j = nj
        }
        return TaxiInfo(
            on = if (onRunway != null) null else lineName(cur.line).takeIf { it.isNotBlank() },
            onRunway = onRunway?.let { lines[it.line].ref },
            next = next, nextM = nextM,
            runwayAhead = rwyAhead, runwayAheadM = rwyAheadM,
            holdAheadM = holdAhead(p, hdgTrue),
        )
    }

    /** Miejsce oczekiwania przed pasem w odległości do 80 m przed samolotem. */
    private fun holdAhead(p: LatLon, hdg: Double): Double? =
        spots.asSequence().filter { it.kind == SpotKind.HOLD }
            .map { it to metersBetween(p, LatLon(it.lat, it.lon)) }
            .filter { (s, d) -> d in 4.0..80.0 && angleDiff(Geo.bearing(p.lat, p.lon, s.lat, s.lon), hdg) < 50 }
            .minByOrNull { it.second }?.second

    companion object {
        /** Odpowiedź Overpass API (`out geom`) → mapa lotniska. */
        fun parse(ident: String, text: String): AirportLayout {
            val root = Json.parseToJsonElement(text).jsonObject
            val els = root["elements"] as? JsonArray ?: JsonArray(emptyList())
            val lines = ArrayList<LayoutLine>(); val areas = ArrayList<LayoutArea>(); val spots = ArrayList<LayoutSpot>()
            fun JsonObject.str(k: String) = this[k]?.jsonPrimitive?.contentOrNull
            fun geometry(o: JsonObject): List<LatLon> = (o["geometry"] as? JsonArray)?.mapNotNull { g ->
                val p = g as? JsonObject ?: return@mapNotNull null
                val la = p["lat"]?.jsonPrimitive?.doubleOrNull ?: return@mapNotNull null
                val lo = p["lon"]?.jsonPrimitive?.doubleOrNull ?: return@mapNotNull null
                LatLon(la, lo)
            }.orEmpty()
            fun width(t: JsonObject) = t.str("width")?.let { Regex("^\\s*([0-9]+(?:[.,][0-9]+)?)").find(it)?.groupValues?.get(1)?.replace(',', '.')?.toDoubleOrNull() }
            fun mid(pts: List<LatLon>) = LatLon(pts.sumOf { it.lat } / pts.size, pts.sumOf { it.lon } / pts.size)
            fun areaKind(a: String) = when (a) {
                "apron" -> AreaKind.APRON; "terminal" -> AreaKind.TERMINAL; "hangar" -> AreaKind.HANGAR; else -> null
            }
            for (e in els) {
                val o = e as? JsonObject ?: continue
                val tags = o["tags"] as? JsonObject ?: continue
                val aeroway = tags.str("aeroway") ?: continue
                val ref = (tags.str("ref") ?: "").trim()
                val name = tags.str("name") ?: ""
                when (o.str("type")) {
                    "node" -> {
                        val la = o["lat"]?.jsonPrimitive?.doubleOrNull ?: continue
                        val lo = o["lon"]?.jsonPrimitive?.doubleOrNull ?: continue
                        when (aeroway) {
                            "holding_position" -> spots.add(LayoutSpot(SpotKind.HOLD, ref, la, lo))
                            "parking_position" -> spots.add(LayoutSpot(SpotKind.STAND, ref.ifBlank { name }, la, lo))
                            "gate" -> spots.add(LayoutSpot(SpotKind.GATE, ref.ifBlank { name }, la, lo))
                        }
                    }
                    "way" -> {
                        val pts = geometry(o)
                        if (pts.size < 2) continue
                        val closed = pts.size >= 4 && pts.first() == pts.last()
                        val area = closed && tags.str("area") == "yes"
                        val m = mid(pts)
                        val ak = areaKind(aeroway)
                        when {
                            aeroway == "runway" && area -> areas.add(LayoutArea(AreaKind.RUNWAY, ref, pts))
                            aeroway == "runway" -> lines.add(LayoutLine(LineKind.RUNWAY, ref, pts, width(tags)))
                            aeroway == "taxiway" && area -> areas.add(LayoutArea(AreaKind.APRON, "", pts))
                            aeroway == "taxiway" -> lines.add(LayoutLine(LineKind.TAXIWAY, ref, pts, width(tags)))
                            aeroway == "taxilane" -> lines.add(LayoutLine(LineKind.TAXILANE, ref, pts, width(tags)))
                            aeroway == "holding_position" -> {
                                lines.add(LayoutLine(LineKind.HOLD, ref, pts, null))
                                spots.add(LayoutSpot(SpotKind.HOLD, ref, m.lat, m.lon))
                            }
                            aeroway == "parking_position" -> spots.add(LayoutSpot(SpotKind.STAND, ref.ifBlank { name }, m.lat, m.lon))
                            ak != null && closed -> areas.add(LayoutArea(ak, name, pts))
                        }
                    }
                    "relation" -> {
                        val k = areaKind(aeroway) ?: continue
                        val parts = (o["members"] as? JsonArray)?.mapNotNull { m ->
                            val mo = m as? JsonObject ?: return@mapNotNull null
                            if (mo.str("type") != "way" || (mo.str("role") ?: "outer") == "inner") return@mapNotNull null
                            geometry(mo).takeIf { it.size >= 2 }
                        }.orEmpty()
                        for (ring in joinRings(parts)) areas.add(LayoutArea(k, name, ring))
                    }
                }
            }
            return AirportLayout(ident, lines, areas, spots)
        }

        /** Łączy kawałki obrysu wielokąta (drogi relacji) w zamknięte pierścienie. */
        internal fun joinRings(parts: List<List<LatLon>>): List<List<LatLon>> {
            val rings = ArrayList<List<LatLon>>()
            val open = ArrayList<List<LatLon>>()
            for (p in parts) {
                if (p.size >= 4 && p.first() == p.last()) rings.add(p) else open.add(p)
            }
            while (open.isNotEmpty()) {
                val cur = ArrayList(open.removeAt(0))
                var grew = true
                while (grew && cur.first() != cur.last()) {
                    grew = false
                    val i = open.indexOfFirst { it.first() == cur.last() || it.last() == cur.last() }
                    if (i >= 0) {
                        val p = open.removeAt(i)
                        cur += (if (p.first() == cur.last()) p else p.reversed()).drop(1)
                        grew = true
                    }
                }
                if (cur.size >= 4 && cur.first() == cur.last()) rings += cur
            }
            return rings
        }

        /** Zapytanie Overpass API o obiekty lotniska w prostokącie (s, w, n, e). */
        fun query(s: Double, w: Double, n: Double, e: Double): String {
            val b = String.format(java.util.Locale.US, "%.5f,%.5f,%.5f,%.5f", s, w, n, e)
            return "[out:json][timeout:40];(" +
                "way[\"aeroway\"~\"^(runway|taxiway|taxilane|apron|terminal|hangar|holding_position|parking_position)$\"]($b);" +
                "node[\"aeroway\"~\"^(holding_position|parking_position|gate)$\"]($b);" +
                "relation[\"aeroway\"~\"^(apron|terminal|hangar)$\"]($b);" +
                ");out geom qt;"
        }
    }
}

/** Pozostała długość trasy od samolotu (m) albo null, gdy samolot jest daleko od niej. */
fun TaxiRoute.remainingFrom(lat: Double, lon: Double): Double? {
    if (pts.size < 2) return null
    val p = LatLon(lat, lon)
    var bestI = -1; var bestD = Double.MAX_VALUE; var bestQ = p
    val k = cos(Math.toRadians(lat))
    for (i in 0 until pts.size - 1) {
        val a = pts[i]; val b = pts[i + 1]
        val dx = (b.lon - a.lon) * k; val dy = b.lat - a.lat
        val len2 = dx * dx + dy * dy
        val t = if (len2 <= 0) 0.0 else ((((p.lon - a.lon) * k) * dx + (p.lat - a.lat) * dy) / len2).coerceIn(0.0, 1.0)
        val q = LatLon(a.lat + (b.lat - a.lat) * t, a.lon + (b.lon - a.lon) * t)
        val d = metersBetween(q, p)
        if (d < bestD) { bestD = d; bestI = i; bestQ = q }
    }
    if (bestI < 0 || bestD > 120) return null
    var rest = metersBetween(bestQ, pts[bestI + 1])
    for (i in bestI + 1 until pts.size - 1) rest += metersBetween(pts[i], pts[i + 1])
    return rest
}
