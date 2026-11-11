package com.freemetar.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.util.Locale

/**
 * Języki aplikacji. Teksty leżą w plikach `src/main/resources/lang/<kod>.json` (klucz → tekst, `{0}` = argument),
 * a kod odwołuje się do nich przez `loc("klucz", argumenty)`, tak samo w interfejsie i w logice.
 * Nowy język: dodaj wartość tutaj, plik JSON i pozycję w ustawieniach.
 */
enum class AppLang(val code: String, val locale: Locale) {
    PL("pl", Locale("pl", "PL")),
    EN("en", Locale.UK),
    ES("es", Locale("es", "ES")),
}

object Lang {
    /** Domyślnie polski (testy JVM); aplikacja ustawia język przy starcie z ustawień lub systemu. */
    @Volatile
    var current: AppLang = AppLang.PL

    /** Locale do formatowania dat i liczb w bieżącym języku. */
    val locale: Locale get() = current.locale

    /** Ustawia język aplikacji razem z domyślnym Locale, żeby liczby i daty formatowały się w tym języku. */
    fun activate(lang: AppLang) {
        current = lang
        Locale.setDefault(locale)
    }

    /** Język wynikający z ustawienia: kod języka albo SYSTEM (język systemu, a gdy go nie ma w aplikacji — angielski). */
    fun resolve(setting: String?, system: Locale = Locale.getDefault()): AppLang =
        AppLang.entries.firstOrNull { it.name == setting }
            ?: AppLang.entries.firstOrNull { it.code == system.language }
            ?: AppLang.EN
}

/** Pliki językowe, wczytywane raz przy pierwszym użyciu. */
object Strings {
    private val cache = java.util.concurrent.ConcurrentHashMap<AppLang, Map<String, String>>()

    fun of(lang: AppLang): Map<String, String> = cache.getOrPut(lang) {
        val stream = Strings::class.java.classLoader!!.getResourceAsStream("lang/${lang.code}.json")
            ?: error("Brak pliku języka ${lang.code}")
        val text = stream.bufferedReader(Charsets.UTF_8).use { it.readText() }
        Json.parseToJsonElement(text).jsonObject.mapValues { it.value.jsonPrimitive.content }
    }

    /** Tekst dla klucza; brak w danym języku → angielski, brak także tam → sam klucz. */
    fun get(lang: AppLang, key: String, args: Array<out Any?>): String {
        var text = of(lang)[key] ?: of(AppLang.EN)[key] ?: return key
        args.forEachIndexed { i, arg -> text = text.replace("{$i}", arg.toString()) }
        return text
    }
}

/** Tekst w bieżącym języku. */
fun loc(key: String, vararg args: Any?): String = Strings.get(Lang.current, key, args)

/** Treść z danych (np. checklisty), gdzie tłumaczenia są polami obiektu. */
fun pick(pl: String, en: String, es: String = en): String = when (Lang.current) {
    AppLang.PL -> pl
    AppLang.EN -> en
    AppLang.ES -> es
}

/** Forma słowa `plural.<name>.one|few|many` dla liczby [n]; polski ma trzy formy: 1 lot, 2 loty, 5 lotów. */
fun plural(n: Number, name: String): String {
    val v = kotlin.math.abs(n.toLong())
    val form = when {
        v == 1L -> "one"
        Lang.current == AppLang.PL && v % 10 in 2..4 && v % 100 !in 12..14 -> "few"
        else -> "many"
    }
    return loc("plural.$name.$form")
}

fun flightCount(n: Number) = "$n " + plural(n, "flights")
fun airportCount(n: Number) = "$n " + plural(n, "airports")
fun pageCount(n: Number) = "$n " + plural(n, "pages")
fun visitCount(n: Number) = "$n " + plural(n, "visits")
fun itemCount(n: Number) = "$n " + plural(n, "items")

/** Dopełniacz po „z N”: 1 z 1 kryterium, 2 z 5 kryteriów. */
fun criteriaWord(n: Number) = plural(n, "criteria")
