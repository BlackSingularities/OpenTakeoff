package com.freemetar.app.data.link

import com.freemetar.app.domain.loc

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.net.SocketTimeoutException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.util.concurrent.atomic.AtomicLong

sealed interface LinkStatus {
    data object Idle : LinkStatus
    data class Connecting(val ip: String) : LinkStatus
    /** Serwer wymaga kodu parowania (pierwsze połączenie albo token unieważniony). */
    data class NeedsCode(val ip: String, val port: Int, val error: String? = null) : LinkStatus
    data class Connected(val ip: String, val server: String, val source: String?, val message: String?) : LinkStatus
    data class Lost(val ip: String, val reason: String) : LinkStatus
}

/** Zapis tokenu parowania (DataStore) — klient nie zna szczegółów przechowywania. */
interface LinkTokenStore {
    suspend fun token(ip: String): String?
    suspend fun saveToken(ip: String, token: String)
    suspend fun clearToken(ip: String)
}

/**
 * Klient OpenTakeoff Link: odkrywanie po UDP, parowanie, strumień `state`, komendy.
 * Po zerwaniu połączenia sam próbuje wrócić (co 3 s), dopóki użytkownik nie rozłączy.
 */
class LinkClient(private val context: Context, private val tokens: LinkTokenStore, private val deviceName: String = Build.MODEL ?: "Android") {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val http = OkHttpClient.Builder().pingInterval(0, TimeUnit.SECONDS).readTimeout(0, TimeUnit.SECONDS).build()

    private val _servers = MutableStateFlow<List<LinkServer>>(emptyList())
    val servers = _servers.asStateFlow()
    private val _status = MutableStateFlow<LinkStatus>(LinkStatus.Idle)
    val status = _status.asStateFlow()
    private val _state = MutableStateFlow<FlightState?>(null)
    val state = _state.asStateFlow()
    private val _lastError = MutableStateFlow<String?>(null)
    val lastError = _lastError.asStateFlow()
    var limits = Limits()
        private set

    private var socket: WebSocket? = null
    private var discoveryJob: Job? = null
    private var pingJob: Job? = null
    private var reconnectJob: Job? = null
    private var wantConnected = false
    private var target: Pair<String, Int>? = null
    private val ids = AtomicLong(1)
    private var lastStateAt = 0L
    @Volatile private var lastMessageAt = 0L

    // ---------- odkrywanie ----------

    fun startDiscovery() {
        if (discoveryJob?.isActive == true) return
        discoveryJob = scope.launch {
            val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            // Bez blokady multicastu wiele telefonów odrzuca pakiety broadcast przy wyłączonym ekranie Wi-Fi
            val lock = wifi?.createMulticastLock("openairbook-link")?.apply { setReferenceCounted(false); acquire() }
            try {
                DatagramSocket(null).use { sock ->
                    sock.reuseAddress = true
                    sock.broadcast = true
                    sock.soTimeout = 2500
                    sock.bind(InetSocketAddress(47811))
                    val buf = ByteArray(4096)
                    while (isActive) {
                        try {
                            val p = DatagramPacket(buf, buf.size)
                            sock.receive(p)
                            val a = runCatching { LinkJson.decodeFromString(Announce.serializer(), String(p.data, 0, p.length, Charsets.UTF_8)) }.getOrNull()
                            if (a != null && a.type == "announce" && a.v == 1) {
                                val ip = p.address?.hostAddress ?: a.ips.firstOrNull() ?: continue
                                val s = LinkServer(a.name, a.host ?: ip, ip, a.port, a.source, a.version, System.currentTimeMillis())
                                _servers.update { list -> (list.filterNot { it.ip == ip } + s).sortedBy { it.host } }
                            }
                        } catch (_: SocketTimeoutException) {
                        }
                        // Serwer, który nie ogłaszał się od 10 s, znika z listy
                        val now = System.currentTimeMillis()
                        _servers.update { list -> list.filter { now - it.lastSeen < 10_000 } }
                    }
                }
            } catch (e: Exception) {
                _lastError.value = loc("Nie można nasłuchiwać w sieci (UDP 47811): ${e.message}", "Cannot listen on the network (UDP 47811): ${e.message}")
            } finally {
                lock?.release()
            }
        }
    }

    fun stopDiscovery() { discoveryJob?.cancel(); discoveryJob = null }

    // ---------- połączenie ----------

