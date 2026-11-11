package com.freemetar.app.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Email
import androidx.compose.material.icons.outlined.Build
import androidx.compose.material.icons.outlined.LocalCafe
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.freemetar.app.BuildConfig
import com.freemetar.app.MainViewModel
import com.freemetar.app.R
import com.freemetar.app.UpdateState
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.theme.AUTHOR_SITE
import com.freemetar.app.ui.theme.AUTHOR_URL
import com.freemetar.app.ui.theme.COFFEE_URL
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono

/**
 * „O aplikacji” w ustawieniach: nazwa i wersja ze stanem aktualizacji w jednym miejscu,
 * pod spodem odnośniki — wsparcie (kawa) i autor.
 */
@Composable
fun AboutSection(vm: MainViewModel) {
    val c = Fm.c
    val state by vm.update.collectAsStateWithLifecycle()
    val install = rememberInstaller()
    val uri = LocalUriHandler.current
    SectionCard(loc("about_section.about"), icon = Icons.Outlined.Info, dense = true) {
        // Nagłówek: znak, nazwa i wersja
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(46.dp).clip(RoundedCornerShape(12.dp)).background(c.terrain), contentAlignment = Alignment.Center) {
                Image(painterResource(R.drawable.opentakeoff_mark), null, Modifier.size(32.dp))
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text("OpenTakeoff", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold, color = c.ink)
                Text(loc("about_section.version") + BuildConfig.VERSION_NAME, fontFamily = Mono, fontSize = 12.sp, color = c.ink2)
            }
        }
        Text(
            loc("about_section.a_free_app_for_flight_sim"),
            style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 8.dp),
        )

        // Aktualizacje: stan i jedno działanie, które ma teraz sens
        Column(Modifier.padding(top = 10.dp).fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(c.terrain).padding(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    when (val s = state) {
                        UpdateState.Idle -> loc("about_section.updates_are_checked_at_startup")
                        UpdateState.Checking -> loc("about_section.checking_for_updates")
                        UpdateState.UpToDate -> loc("about_section.you_have_the_latest_version")
                        is UpdateState.Available -> loc("about_section.version_is_available", s.info.versionName) + sizeMb(s.info.sizeBytes)
                        is UpdateState.Downloading -> loc("common.downloading", s.info.versionName)
                        is UpdateState.Ready -> loc("about_section.version_downloaded", s.info.versionName)
                        is UpdateState.Error -> s.message
                    },
                    color = if (state is UpdateState.Error) c.ifr else c.ink, fontSize = 14.sp, modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                when (val s = state) {
                    is UpdateState.Available -> Button({ vm.downloadUpdate(s.info) }) { Text(loc("common.install")) }
                    is UpdateState.Ready -> Button({ install(s.file) }) { Text(loc("common.install")) }
                    is UpdateState.Downloading, UpdateState.Checking -> {}
                    else -> OutlinedButton({ vm.checkForUpdate() }) { Text(loc("about_section.check")) }
                }
            }
            (state as? UpdateState.Downloading)?.let { s ->
                LinearProgressIndicator(progress = { s.progress }, modifier = Modifier.fillMaxWidth().padding(top = 8.dp))
            }
            Text(
                loc("about_section.the_app_checks_for_new_versions"),
                style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 6.dp),
            )
        }

        // Odnośniki
        Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            LinkRow(Icons.Outlined.LocalCafe, loc("about_section.buy_me_a_coffee"),
                loc("about_section.support_the_app_s_development_buycoffee"), highlight = true) { uri.openUri(COFFEE_URL) }
            LinkRow(Icons.Outlined.Person, loc("about_section.idea_and_development"), "Adam Rędzikowski · $AUTHOR_SITE") { uri.openUri(AUTHOR_URL) }
            LinkRow(Icons.Outlined.Email, loc("about_section.contact"), "kontakt@redzikowski.com") { uri.openUri("mailto:kontakt@redzikowski.com") }
            com.freemetar.app.feedback.FeedbackEntry { open ->
                LinkRow(Icons.Outlined.Build, loc("common.report_a_bug_or_suggestion"), loc("common.goes_straight_to_the_developer")) { open() }
            }
        }
    }
}

/** Wiersz odnośnika: ikona w kółku, tytuł i opis, strzałka „otwórz”; wyróżniony — tło akcentu i obwódka. */
@Composable
private fun LinkRow(icon: ImageVector, title: String, sub: String, highlight: Boolean = false, onClick: () -> Unit) {
    val c = Fm.c
    val shape = RoundedCornerShape(10.dp)
    Row(
        Modifier.fillMaxWidth().clip(shape)
            .background(if (highlight) c.accent.copy(alpha = 0.12f) else Color.Transparent)
            .border(1.dp, if (highlight) c.accent.copy(alpha = 0.6f) else c.rule, shape)
            .clickable(role = Role.Button, onClick = onClick).padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(34.dp).clip(CircleShape).background(if (highlight) c.accent.copy(alpha = 0.2f) else c.terrain), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = if (highlight) c.accent else c.ink, modifier = Modifier.size(19.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = c.ink, fontSize = 15.sp, fontWeight = if (highlight) FontWeight.Bold else FontWeight.Normal)
            Text(sub, color = c.ink2, fontSize = 12.sp, maxLines = 1)
        }
        Spacer(Modifier.height(1.dp).width(8.dp))
        Icon(Icons.AutoMirrored.Outlined.OpenInNew, null, tint = c.ink2, modifier = Modifier.size(16.dp))
    }
}
