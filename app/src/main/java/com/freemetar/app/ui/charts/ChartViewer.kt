package com.freemetar.app.ui.charts

import com.freemetar.app.domain.loc

import android.graphics.Bitmap
import android.graphics.Color as AColor
import android.graphics.Matrix
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.ChevronLeft
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.DarkMode
import androidx.compose.material.icons.outlined.LightMode
import androidx.compose.material.icons.outlined.ZoomOutMap
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.freemetar.app.data.charts.Chart
import com.freemetar.app.data.charts.FaaCharts
import com.freemetar.app.ui.components.Pill
import com.freemetar.app.ui.theme.Fm
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.Closeable
import java.io.File
import kotlin.math.max
import kotlin.math.min

/** [ChartViewer] na pełnym ekranie w oknie dialogowym (bez nawigacji w aplikacji). */
@Composable
fun ChartViewerDialog(chart: Chart, onClose: () -> Unit) {
    Dialog(onDismissRequest = onClose, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        ChartViewer(chart, onClose)
    }
}

/**
 * Pełnoekranowa karta FAA: PdfRenderer, szczypanie i przesuwanie, kilka stron (CONT.n),
 * tryb nocny z odwróceniem kolorów (białe tło karty staje się czernią AMOLED).
 * Ostrość przy powiększeniu: po zatrzymaniu gestu widoczny fragment jest renderowany
 * ponownie w rozdzielczości ekranu i nakładany na podgląd całej strony.
 */
@Composable
fun ChartViewer(chart: Chart, onClose: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val repo = remember { FaaCharts.get(context) }
    val c = Fm.c
    var doc by remember(chart) { mutableStateOf<ChartDocument?>(null) }
    var error by remember(chart) { mutableStateOf<String?>(null) }
    var attempt by remember { mutableIntStateOf(0) }
    var page by rememberSaveable(chart) { mutableIntStateOf(0) }
    var night by rememberSaveable { mutableStateOf(c.isNight) }

    LaunchedEffect(chart, attempt) {
        error = null
        runCatching { ChartDocument.open(repo.pdfs(chart)) }
            .onSuccess { doc = it }
            .onFailure { error = it.message ?: loc("chart_viewer.could_not_download_the_chart") }
    }
    DisposableEffect(doc) { val d = doc; onDispose { d?.close() } }

    Box(modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        val d = doc
        when {
            d != null -> PdfPage(d, page.coerceIn(0, d.pageCount - 1), night, Modifier.fillMaxSize())
            error != null -> Column(Modifier.align(Alignment.Center).padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text(error!!, color = c.danger, style = MaterialTheme.typography.bodyMedium)
                Spacer(Modifier.size(12.dp))
                Pill(loc("common.try_again")) { attempt++ }
            }
            else -> CircularProgressIndicator(Modifier.align(Alignment.Center), color = MaterialTheme.colorScheme.primary)
        }

        // Pasek u góry: pigułki na czarnym tle, karta przewija się pod nim
        Row(
            Modifier.fillMaxWidth().safeDrawingPadding().padding(horizontal = 10.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            RoundButton(Icons.AutoMirrored.Outlined.ArrowBack, loc("common.close"), onClick = onClose)
            Text(
                chart.name, color = MaterialTheme.colorScheme.onBackground, style = MaterialTheme.typography.titleSmall,
                maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.background).padding(horizontal = 12.dp, vertical = 8.dp),
            )
            RoundButton(if (night) Icons.Outlined.LightMode else Icons.Outlined.DarkMode, if (night) loc("chart_viewer.day_mode") else loc("chart_viewer.night_mode")) { night = !night }
        }
        val count = d?.pageCount ?: 0
        if (count > 1) {
            Row(
                Modifier.align(Alignment.BottomCenter).safeDrawingPadding().padding(bottom = 12.dp)
                    .clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.background).padding(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RoundButton(Icons.Outlined.ChevronLeft, loc("chart_viewer.previous_page"), enabled = page > 0) { page-- }
                Text("${page + 1} / $count", color = MaterialTheme.colorScheme.onBackground, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(horizontal = 10.dp))
                RoundButton(Icons.Outlined.ChevronRight, loc("chart_viewer.next_page"), enabled = page < count - 1) { page++ }
            }
        }
    }
}

