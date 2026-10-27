package com.freemetar.app.domain

import com.freemetar.app.data.link.AircraftInfo
import com.freemetar.app.data.link.Attitude
import com.freemetar.app.data.link.FlightRecorder
import com.freemetar.app.data.link.FlightState
import com.freemetar.app.data.link.Position
import com.freemetar.app.data.link.Speed
import com.freemetar.app.data.link.Touchdown
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LandingTest {
    // EPGD RWY 29: próg wschodni, kurs rzeczywisty ok. 292°
    private val rw29 = RunwayGeom("EPGD 29", 54.3770, 18.4880, 292.0, 9186.0)
    private val rw11 = RunwayGeom("EPGD 11", 54.3840, 18.4370, 112.0, 9186.0)

    @Test
    fun offsetAlongAndAcrossRunway() {
        // 1000 ft wzdłuż kursu 292° od progu
        val d = 1000.0 / 364_000.0
        val lat = rw29.thresholdLat + d * kotlin.math.cos(Math.toRadians(292.0))
        val lon = rw29.thresholdLon + d * kotlin.math.sin(Math.toRadians(292.0)) / kotlin.math.cos(Math.toRadians(rw29.thresholdLat))
        val o = RunwayMath.offset(rw29, lat, lon)
        assertEquals(1000.0, o.alongFt, 5.0)
        assertEquals(0.0, o.crossFt, 5.0)
    }

    @Test
    fun picksRunwayByHeading() {
        val d = 1500.0 / 364_000.0
        val lat = rw29.thresholdLat + d * kotlin.math.cos(Math.toRadians(292.0))
        val lon = rw29.thresholdLon + d * kotlin.math.sin(Math.toRadians(292.0)) / kotlin.math.cos(Math.toRadians(54.38))
        assertEquals("EPGD 29", RunwayMath.pick(listOf(rw11, rw29), lat, lon, 290.0)?.ident)
        assertNull(RunwayMath.pick(listOf(rw29), lat, lon, 200.0))
    }

    @Test
    fun butterLandingScoresHigh() {
        val r = LandingScore.evaluate(TouchdownData(vsFpm = -140.0, gForce = 1.12, bankDeg = 0.8, pitchDeg = 4.0, alongFt = 1200.0, crossFt = 4.0))
        assertTrue(r.score >= 95)
        assertEquals("Doskonałe", r.grade)
        assertEquals(-140, r.vsFpm)
    }

    @Test
    fun hardShortLandingScoresLow() {
        val r = LandingScore.evaluate(TouchdownData(vsFpm = -720.0, gForce = 2.3, bankDeg = 9.0, pitchDeg = -1.0, alongFt = -200.0, crossFt = 60.0, bounces = 1))
        assertEquals(0, r.score)
        assertEquals("Bardzo twarde", r.grade)
        assertTrue(r.notes.any { it.contains("przed progiem") })
        assertTrue(r.notes.any { it.contains("Odbicia") })
    }

    @Test
    fun longLandingPenalised() {
        val r = LandingScore.evaluate(TouchdownData(vsFpm = -180.0, alongFt = 5000.0, runwayLengthFt = 9000.0))
        assertEquals(75, r.score)
        assertTrue(r.notes.any { it.contains("zostało 4000 ft") })
    }

    @Test
    fun savedLandingShownInCurrentLanguage() {
        // Zapisane po polsku, wyświetlane po przełączeniu na angielski
        val r = LandingScore.evaluate(TouchdownData(vsFpm = -180.0, alongFt = 5000.0, runwayLengthFt = 9000.0, bounces = 1))
        try {
            Lang.current = AppLang.EN
            assertEquals("Fair", r.gradeText)
            assertTrue(r.noteTexts.any { it.contains("4000 ft of runway remaining") })
            assertTrue(r.noteTexts.any { it.startsWith("Bounces") })
            assertTrue(r.noteTexts.none { it.contains("Odbicia") })
        } finally {
            Lang.current = AppLang.PL
        }
    }

    private fun st(lat: Double, lon: Double, gs: Double, ground: Boolean, vs: Double = 0.0, td: Touchdown? = null) = FlightState(
        aircraft = AircraftInfo("A320", ground, false, category = "airliner"),
        position = Position(lat, lon, if (ground) 490.0 else 3000.0),
        speed = Speed(iasKt = gs, gsKt = gs, vsFpm = vs),
        attitude = Attitude(pitchDeg = 4.0, bankDeg = 1.0, hdgTrueDeg = 292.0),
        touchdown = td,
    )

    @Test
    fun recorderScoresTouchdownFromLinkAndCountsBounce() {
        val r = FlightRecorder(runwaysNear = { _, _ -> listOf(rw29, rw11) }, nearestAirport = { _, _ -> "EPGD" })
        var t = 0L
        r.onState(st(54.30, 18.80, 140.0, false, -700.0), t)
        repeat(30) { t += 5000; r.onState(st(54.30 + it * 0.002, 18.80 - it * 0.01, 140.0, false, -700.0), t) }
        // Przyziemienie 1500 ft za progiem 29
        val d = 1500.0 / 364_000.0
        val lat = rw29.thresholdLat + d * kotlin.math.cos(Math.toRadians(292.0))
        val lon = rw29.thresholdLon + d * kotlin.math.sin(Math.toRadians(292.0)) / kotlin.math.cos(Math.toRadians(54.38))
        // Ostatni odcinek podejścia (~2 NM) trwa realnie ok. minuty — krótszy czas rejestrator uznałby za restart lotu
        t += 60_000
        r.onState(st(lat, lon, 130.0, true, td = Touchdown(seq = 1, vsFpm = -150.0, gForce = 1.15, pitchDeg = 4.0, bankDeg = 1.0, headingTrueDeg = 292.0, lat = lat, lon = lon, iasKt = 132.0)), t)
        val first = r.lastLanding
        assertNotNull(first)
        assertEquals("EPGD 29", first!!.runway)
        assertEquals(1500.0, first.alongFt!!.toDouble(), 30.0)
        // Odbicie w ciągu 15 s
        t += 3000
        r.onState(st(lat, lon, 120.0, true, td = Touchdown(seq = 2, vsFpm = -90.0, lat = lat, lon = lon)), t)
        // Postój i koniec lotu
        t += 5000; r.onState(st(lat, lon, 2.0, true), t)
        t += 61_000; val log = r.onState(st(lat, lon, 0.0, true), t)
        assertNotNull(log)
        assertEquals(1, log!!.landing!!.bounces)
        assertEquals("airliner", log.category)
        assertTrue(log.landing!!.score < first.score)
    }

    @Test
    fun recorderFallsBackToGroundTransition() {
        val r = FlightRecorder(nearestAirport = { _, _ -> null })
        var t = 0L
        r.onState(st(52.0, 20.0, 100.0, false, -300.0), t)
        repeat(20) { t += 5000; r.onState(st(52.0 + it * 0.01, 20.0, 100.0, false, -300.0), t) }
        t += 1000; r.onState(st(52.2, 20.0, 90.0, true), t)
        assertEquals(-300, r.lastLanding!!.vsFpm)
    }

    @Test
    fun recorderScoresEachTouchAndGoSeparately() {
        val r = FlightRecorder(nearestAirport = { lat, _ -> if (lat < 52.15) "EPWA" else "EPMO" })
        var t = 0L
        var lat = 52.0
        fun fly(n: Int) = repeat(n) { t += 5000; lat += 0.005; r.onState(st(lat, 20.0, 100.0, false, -300.0), t) }
        r.onState(st(lat, 20.0, 100.0, false, -300.0), t)
        fly(20)
        // Touch and go: przyziemienie i od razu start
        t += 1000; r.onState(st(lat, 20.0, 90.0, true, td = Touchdown(seq = 1, vsFpm = -150.0, lat = lat, lon = 20.0)), t)
        val first = r.lastLanding!!
        assertEquals("EPWA", first.airport)
        fly(30)
        t += 1000; r.onState(st(lat, 20.0, 90.0, true, td = Touchdown(seq = 2, vsFpm = -550.0, lat = lat, lon = 20.0)), t)
        assertTrue(r.lastLanding !== first)
        t += 5000; r.onState(st(lat, 20.0, 2.0, true), t)
        t += 61_000; val log = r.onState(st(lat, 20.0, 0.0, true), t)!!
        assertEquals(2, log.landings.size)
        assertEquals(listOf(-150, -550), log.landings.map { it.vsFpm })
        assertEquals(listOf("EPWA", "EPMO"), log.landings.map { it.airport })
        assertEquals(0, log.landings[0].bounces)
        assertEquals(-550, log.landing!!.vsFpm)
    }
}
