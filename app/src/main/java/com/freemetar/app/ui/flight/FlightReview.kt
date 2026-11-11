package com.freemetar.app.ui.flight

import com.freemetar.app.domain.criteriaWord
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Assignment
import androidx.compose.material.icons.outlined.CompareArrows
import androidx.compose.material.icons.outlined.FlightLand
import androidx.compose.material.icons.outlined.School
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material.icons.outlined.Waves
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.freemetar.app.domain.ApproachAnalysis
import com.freemetar.app.domain.Comparison
import com.freemetar.app.domain.Scenario
import com.freemetar.app.domain.ScenarioResult
import com.freemetar.app.domain.Smoothness
import com.freemetar.app.domain.loc
import com.freemetar.app.ui.components.Fact
import com.freemetar.app.ui.components.FactGrid
import com.freemetar.app.ui.components.Section
import com.freemetar.app.ui.theme.Fm
import com.freemetar.app.ui.theme.Mono
import kotlin.math.abs
import kotlin.math.roundToInt

/** Okrągła ocena 0–100 w kolorze progu (jak przy lądowaniu). */
@Composable
fun ScoreCircle(score: Int?, size: Int = 56) {
    val c = Fm.c
    val col = score?.let { scoreColor(it) } ?: c.ink2
    Box(Modifier.size(size.dp).clip(CircleShape).border(2.dp, col, CircleShape), contentAlignment = Alignment.Center) {
        Text(score?.toString() ?: "—", fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = (size / 3).sp, color = col)
    }
}

@Composable
private fun ScoreHeader(score: Int, title: String, sub: String) {
    val c = Fm.c
    Row(verticalAlignment = Alignment.CenterVertically) {
        ScoreCircle(score)
        Spacer(Modifier.width(14.dp))
        Column {
            Text(title, style = MaterialTheme.typography.titleMedium, color = scoreColor(score))
            Text(sub, fontFamily = Mono, fontSize = 13.sp, color = c.ink)
        }
    }
    Spacer(Modifier.size(10.dp))
}

@Composable
private fun Notes(notes: List<String>) {
    val c = Fm.c
    notes.forEach { Text("• $it", style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.padding(vertical = 2.dp)) }
}

@Composable
fun ApproachSection(a: ApproachAnalysis, ordinal: String?) {
    val c = Fm.c
    Section(loc("common.approach") + (ordinal?.let { " $it" } ?: ""), Icons.Outlined.FlightLand) {
        ScoreHeader(a.score, when {
            a.stableFromFt >= 1000 -> loc("flight_review.stabilised")
            a.stableFromFt >= 500 -> loc("flight_review.late_stabilisation")
            else -> loc("flight_review.unstable")
        }, loc("common.stable_from_ft", a.stableFromFt))
        // Bramki stabilizacji: wartości w chwili przejścia przez 1000 i 500 ft nad przyziemieniem
        if (a.gates.isNotEmpty()) {
            Row(Modifier.fillMaxWidth().padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                a.gates.forEach { g ->
                    val col = if (g.stable) c.green else c.caution
                    Column(Modifier.weight(1f).clip(RoundedCornerShape(8.dp)).border(1.dp, col, RoundedCornerShape(8.dp)).padding(8.dp)) {
                        Text("${g.heightFt} ft " + if (g.stable) "✓" else "✗", fontFamily = Mono, fontWeight = FontWeight.Bold, color = col, fontSize = 13.sp)
                        Text(listOfNotNull(g.iasKt?.let { "$it kt" }, g.vsFpm?.let { "$it ft/min" }, g.bankDeg?.let { "${abs(it).roundToInt()}°" }).joinToString("  "),
                            fontFamily = Mono, fontSize = 11.sp, color = c.ink)
                    }
                }
            }
        }
        FactGrid(listOf(
            Fact(loc("flight_review.path_angle"), a.pathAngleDeg?.let { "%.1f°".format(it) } ?: "—", accent = a.pathAngleDeg?.takeIf { abs(it - 3) > 0.5 }?.let { c.caution }),
            Fact(loc("flight_review.speed_spread"), a.iasSpreadKt?.let { "$it kt" } ?: "—", accent = a.iasSpreadKt?.takeIf { it > 15 }?.let { c.caution }),
            Fact(loc("flight_review.max_sink"), a.maxSinkFpm?.let { "$it ft/min" } ?: "—", accent = a.maxSinkFpm?.takeIf { it > 1000 }?.let { c.caution }),
            Fact(loc("flight_review.max_bank"), a.maxBankDeg?.let { "${it.roundToInt()}°" } ?: "—", accent = a.maxBankDeg?.takeIf { it > 7 }?.let { c.caution }),
        ))
        Spacer(Modifier.size(8.dp))
        Notes(a.notes)
    }
}

