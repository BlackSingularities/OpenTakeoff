package com.freemetar.app.data.link

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit

/**
 * Synchronizacja dziennika lotów z programem OpenTakeoff na Windows (funkcja `logbook_sync`, HTTP na porcie
 * Linka, docs/link-protocol.md p. 6). Telefon podaje swoje loty i loty usunięte; komputer odpowiada, które
 * przysłać (`want`), które ma dla telefonu (`offer`) i które usunięto na komputerze (`remove`).
 * Ten sam lot nagrany jednocześnie w telefonie i na komputerze (pokrywający się czas) nie jest przesyłany.
 */
object LogbookSync {
    @Serializable data class SyncFlight(val id: String, val start: Long, val end: Long, val aircraft: String? = null)
    @Serializable data class SyncRequest(val v: Int = 1, val device: String, val flights: List<SyncFlight>, val deleted: List<String>)
    @Serializable data class SyncResponse(val v: Int = 1, val want: List<String> = emptyList(), val offer: List<String> = emptyList(), val remove: List<String> = emptyList(), val flights: Int = 0)
    @Serializable private data class DownloadRequest(val ids: List<String>)

    data class Result(val flights: List<FlightLog>, val sent: Int, val received: Int, val removed: Int)

    private val http = OkHttpClient.Builder().connectTimeout(10, TimeUnit.SECONDS).readTimeout(120, TimeUnit.SECONDS).writeTimeout(120, TimeUnit.SECONDS).build()
    private val jsonType = "application/json".toMediaType()
    private val binType = "application/octet-stream".toMediaType()

    private fun post(url: String, token: String, body: okhttp3.RequestBody): ByteArray {
        val req = Request.Builder().url(url).header("Authorization", "Bearer $token").post(body).build()
        http.newCall(req).execute().use { r ->
            if (!r.isSuccessful) throw LinkException("http_${r.code}", "HTTP ${r.code}")
            return r.body?.bytes() ?: ByteArray(0)
        }
    }

    /** Pełna synchronizacja; zwraca nową listę lotów telefonu. Rzuca wyjątek przy błędzie sieci. */
    suspend fun run(ip: String, port: Int, token: String, device: String, local: List<FlightLog>, deleted: Set<String>): Result = withContext(Dispatchers.IO) {
        val base = "http://$ip:$port/v1/logbook"
        val req = SyncRequest(device = device, flights = local.map { SyncFlight(it.id, it.start, it.end, it.aircraft) }, deleted = deleted.toList())
        val plan = LinkJson.decodeFromString(SyncResponse.serializer(), post("$base/sync", token, LinkJson.encodeToString(SyncRequest.serializer(), req).toRequestBody(jsonType)).decodeToString())
        // Usunięte na komputerze – usuwamy też tutaj
        var flights = local.filterNot { it.id in plan.remove }
        val want = flights.filter { it.id in plan.want }
        if (want.isNotEmpty()) {
            // Paczkami, żeby jedno zapytanie nie było ogromne
            want.chunked(40).forEach { part -> post("$base/upload", token, FlightArchive.encode(part).toRequestBody(binType)) }
        }
        var received = 0
        if (plan.offer.isNotEmpty()) {
            plan.offer.chunked(40).forEach { ids ->
                val bytes = post("$base/download", token, LinkJson.encodeToString(DownloadRequest.serializer(), DownloadRequest(ids)).toRequestBody(jsonType))
                val incoming = FlightArchive.decode(bytes).filterNot { it.id in deleted }
                val before = flights.size
                flights = FlightArchive.merge(flights, incoming)
                received += flights.size - before
            }
        }
        Result(flights, want.size, received, local.size - local.count { it.id !in plan.remove })
    }
}