@Composable
private fun RoundButton(icon: ImageVector, label: String, enabled: Boolean = true, onClick: () -> Unit) {
    Box(
        Modifier.size(40.dp).clip(RoundedCornerShape(50)).background(Fm.c.terrain)
            .clickable(enabled = enabled, role = Role.Button, onClickLabel = label, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, label, tint = if (enabled) Fm.c.ink else Fm.c.ink2.copy(alpha = 0.4f), modifier = Modifier.size(20.dp))
    }
}

/** Odwrócenie kolorów: białe tło → czerń, czarne linie → biel. */
private val InvertFilter = ColorFilter.colorMatrix(
    ColorMatrix(
        floatArrayOf(
            -1f, 0f, 0f, 0f, 255f,
            0f, -1f, 0f, 0f, 255f,
            0f, 0f, -1f, 0f, 255f,
            0f, 0f, 0f, 1f, 0f,
        )
    )
)

private const val MAX_ZOOM = 10f

/** Zrenderowany widoczny fragment, ważny tylko dla tej samej skali i położenia strony. */
private class Tile(val image: ImageBitmap, val scale: Float, val left: Float, val top: Float)

@Composable
private fun PdfPage(doc: ChartDocument, index: Int, night: Boolean, modifier: Modifier) {
    BoxWithConstraints(modifier) {
        val w = constraints.maxWidth.toFloat()
        val h = constraints.maxHeight.toFloat()
        val ratio = remember(doc, index) { doc.aspect(index) } ?: return@BoxWithConstraints
        // Strona dopasowana do ekranu przy skali 1
        val fitW = min(w, h / ratio)
        val fitH = fitW * ratio

        var scale by remember(index) { mutableFloatStateOf(1f) }
        var left by remember(index, w, h) { mutableFloatStateOf((w - fitW) / 2) }
        var top by remember(index, w, h) { mutableFloatStateOf((h - fitH) / 2) }
        var base by remember(doc, index) { mutableStateOf<ImageBitmap?>(null) }
        var tile by remember(doc, index) { mutableStateOf<Tile?>(null) }

        fun clamp() {
            val pw = fitW * scale
            val ph = fitH * scale
            left = if (pw <= w) (w - pw) / 2 else left.coerceIn(w - pw, 0f)
            top = if (ph <= h) (h - ph) / 2 else top.coerceIn(h - ph, 0f)
        }

        fun zoomAt(centroid: Offset, factor: Float, pan: Offset = Offset.Zero) {
            val ns = (scale * factor).coerceIn(1f, MAX_ZOOM)
            val z = ns / scale
            left = centroid.x - (centroid.x - left) * z + pan.x
            top = centroid.y - (centroid.y - top) * z + pan.y
            scale = ns
            clamp()
        }

        // Podgląd całej strony: 2× rozmiar dopasowany, ale nie więcej niż ~2000 px w dłuższym boku
        LaunchedEffect(doc, index, fitW) {
            val k = min(2f, 2000f / max(fitW, fitH))
            base = doc.render(index, IntSize((fitW * k).toInt().coerceAtLeast(1), (fitH * k).toInt().coerceAtLeast(1)), fitW * k, 0f, 0f)
        }
        // Ostry fragment po zatrzymaniu gestu (debounce), tylko przy powiększeniu
        LaunchedEffect(doc, index, scale, left, top) {
            if (scale <= 1.05f) { tile = null; return@LaunchedEffect }
            delay(160)
            val s = scale; val l = left; val t = top
            val img = doc.render(index, IntSize(w.toInt(), h.toInt()), fitW * s, l, t)
            if (img != null) tile = Tile(img, s, l, t)
        }

        val filter = if (night) InvertFilter else null
        Canvas(
            Modifier.fillMaxSize()
                .pointerInput(index, fitW, fitH) {
                    detectTransformGestures { centroid, pan, zoom, _ -> zoomAt(centroid, zoom, pan) }
                }
                .pointerInput(index, fitW, fitH) {
                    detectTapGestures(onDoubleTap = { p -> if (scale > 1.5f) zoomAt(p, 1f / scale) else zoomAt(p, 3f) })
                },
        ) {
            val pw = fitW * scale
            val ph = fitH * scale
            base?.let {
                drawImage(
                    it, dstOffset = IntOffset(left.toInt(), top.toInt()), dstSize = IntSize(pw.toInt(), ph.toInt()),
                    filterQuality = FilterQuality.Medium, colorFilter = filter,
                )
            }
            tile?.takeIf { it.scale == scale && it.left == left && it.top == top }?.let {
                drawImage(it.image, colorFilter = filter)
            }
        }

        if (scale > 1.05f) {
            Box(Modifier.align(Alignment.BottomEnd).safeDrawingPadding().padding(12.dp)) {
                RoundButton(Icons.Outlined.ZoomOutMap, loc("chart_viewer.fit_page")) { scale = 1f; clamp() }
            }
        }
    }
}