@Composable
fun SmoothnessSection(s: Smoothness?) {
    val c = Fm.c
    Section(loc("flight_review.handling_smoothness"), Icons.Outlined.Waves) {
        if (s == null) {
            Text(loc("flight_review.the_track_is_too_sparse_a"),
                style = MaterialTheme.typography.bodyMedium, color = c.ink2)
            return@Section
        }
        ScoreHeader(s.score, when { s.score >= 85 -> loc("flight_review.smooth"); s.score >= 65 -> loc("flight_review.fair"); else -> loc("flight_review.rough") },
            loc("flight_review.abrupt_inputs", s.jerks))
        FactGrid(listOf(
            Fact(loc("flight_review.roll_rate"), s.bankRateRms?.let { "%.1f°/s".format(it) } ?: "—"),
            Fact(loc("flight_review.pitch_rate"), s.pitchRateRms?.let { "%.1f°/s".format(it) } ?: "—"),
            Fact(loc("flight_review.v_s_changes"), "${s.vsChangeRms.roundToInt()} ft/min/s"),
            Fact(loc("flight_review.max_bank_g"), (s.maxBankDeg?.let { "${it.roundToInt()}°" } ?: "—") + " / " + (s.maxG?.let { "%.1f".format(it) } ?: "—")),
        ))
        Spacer(Modifier.size(8.dp))
        Notes(s.notes)
    }
}

@Composable
fun ScenarioSection(selected: Scenario?, result: ScenarioResult?, onSelect: (Scenario?) -> Unit) {
    val c = Fm.c
    Section(loc("flight_review.exercise_completion"), Icons.Outlined.Assignment) {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            FilterChip(selected == null, { onSelect(null) }, { Text(loc("flight_review.none")) })
            Scenario.entries.forEach { s -> FilterChip(selected == s, { onSelect(s) }, { Text(s.title) }) }
        }
        if (selected == null || result == null) {
            Text(loc("flight_review.pick_an_exercise_to_check_the"),
                style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 8.dp))
            return@Section
        }
        Text(selected.description, style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(vertical = 8.dp))
        ScoreHeader(result.percent, loc("flight_review.of", result.passed, result.judged, criteriaWord(result.judged)), "${result.percent}%")
        result.criteria.forEach { cr ->
            val col = when (cr.passed) { true -> c.green; false -> c.danger; null -> c.ink2 }
            Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(when (cr.passed) { true -> "✓"; false -> "✗"; null -> "?" }, color = col, fontWeight = FontWeight.Bold, modifier = Modifier.width(22.dp))
                Text(cr.label, style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.weight(1f))
                Text(cr.value, fontFamily = Mono, fontSize = 13.sp, color = col)
            }
        }
    }
}

@Composable
fun ComparisonSection(cmp: Comparison?) {
    val c = Fm.c
    Section(loc("flight_review.compared_with_the_previous_flight"), Icons.Outlined.CompareArrows) {
        if (cmp == null) {
            Text(loc("flight_review.no_earlier_flight_on_this_route"),
                style = MaterialTheme.typography.bodyMedium, color = c.ink2)
            return@Section
        }
        val p = cmp.previous
        Text((if (cmp.sameRoute) loc("flight_review.same_route") else loc("flight_review.similar_flight")) + ": ${p.from ?: "?"} → ${p.to ?: "?"}, " +
            java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM, com.freemetar.app.domain.Lang.locale).format(java.util.Date(p.start)),
            style = MaterialTheme.typography.bodySmall, color = c.ink2)
        Spacer(Modifier.size(8.dp))
        Row(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
            Spacer(Modifier.weight(1f))
            Text(loc("common.now"), fontSize = 11.sp, color = c.ink2, modifier = Modifier.width(92.dp))
            Text(loc("flight_review.before"), fontSize = 11.sp, color = c.ink2, modifier = Modifier.width(92.dp))
        }
        cmp.rows.forEach { r ->
            val col = when (r.better) { true -> c.green; false -> c.danger; null -> c.ink }
            Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(c.terrain.copy(alpha = 0.35f)).padding(horizontal = 6.dp, vertical = 5.dp).padding(vertical = 0.dp),
                verticalAlignment = Alignment.CenterVertically) {
                Text(r.label, style = MaterialTheme.typography.bodyMedium, color = c.ink, modifier = Modifier.weight(1f))
                Text(r.now + when (r.better) { true -> " ▲"; false -> " ▼"; null -> "" }, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = col, modifier = Modifier.width(92.dp))
                Text(r.before, fontFamily = Mono, fontSize = 13.sp, color = c.ink2, modifier = Modifier.width(92.dp))
            }
            Spacer(Modifier.size(3.dp))
        }
    }
}

@Composable
fun InstructorReportSection(report: String, title: String) {
    val c = Fm.c
    val ctx = LocalContext.current
    Section(loc("flight_review.instructor_report"), Icons.Outlined.School) {
        Text(report, fontFamily = Mono, fontSize = 12.sp, color = c.ink, lineHeight = 17.sp,
            modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(c.paper).padding(10.dp))
        OutlinedButton({
            val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_SUBJECT, title).putExtra(Intent.EXTRA_TEXT, report)
            ctx.startActivity(Intent.createChooser(send, loc("flight_review.share_report")))
        }, Modifier.padding(top = 10.dp)) {
            Icon(Icons.Outlined.Share, null, Modifier.size(18.dp))
            Spacer(Modifier.width(8.dp))
            Text(loc("flight_review.share_report"))
        }
    }
}