    /** Łączy z serwerem; z kodem przy pierwszym parowaniu, bez kodu używa zapisanego tokenu. */
    fun connect(ip: String, port: Int = 47810, code: String? = null) {
        wantConnected = true
        target = ip to port
        reconnectJob?.cancel()
        open(ip, port, code)
    }

    fun disconnect() {
        wantConnected = false
        reconnectJob?.cancel()
        pingJob?.cancel()
        socket?.close(1000, loc("rozłączono", "disconnected"))
        socket = null
        _status.value = LinkStatus.Idle
        _state.value = null
    }

    /**
     * [retry] = ponowna próba po zerwaniu: status zostaje [LinkStatus.Lost], bo „Connecting” oznacza dla aplikacji
     * nowe połączenie — zapis lotu by się zakończył, a usługa w tle (blokada Wi-Fi) zatrzymała.
     */
    private fun open(ip: String, port: Int, code: String?, retry: Boolean = false) {
        socket?.cancel()
        if (!retry) _status.value = LinkStatus.Connecting(ip)
        scope.launch {
            val token = if (code == null) tokens.token(ip) else null
            if (code == null && token == null) {
                _status.value = LinkStatus.NeedsCode(ip, port)
                return@launch
            }
            val req = Request.Builder().url("ws://$ip:$port/").build()
            socket = http.newWebSocket(req, Listener(ip, port, code, token))
        }
    }

    private inner class Listener(val ip: String, val port: Int, val code: String?, val token: String?) : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            val hello = Hello(code = code, token = token, client = ClientInfo(deviceName), rateHz = 10)
            webSocket.send(LinkJson.encodeToString(Hello.serializer(), hello))
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            lastMessageAt = System.currentTimeMillis()
            // Odpowiedzi na zapytania o dane nawigacyjne trafiają do oczekującego wywołania po id
            val obj = runCatching { LinkJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
            val type = obj["type"]?.jsonPrimitive?.contentOrNull
            val id = obj["id"]?.jsonPrimitive?.contentOrNull
            if (id != null && pending.containsKey(id) && (type == "navdata" || type == "error")) {
                val waiter = pending.remove(id) ?: return
                if (type == "navdata") waiter.complete(obj["data"] ?: JsonNull)
                else waiter.completeExceptionally(LinkException(obj["code"]?.jsonPrimitive?.contentOrNull ?: "error", obj["message"]?.jsonPrimitive?.contentOrNull))
                return
            }
            val m = runCatching { LinkJson.decodeFromJsonElement(Incoming.serializer(), obj) }.getOrNull() ?: return
            when (m.type) {
                "hello_ok" -> {
                    m.token?.let { t -> scope.launch { tokens.saveToken(ip, t) } }
                    m.limits?.let { limits = it }
                    _status.value = LinkStatus.Connected(ip, m.server ?: "OpenTakeoff Link", m.source, null)
                    startPing(webSocket)
                }
                "status" -> _status.update { s -> if (s is LinkStatus.Connected) s.copy(source = m.source, message = m.message) else s }
                "state" -> m.data?.let { _state.value = it; lastStateAt = System.currentTimeMillis() }
                "error" -> when (m.code) {
                    "pairing_failed" -> {
                        scope.launch { tokens.clearToken(ip) }
                        _status.value = LinkStatus.NeedsCode(ip, port, if (code != null) loc("Nieprawidłowy kod parowania.", "Wrong pairing code.") else loc("Parowanie wygasło — wpisz nowy kod.", "Pairing expired — enter a new code."))
                        wantConnected = false
                        webSocket.close(1000, null)
                    }
                    "rate_limited" -> _lastError.value = loc("Za dużo prób — odczekaj minutę.", "Too many attempts — wait a minute.")
                    else -> _lastError.value = m.message ?: m.code
                }
            }
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = lost(webSocket, t.message ?: loc("błąd połączenia", "connection error"))
        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = lost(webSocket, reason.ifBlank { loc("połączenie zamknięte", "connection closed") })

        private fun lost(ws: WebSocket, reason: String) {
            if (ws != socket) return
            pingJob?.cancel()
            _state.value = null
            if (_status.value is LinkStatus.NeedsCode) return
            if (wantConnected) {
                _status.value = LinkStatus.Lost(ip, reason)
                reconnectJob?.cancel()
                reconnectJob = scope.launch { delay(3000); if (wantConnected) open(ip, port, null, retry = true) }
            } else {
                _status.value = LinkStatus.Idle
            }
        }
    }

