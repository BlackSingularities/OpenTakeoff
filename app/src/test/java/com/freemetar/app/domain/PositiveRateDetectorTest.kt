package com.freemetar.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PositiveRateDetectorTest {
    @Test
    fun `fires once after a takeoff roll, also at light aircraft speeds`() {
        val d = PositiveRateDetector()
        listOf(0.0, 20.0, 40.0, 55.0).forEach { assertFalse(d.update(true, it, 0.0)) }
        assertFalse(d.update(false, 58.0, 50.0))
        assertTrue(d.update(false, 60.0, 300.0))
        assertFalse(d.update(false, 62.0, 500.0))
    }

    @Test
    fun `landing bounce does not fire`() {
        val d = PositiveRateDetector()
        assertFalse(d.update(false, 130.0, -700.0))
        listOf(125.0, 120.0).forEach { assertFalse(d.update(true, it, -100.0)) }
        assertFalse(d.update(false, 118.0, 300.0))
    }

    @Test
    fun `bounce after a flight without a positive rate does not fire`() {
        val d = PositiveRateDetector()
        listOf(0.0, 60.0).forEach { d.update(true, it, 0.0) }
        assertFalse(d.update(false, 100.0, -500.0))
        d.update(true, 95.0, -100.0)
        assertFalse(d.update(false, 93.0, 300.0))
    }

    @Test
    fun `fires again on the next takeoff`() {
        val d = PositiveRateDetector()
        val results = (1..2).map {
            listOf(0.0, 60.0).forEach { ias -> d.update(true, ias, 0.0) }
            d.update(false, 65.0, 400.0)
        }
        assertEquals(listOf(true, true), results)
    }
}
