package com.freemetar.app.ui.theme

import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import com.freemetar.app.domain.AppLang
import com.freemetar.app.domain.Strings

/**
 * Język w drzewie Compose — zmiana w ustawieniach przebudowuje cały interfejs.
 * Teksty w komponentach: `tr("klucz", argumenty)`.
 */
val LocalLang = staticCompositionLocalOf { AppLang.PL }

@Composable
fun tr(key: String, vararg args: Any?): String = Strings.get(LocalLang.current, key, args)

/** Podpis autora — w ustawieniach, na ekranie startowym i w README. */
const val AUTHOR_SITE = "redzikowski.com"
const val AUTHOR_URL = "https://redzikowski.com"
const val COFFEE_URL = "https://buycoffee.to/adamredzikowski"