    private fun startPing(ws: WebSocket) {
        pingJob?.cancel()
        lastMessageAt = System.currentTimeMillis()
        pingJob = scope.launch {
            var tick = 0
            while (isActive) {
                delay(1000)
                val now = System.currentTimeMillis()
                // Brak stanów (MSFS zamknięty, Link bez źródła): nie pokazuj ostatniego lotu jako bieżącego
                if (_state.value != null && now - lastStateAt > 5_000) _state.value = null
                // Martwe połączenie (komputer uśpiony, Wi-Fi zerwane bez zamknięcia gniazda): zerwij i połącz od nowa
                if (now - lastMessageAt > 20_000) { ws.cancel(); return@launch }
                if (++tick % 5 == 0) ws.send(LinkJson.encodeToString(Ping.serializer(), Ping(id = "p${ids.getAndIncrement()}", t = now)))
            }
        }
    }

    // ---------- dane nawigacyjne ----------

    private val pending = ConcurrentHashMap<String, CompletableDeferred<JsonElement>>()

    /**
     * Zapytanie `navdata_request` (kind: airport / procedures / procedure_legs / fix).
     * Zwraca pole `data` odpowiedzi; rzuca [LinkException] przy błędzie albo braku połączenia.
     */
    suspend fun navdata(kind: String, params: Map<String, Any?>): JsonElement {
        val ws = socket
        if (ws == null || _status.value !is LinkStatus.Connected) throw LinkException("not_connected", loc("Brak połączenia z OpenTakeoff Link", "Not connected to OpenTakeoff Link"))
        val id = "n${ids.getAndIncrement()}"
        val msg = buildJsonObject {
            put("v", 1); put("type", "navdata_request"); put("id", id); put("kind", kind)
            params.forEach { (k, v) ->
                when (v) {
                    null -> {}
                    is Number -> put(k, v)
                    is Boolean -> put(k, v)
                    else -> put(k, v.toString())
                }
            }
        }
        val waiter = CompletableDeferred<JsonElement>()
        pending[id] = waiter
        if (!ws.send(msg.toString())) { pending.remove(id); throw LinkException("send_failed", loc("Nie wysłano zapytania", "Request not sent")) }
        return try {
            withTimeout(10_000) { waiter.await() }
        } catch (e: TimeoutCancellationException) {
            pending.remove(id)
            throw LinkException("timeout", loc("OpenTakeoff Link nie odpowiedział w 10 s", "OpenTakeoff Link did not respond within 10 s"))
        }
    }

    // ---------- komendy ----------

    fun command(name: String, value: Double? = null): Boolean {
        val ws = socket ?: return false
        if (_status.value !is LinkStatus.Connected) return false
        return ws.send(LinkJson.encodeToString(Command.serializer(), Command(id = "c${ids.getAndIncrement()}", name = name, value = value)))
    }

    fun axis(aileron: Double, elevator: Double, rudder: Double = 0.0): Boolean {
        val ws = socket ?: return false
        return ws.send(LinkJson.encodeToString(AxisMessage.serializer(), AxisMessage(aileron = aileron, elevator = elevator, rudder = rudder)))
    }

    /**
     * Osie hamulców 0…1: [speedbrake] zostaje w ustawionej pozycji, [brake] (koła, tylko na ziemi)
     * trzeba przysyłać, dopóki jest wciśnięty — Link puszcza go sam po 0,5 s ciszy. null = bez zmian.
     */
    fun brakes(speedbrake: Double? = null, brake: Double? = null): Boolean {
        val ws = socket ?: return false
        if (speedbrake == null && brake == null) return false
        return ws.send(LinkJson.encodeToString(AxisMessage.serializer(), AxisMessage(speedbrake = speedbrake, brake = brake)))
    }

    fun releaseAxes() {
        socket?.send(LinkJson.encodeToString(AxisMessage.serializer(), AxisMessage(release = true)))
    }

    /** Ile ms minęło od ostatniego `state` — do wykrywania zamrożonego strumienia. */
    fun stateAgeMs() = if (lastStateAt == 0L) Long.MAX_VALUE else System.currentTimeMillis() - lastStateAt
}

/** Błąd zwrócony przez OpenTakeoff Link (kod z protokołu) albo brak połączenia. */
class LinkException(val code: String, message: String?) : Exception(message ?: code)