/**
 * Strony z kilku plików PDF jako jeden dokument. PdfRenderer nie jest wątkowo bezpieczny
 * i pozwala na jedną otwartą stronę naraz — renderowanie idzie przez [lock] na jednym wątku.
 */
@OptIn(ExperimentalCoroutinesApi::class)
private class ChartDocument private constructor(
    private val files: List<Pair<ParcelFileDescriptor, PdfRenderer>>,
) : Closeable {
    private val lock = Mutex()
    private val io = Dispatchers.IO.limitedParallelism(1)
    private val pages: List<Pair<Int, Int>> = files.flatMapIndexed { fi, (_, r) -> (0 until r.pageCount).map { fi to it } }
    private val aspects = HashMap<Int, Float>()
    @Volatile private var closed = false

    val pageCount: Int get() = pages.size

    /** Wysokość / szerokość strony (pierwsze wywołanie otwiera stronę). */
    fun aspect(index: Int): Float? = aspects[index] ?: runCatching {
        val (fi, pi) = pages[index]
        synchronized(this) { files[fi].second.openPage(pi).use { it.height.toFloat() / it.width } }
    }.getOrNull()?.also { aspects[index] = it }

    /**
     * Renderuje stronę do bitmapy [size]: strona ma szerokość [pageWidthPx] i lewy górny róg
     * w ([left], [top]) względem bitmapy. Tło białe (PdfRenderer zostawia przezroczyste).
     */
    suspend fun render(index: Int, size: IntSize, pageWidthPx: Float, left: Float, top: Float): ImageBitmap? = lock.withLock {
        withContext(io) {
            if (closed || size.width <= 0 || size.height <= 0) return@withContext null
            runCatching {
                val (fi, pi) = pages[index]
                synchronized(this@ChartDocument) {
                    files[fi].second.openPage(pi).use { p ->
                        val bmp = Bitmap.createBitmap(size.width, size.height, Bitmap.Config.ARGB_8888)
                        bmp.eraseColor(AColor.WHITE)
                        val k = pageWidthPx / p.width
                        val m = Matrix().apply { setScale(k, k); postTranslate(left, top) }
                        p.render(bmp, null, m, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                        bmp.asImageBitmap()
                    }
                }
            }.getOrNull()
        }
    }

    override fun close() {
        synchronized(this) {
            if (closed) return
            closed = true
            files.forEach { (fd, r) -> runCatching { r.close() }; runCatching { fd.close() } }
        }
    }

    companion object {
        suspend fun open(files: List<File>): ChartDocument = withContext(Dispatchers.IO) {
            val opened = ArrayList<Pair<ParcelFileDescriptor, PdfRenderer>>()
            try {
                files.forEach { f ->
                    val fd = ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY)
                    opened += fd to PdfRenderer(fd)
                }
            } catch (e: Exception) {
                opened.forEach { (fd, r) -> runCatching { r.close() }; runCatching { fd.close() } }
                // Uszkodzony plik (przerwane pobieranie) — usuwamy, żeby kolejna próba pobrała go od nowa
                files.forEach { it.delete() }
                throw java.io.IOException(loc("chart_viewer.could_not_open_chart_pdf") + e.message, e)
            }
            ChartDocument(opened)
        }
    }
}
