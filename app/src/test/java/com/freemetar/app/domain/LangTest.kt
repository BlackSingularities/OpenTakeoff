package com.freemetar.app.domain

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Locale

class LangTest {
    private val savedLocale = Locale.getDefault()

    @After
    fun reset() {
        Lang.current = AppLang.PL
        Locale.setDefault(savedLocale)
    }

    @Test
    fun `Polish plural forms`() {
        Lang.current = AppLang.PL
        val cases = mapOf(1 to "1 lot", 2 to "2 loty", 4 to "4 loty", 5 to "5 lotów", 12 to "12 lotów", 22 to "22 loty", 112 to "112 lotów", 0 to "0 lotów")
        cases.forEach { (n, text) -> assertEquals(text, flightCount(n)) }
    }

    @Test
    fun `English and Spanish plural forms`() {
        Lang.current = AppLang.EN
        assertEquals("1 flight", flightCount(1))
        assertEquals("0 flights", flightCount(0))
        Lang.current = AppLang.ES
        assertEquals("1 vuelo", flightCount(1))
        assertEquals("3 vuelos", flightCount(3))
    }

    @Test
    fun `numbers follow the app language, grouping stays a space`() {
        Lang.activate(AppLang.EN)
        assertEquals("1.5", 1.5.fmt(1))
        assertEquals("12 000", 12000.grouped())
        Lang.activate(AppLang.ES)
        assertEquals("1,5", 1.5.fmt(1))
        assertEquals("12 000", 12000.grouped())
        Lang.activate(AppLang.PL)
        assertEquals("1,5", 1.5.fmt(1))
    }

    @Test
    fun `system language picks a supported language or English`() {
        assertEquals(AppLang.ES, Lang.resolve("SYSTEM", Locale("es", "ES")))
        assertEquals(AppLang.PL, Lang.resolve("SYSTEM", Locale("pl", "PL")))
        assertEquals(AppLang.EN, Lang.resolve("SYSTEM", Locale.GERMANY))
        assertEquals(AppLang.ES, Lang.resolve("ES", Locale.UK))
    }

    @Test
    fun `texts come from the language files`() {
        Lang.current = AppLang.ES
        assertEquals("Cancelar", loc("common.cancel"))
        Lang.current = AppLang.PL
        assertEquals("Anuluj", loc("common.cancel"))
        assertEquals("no.such.key", loc("no.such.key"))
    }

    @Test
    fun `every language has every key and no empty text`() {
        val english = Strings.of(AppLang.EN)
        AppLang.entries.forEach { lang ->
            val texts = Strings.of(lang)
            assertEquals(lang.code, english.keys, texts.keys)
            assertEquals(lang.code, emptyList<String>(), texts.filterValues { it.isBlank() }.keys.toList())
        }
    }

    @Test
    fun `Spanish weather keeps adjectives after the noun`() {
        Lang.current = AppLang.ES
        assertEquals("Lluvia engelante débil", MetarDecoder.weatherText("-FZRA"))
        assertEquals("Chubascos de nieve fuerte", MetarDecoder.weatherText("+SHSN"))
        assertEquals("Tormenta con lluvia", MetarDecoder.weatherText("TSRA"))
        assertEquals("Tormenta", MetarDecoder.weatherText("TS"))
    }

    @Test
    fun `Polish and English weather order is unchanged`() {
        Lang.current = AppLang.PL
        assertEquals("Słaby przelotny deszcz", MetarDecoder.weatherText("-SHRA"))
        Lang.current = AppLang.EN
        assertEquals("Light showers of rain", MetarDecoder.weatherText("-SHRA"))
        assertEquals("Thunderstorm", MetarDecoder.weatherText("TS"))
    }
}
