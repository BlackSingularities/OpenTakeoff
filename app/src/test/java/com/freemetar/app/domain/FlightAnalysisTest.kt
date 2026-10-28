package com.freemetar.app.domain

import com.freemetar.app.data.link.FlightArchive
import com.freemetar.app.data.link.FlightLog
import com.freemetar.app.data.link.FlightMarker
import com.freemetar.app.data.link.TrackPoint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FlightAnalysisTest {
    private val start = 1_700_000_000_000L

    /**
     * 20 min: start, przelot na 3000 ft, prosta do lądowania 3° z prędkością 120 kt (≈ 640 ft/min).
     * [wobble] — przechylenie zmieniane co sekundę o tyle stopni (nerwowe sterowanie).
     */
    private fun flight(id: String = "a", t0: Long = start, wobble: Float = 0f, steepFinal: Boolean = false): FlightLog {
        val pts = ArrayList<TrackPoint>()
        val final = 300 // ostatnie 300 s to podejście z 3000 ft
        val total = 1200
        for (i in 0..total) {
            val toGo = total - i
            val alt = when {
                i < 300 -> i * 10
                toGo > final -> 3000
                steepFinal -> 3000 * toGo / final
                else -> 3000 * toGo / final
            }
            val vs = when { i < 300 -> 600; toGo > final -> 0; else -> -600 }
            // Prosta: 120 kt → 2 NM/min; podejście 10 NM ≈ 5 min, ścieżka ≈ 3°. Stroma: ten sam spadek na połowie odległości.
            val nmPerS = if (steepFinal && toGo <= final) 1.0 / 60 else 2.0 / 60
            val lat = 52.0 + (if (toGo > final) (i - 0) * 2.0 / 60 / 60 else ((total - final) * 2.0 / 60 + (i - (total - final)) * nmPerS) / 60)
            val bank = if (wobble > 0f) (if (i % 2 == 0) wobble else -wobble) else 0f
            pts += TrackPoint(t0 + i * 1000L, lat, 20.0, alt, 120, vs, 120, 0, bank, 2f, alt, 1.0f, i == total)
        }
        val td = LandingScore.evaluate(TouchdownData(vsFpm = -150.0, alongFt = 1200.0)).copy(at = t0 + total * 1000L, airport = "EPKK")
        return FlightLog(id = id, start = t0, end = t0 + total * 1000L, aircraft = "C172", from = "EPWA", to = "EPKK", maxAltFt = 3000, distanceNm = 40, track = pts, landing = td, landings = listOf(td))
    }

    @Test
    fun stableApproachOnThreeDegreePath() {
        val a = FlightAnalysis.analyze(flight())
        val ap = a.approaches.single()
        assertEquals(1000, ap.stableFromFt)
        assertTrue("kąt ${ap.pathAngleDeg}", ap.pathAngleDeg!! in 2.5..3.5)
        assertTrue(ap.score >= 90)
        assertEquals(listOf(1000, 500), ap.gates.map { it.heightFt })
    }

    @Test
    fun steepFinalLosesPoints() {
        val steep = FlightAnalysis.analyze(flight(steepFinal = true)).approaches.single()
        assertTrue("kąt ${steep.pathAngleDeg}", steep.pathAngleDeg!! > 4)
        assertTrue(steep.score < 95)
    }

    @Test
    fun nervousBankIsLessSmooth() {
        val calm = FlightAnalysis.smoothness(flight().track)!!
        val rough = FlightAnalysis.smoothness(flight(wobble = 8f).track)!!
        assertTrue("${calm.score} vs ${rough.score}", calm.score > rough.score + 20)
        assertTrue(rough.jerks > 0)
    }

    @Test
    fun sparseTrackHasNoSmoothness() {
        val f = flight()
        assertNull(FlightAnalysis.smoothness(f.track.filterIndexed { i, _ -> i % 5 == 0 }))
    }

    @Test
    fun eventsIncludeMarkersAndLandingInOrder() {
        val f = flight().copy(markers = listOf(FlightMarker(start + 600_000, "klapy")))
        val ev = FlightAnalysis.events(f)
        assertEquals(EventKind.TAKEOFF, ev.first().kind)
        assertTrue(ev.any { it.kind == EventKind.MARKER && it.label == "klapy" })
        assertEquals(EventKind.LANDING, ev.last().kind)
        assertEquals(ev.map { it.t }, ev.map { it.t }.sorted())
    }

    @Test
    fun scenarioAndComparisonAndReport() {
        val before = flight("old", start - 86_400_000, wobble = 8f)
        val now = flight("new")
        val a = FlightAnalysis.analyze(now)
        val sc = FlightAnalysis.scenario(now, Scenario.CROSS_COUNTRY, a)
        assertEquals(sc.judged, sc.passed)
        val cmp = FlightAnalysis.compare(now, listOf(now, before), a)
        assertNotNull(cmp)
        assertTrue(cmp!!.sameRoute)
        assertTrue(cmp.rows.any { it.better == true })
        assertNull(FlightAnalysis.compare(before, listOf(now, before)))
        val report = FlightAnalysis.instructorReport(now, a, sc, cmp)
        assertTrue(report.contains("EPWA → EPKK"))
        assertTrue(report.contains("MOCNE STRONY"))
    }

    @Test
    fun archiveKeepsAnalysisColumnsAndMarkers() {
        val f = flight(wobble = 3f).copy(markers = listOf(FlightMarker(start + 1000, "x")), scenario = "pattern")
        val back = FlightArchive.decode(FlightArchive.encode(listOf(f))).single()
        assertEquals(f.markers, back.markers)
        assertEquals("pattern", back.scenario)
        f.track.zip(back.track).forEach { (a, b) ->
            assertEquals(a.bankDeg, b.bankDeg); assertEquals(a.aglFt, b.aglFt); assertEquals(a.onGround, b.onGround); assertEquals(a.gLoad, b.gLoad)
        }
    }
}
