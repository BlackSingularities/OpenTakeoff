using OpenAirBookLink.Core.Localization;
using System.Net;
using System.Net.Sockets;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using OpenAirBookLink.Core.Control;
using OpenAirBookLink.Core.NavData;
using OpenAirBookLink.Core.Pairing;
using OpenAirBookLink.Core.Protocol;
using OpenAirBookLink.Core.Sources;

namespace OpenAirBookLink.Core.Server;

public sealed record ClientInfo(string Id, string Name, string RemoteIp, DateTime ConnectedAt, bool Paired, int RateHz);

// Serwer WebSocket dla telefonów: parowanie, strumień `state`, komendy i osie.
public sealed class LinkServer : IDisposable
{
    public static readonly TimeSpan HelloTimeout = TimeSpan.FromSeconds(10);
    public static readonly TimeSpan IdleTimeout = TimeSpan.FromSeconds(30);
    public const int MaxCommandsPerSecond = 50;
    public const int MaxNavdataInFlight = 4;

    private readonly PairingManager _pairing;
    private readonly string _version;
    private readonly int _port;
    private readonly List<Session> _sessions = new();
    private readonly CancellationTokenSource _cts = new();
    private TcpListener? _listener;
    private IFlightSource? _source;
    private long _seq;
    private readonly NavDataService _navData = new();

    public LinkServer(PairingManager pairing, AxisController axes, string version, int port = LinkProtocol.WebSocketPort)
    {
        _pairing = pairing;
        Axes = axes;
        _version = version;
        _port = port;
    }

    public AxisController Axes { get; }

    public int Port => (_listener?.LocalEndpoint as IPEndPoint)?.Port ?? _port;

    public event Action<string>? Log;
    public event Action? ClientsChanged;

    public IFlightSource? Source
    {
        get => Volatile.Read(ref _source);
        set
        {
            Volatile.Write(ref _source, value);
            BroadcastStatus();
        }
    }

    public string SourceName
    {
        get
        {
            var s = Source;
            return s is { IsConnected: true } ? s.Kind : "none";
        }
    }

    public IReadOnlyList<ClientInfo> Clients
    {
        get
        {
            lock (_sessions)
                return _sessions.Select(s => new ClientInfo(s.Id, s.Name, s.RemoteIp, s.ConnectedAt, s.Paired, s.RateHz)).ToList();
        }
    }

    public void Start()
    {
        _listener = new TcpListener(IPAddress.Any, _port);
        _listener.Start();
        Log?.Invoke(L.T("server.listening", Port));
        _ = AcceptLoopAsync(_cts.Token);
    }

    // Wyślij `status` wszystkim sparowanym (zmiana źródła danych / połączenia z MSFS).
    public void BroadcastStatus()
    {
        var msg = BuildStatus();
        foreach (var s in Snapshot().Where(s => s.Paired)) _ = SendAsync(s, msg);
    }

    private StatusMessage BuildStatus()
    {
        var s = Source;
        return new StatusMessage
        {
            Source = SourceName,
            SimConnected = s is { Kind: "msfs", IsConnected: true },
            Message = s?.StatusText ?? L.T("server.no_source"),
        };
    }

    private async Task AcceptLoopAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            TcpClient tcp;
            try
            {
                tcp = await _listener!.AcceptTcpClientAsync(ct).ConfigureAwait(false);
            }
            catch (Exception) when (ct.IsCancellationRequested)
            {
                return;
            }
            catch (SocketException e)
            {
                Log?.Invoke(L.T("server.accept_error", e.Message));
                continue;
            }
            _ = HandleClientAsync(tcp, ct);
        }
    }

    private async Task HandleClientAsync(TcpClient tcp, CancellationToken serverCt)
    {
        var remoteIp = (tcp.Client.RemoteEndPoint as IPEndPoint)?.Address.MapToIPv4().ToString() ?? "?";
        using var _tcp = tcp;
        tcp.NoDelay = true;
        var stream = tcp.GetStream();

        using var handshakeCts = CancellationTokenSource.CreateLinkedTokenSource(serverCt);
        handshakeCts.CancelAfter(HelloTimeout);
        try
        {
            if (await WebSocketHandshake.AcceptAsync(stream, handshakeCts.Token) != WebSocketHandshake.Outcome.Upgraded)
                return;
        }
        catch (Exception e) when (e is IOException or OperationCanceledException or SocketException)
        {
            return;
        }

        using var ws = WebSocket.CreateFromStream(stream, new WebSocketCreationOptions
        {
            IsServer = true,
            KeepAliveInterval = TimeSpan.FromSeconds(15),
        });
        var session = new Session(ws, remoteIp);
        lock (_sessions) _sessions.Add(session);
        ClientsChanged?.Invoke();
        Log?.Invoke(L.T("server.connection", remoteIp));

        using var sessionCts = CancellationTokenSource.CreateLinkedTokenSource(serverCt);
        var sender = StateLoopAsync(session, sessionCts.Token);
        try
        {
            await ReceiveLoopAsync(session, sessionCts.Token);
        }
        catch (Exception e) when (e is WebSocketException or IOException or OperationCanceledException or ObjectDisposedException)
        {
        }
        finally
        {
            sessionCts.Cancel();
            try { await sender; } catch (OperationCanceledException) { }
            Axes.Release(session.Id);
            lock (_sessions) _sessions.Remove(session);
            ClientsChanged?.Invoke();
            Log?.Invoke(L.T("server.disconnected", session.Name, remoteIp));
            if (ws.State == WebSocketState.Open)
            {
                try
                {
                    using var closeCts = new CancellationTokenSource(1000);
                    await ws.CloseOutputAsync(WebSocketCloseStatus.NormalClosure, "bye", closeCts.Token);
                }
                catch (Exception) { }
            }
        }
    }

    private async Task ReceiveLoopAsync(Session session, CancellationToken ct)
    {
        var buffer = new byte[LinkProtocol.MaxClientMessageBytes];
        while (session.Socket.State == WebSocketState.Open && !ct.IsCancellationRequested)
        {
            var timeout = session.Paired ? IdleTimeout : HelloTimeout - (DateTime.UtcNow - session.ConnectedAt);
            if (timeout <= TimeSpan.Zero)
            {
                await CloseAsync(session, WebSocketCloseStatus.PolicyViolation, "hello timeout");
                return;
            }

            using var readCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            readCts.CancelAfter(timeout);

            var length = 0;
            ValueWebSocketReceiveResult result;
            try
            {
                do
                {
                    if (length == buffer.Length)
                    {
                        await CloseAsync(session, WebSocketCloseStatus.MessageTooBig, "too big");
                        return;
                    }
                    result = await session.Socket.ReceiveAsync(buffer.AsMemory(length), readCts.Token);
                    length += result.Count;
                } while (!result.EndOfMessage);
            }
            catch (OperationCanceledException) when (!ct.IsCancellationRequested)
            {
                Log?.Invoke(L.T("server.silent", session.Name, session.RemoteIp));
                return;
            }

            if (result.MessageType == WebSocketMessageType.Close) return;
            if (result.MessageType != WebSocketMessageType.Text) continue;

            await HandleMessageAsync(session, Encoding.UTF8.GetString(buffer, 0, length), ct);
        }
    }

    internal async Task HandleMessageAsync(Session session, string text, CancellationToken ct)
    {
        var msg = IncomingMessage.Parse(text, out var parseError);
        if (msg == null)
        {
            await SendAsync(session, parseError!);
            return;
        }

        if (msg.Type == "ping")
        {
            await SendAsync(session, new PongMessage
            {
                Id = msg.Id,
                T = msg.Root.TryGetProperty("t", out var t) && t.TryGetInt64(out var tv) ? tv : null,
                ServerT = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            });
            return;
        }

        if (msg.Type == "hello")
        {
            await HandleHelloAsync(session, msg);
            return;
        }

        if (!session.Paired)
        {
            await SendError(session, msg.Id, ErrorCodes.NotPaired, "Najpierw wyślij hello z kodem parowania");
            return;
        }

        switch (msg.Type)
        {
            case "command":
                await HandleCommandAsync(session, msg, ct);
                break;
            case "axis":
                await HandleAxisAsync(session, msg);
                break;
            case "navdata_request":
                await HandleNavdataAsync(session, msg, ct);
                break;
            default:
                await SendError(session, msg.Id, ErrorCodes.UnknownType, $"Nieznany typ wiadomości: {msg.Type}");
                break;
        }
    }

    private async Task HandleHelloAsync(Session session, IncomingMessage msg)
    {
        var result = _pairing.TryPair(session.RemoteIp, msg.GetString("code"), msg.GetString("token"), DateTime.UtcNow);
        if (!result.Ok)
        {
            Log?.Invoke(result.RateLimited
                ? L.T("server.blocked", session.RemoteIp, PairingManager.BlockTime.TotalSeconds.ToString("0"))
                : L.T("server.pair_failed", session.RemoteIp));
            await SendError(session, msg.Id,
                result.RateLimited ? ErrorCodes.RateLimited : ErrorCodes.PairingFailed,
                result.RateLimited ? "Za dużo prób — odczekaj minutę" : "Zły kod parowania lub nieważny token");
            return;
        }

        if (msg.Root.TryGetProperty("client", out var client) && client.ValueKind == JsonValueKind.Object
            && client.TryGetProperty("name", out var name) && name.ValueKind == JsonValueKind.String)
        {
            var n = name.GetString()!.Trim();
            if (n.Length > 0) session.Name = n.Length > 40 ? n[..40] : n;
        }

        session.RateHz = Math.Clamp(msg.GetInt("rateHz") ?? LinkProtocol.DefaultRateHz, LinkProtocol.MinRateHz, LinkProtocol.MaxRateHz);
        session.Paired = true;
        ClientsChanged?.Invoke();
        Log?.Invoke(L.T("server.paired", session.Name, session.RemoteIp, session.RateHz));

        await SendAsync(session, new HelloOkMessage
        {
            Id = msg.Id,
            Token = result.Token!,
            Version = _version,
            Source = SourceName,
            RateHz = session.RateHz,
            Limits = new LimitsInfo { MaxDeflection = Axes.MaxDeflection, AxisTimeoutMs = Axes.TimeoutMs },
        });
        await SendAsync(session, BuildStatus());
    }

    private async Task HandleCommandAsync(Session session, IncomingMessage msg, CancellationToken ct)
    {
        var name = msg.GetString("name");
        JsonElement? value = msg.Root.TryGetProperty("value", out var v) ? v : null;
        var validation = CommandAllowlist.Validate(name, value);
        // Kąty rozglądania to wartości bezwzględne wysyłane strumieniem (jak osie) — poza limitem komend na sekundę
        var cameraLook = validation.Ok && validation.Command!.Spec.Target is CommandTarget.CameraPitch or CommandTarget.CameraYaw;
        if (!cameraLook && !session.TakeCommandToken())
        {
            await SendError(session, msg.Id, ErrorCodes.RateLimited, $"Maksymalnie {MaxCommandsPerSecond} komend na sekundę");
            return;
        }
        if (!validation.Ok)
        {
            Log?.Invoke(L.T("server.rejected", session.Name, name ?? "—", validation.ErrorMessage));
            await SendError(session, msg.Id, validation.ErrorCode!, validation.ErrorMessage!);
            return;
        }

        var source = Source;
        if (source is not { IsConnected: true })
        {
            await SendError(session, msg.Id, ErrorCodes.SimUnavailable, "Brak połączenia z MSFS (włącz symulator albo tryb symulacji)");
            return;
        }

        var cmd = validation.Command!;
        CommandResult result;
        try
        {
            result = await source.ExecuteAsync(cmd, ct);
        }
        catch (Exception e) when (e is not OperationCanceledException)
        {
            result = CommandResult.Fail(ErrorCodes.SimError, e.Message);
        }

        if (result.Ok)
        {
            // Rozglądanie się to kilkanaście komend na sekundę — nie zaśmiecamy nimi dziennika
            if (cmd.Spec.Target is not (CommandTarget.CameraPitch or CommandTarget.CameraYaw))
                Log?.Invoke($"{session.Name}: {cmd.Spec.Name}{(cmd.Spec.NeedsValue ? $" = {cmd.Value}" : "")}");
            await SendAsync(session, new AckMessage { Id = msg.Id, Name = cmd.Spec.Name });
        }
        else
        {
            Log?.Invoke(L.T("server.command_failed", session.Name, cmd.Spec.Name, result.ErrorMessage));
            await SendError(session, msg.Id, result.ErrorCode ?? ErrorCodes.SimError, result.ErrorMessage ?? "Błąd symulatora");
        }
    }

    private async Task HandleNavdataAsync(Session session, IncomingMessage msg, CancellationToken ct)
    {
        if (!session.NavBucket.TryTake())
        {
            await SendError(session, msg.Id, ErrorCodes.RateLimited, $"Maksymalnie {LinkProtocol.MaxNavdataPerSecond} zapytań navdata na sekundę");
            return;
        }

        var query = NavdataQuery.Parse(msg, out var error);
        if (query == null)
        {
            await SendError(session, msg.Id, ErrorCodes.InvalidValue, error!);
            return;
        }

        var source = Source;
        if (source is not { IsConnected: true } || source.NavData is not { } provider)
        {
            await SendError(session, msg.Id, ErrorCodes.SimUnavailable, "Brak połączenia z MSFS (włącz symulator albo tryb symulacji)");
            return;
        }

        // Zapytanie do MSFS może trwać kilka sekund — nie blokuj odbioru kolejnych wiadomości.
        // Najwyżej MaxNavdataInFlight naraz na telefon, reszta czeka w kolejce.
        _ = Task.Run(async () =>
        {
            try
            {
                await session.NavSlots.WaitAsync(ct);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            try
            {
                var data = await _navData.ExecuteAsync(query, provider, source.Kind, ct);
                await SendAsync(session, new NavdataMessage { Id = msg.Id, Kind = query.Kind, Data = data });
            }
            catch (NavDataException e)
            {
                await SendError(session, msg.Id, e.Code, e.Message);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
            }
            catch (Exception e)
            {
                Log?.Invoke($"{session.Name}: navdata {query.Kind} — {e.Message}");
                await SendError(session, msg.Id, ErrorCodes.SimError, e.Message);
            }
            finally
            {
                session.NavSlots.Release();
            }
        }, CancellationToken.None);
    }

    private async Task HandleAxisAsync(Session session, IncomingMessage msg)
    {
        if (msg.GetBool("release"))
        {
            Axes.Release(session.Id);
            return;
        }

        if (!TryAxis(msg, "aileron", out var a) || !TryAxis(msg, "elevator", out var e) || !TryAxis(msg, "rudder", out var r))
        {
            await SendError(session, msg.Id, ErrorCodes.InvalidValue, "Osie muszą być liczbami −1…1");
            return;
        }
        if (!TryAxis(msg, "speedbrake", out var sb) || !TryAxis(msg, "brake", out var br))
        {
            await SendError(session, msg.Id, ErrorCodes.InvalidValue, "Hamulce muszą być liczbami 0…1");
            return;
        }
        if (sb.HasValue || br.HasValue)
        {
            if (Axes.UpdateAux(session.Id, sb, br, Environment.TickCount64) == AxisUpdateResult.Busy)
                await SendError(session, msg.Id, ErrorCodes.NotAllowed, "Hamulcami steruje już inny telefon");
            // Sama wiadomość o hamulcach nie przejmuje lotek, steru wysokości i kierunku
            if (a == null && e == null && r == null) return;
        }

        var wasActive = Axes.Owner == session.Id;
        switch (Axes.Update(session.Id, a, e, r, Environment.TickCount64))
        {
            case AxisUpdateResult.Busy:
                await SendError(session, msg.Id, ErrorCodes.NotAllowed, "Osiami steruje już inny telefon");
                break;
            case AxisUpdateResult.Invalid:
                await SendError(session, msg.Id, ErrorCodes.InvalidValue, "Osie muszą być liczbami −1…1");
                break;
            case AxisUpdateResult.Ok when !wasActive:
                Log?.Invoke(L.T("server.takes_axes", session.Name));
                break;
        }
    }

    private static bool TryAxis(IncomingMessage msg, string name, out double? value)
    {
        value = null;
        if (!msg.Root.TryGetProperty(name, out var el) || el.ValueKind == JsonValueKind.Null) return true;
        if (el.ValueKind != JsonValueKind.Number || !el.TryGetDouble(out var d) || !double.IsFinite(d)) return false;
        value = d;
        return true;
    }

    private async Task StateLoopAsync(Session session, CancellationToken ct)
    {
        // PeriodicTimer nie kumuluje opóźnień jak Task.Delay (rozdzielczość zegara Windows ~15,6 ms).
        using var timer = new PeriodicTimer(TimeSpan.FromMilliseconds(1000.0 / LinkProtocol.MaxRateHz));
        var credit = 0;
        try
        {
            while (await timer.WaitForNextTickAsync(ct))
            {
                if (!session.Paired) continue;
                // Zegar tyka z maksymalną częstotliwością; wysyłamy co (Max/RateHz) tyknięć, średnio dokładnie RateHz.
                credit += session.RateHz;
                if (credit < LinkProtocol.MaxRateHz) continue;
                credit -= LinkProtocol.MaxRateHz;
                var snapshot = Source is { IsConnected: true } src ? src.GetSnapshot() : null;
                if (snapshot == null) continue;
                await SendAsync(session, new StateMessage
                {
                    Seq = Interlocked.Increment(ref _seq),
                    T = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                    Data = snapshot,
                });
            }
        }
        catch (OperationCanceledException)
        {
        }
    }

    private Task SendError(Session s, string? id, string code, string message) =>
        SendAsync(s, new ErrorMessage { Id = id, Code = code, Message = message });

    private static async Task SendAsync<T>(Session session, T message) where T : LinkMessage
    {
        var bytes = Encoding.UTF8.GetBytes(LinkJson.Serialize(message));
        await session.SendLock.WaitAsync();
        try
        {
            if (session.Socket.State != WebSocketState.Open) return;
            using var cts = new CancellationTokenSource(5000);
            await session.Socket.SendAsync(bytes, WebSocketMessageType.Text, true, cts.Token);
        }
        catch (Exception e) when (e is WebSocketException or OperationCanceledException or ObjectDisposedException or IOException)
        {
            session.Socket.Abort();
        }
        finally
        {
            session.SendLock.Release();
        }
    }

    private static async Task CloseAsync(Session s, WebSocketCloseStatus status, string reason)
    {
        try
        {
            using var cts = new CancellationTokenSource(1000);
            await s.Socket.CloseOutputAsync(status, reason, cts.Token);
        }
        catch (Exception) { }
    }

    private List<Session> Snapshot()
    {
        lock (_sessions) return _sessions.ToList();
    }

    public void Dispose()
    {
        _cts.Cancel();
        _listener?.Stop();
        foreach (var s in Snapshot()) s.Socket.Abort();
    }

    internal sealed class Session
    {
        private readonly TokenBucket _commands = new(MaxCommandsPerSecond);

        public Session(WebSocket socket, string remoteIp)
        {
            Socket = socket;
            RemoteIp = remoteIp;
            Name = remoteIp;
        }

        public string Id { get; } = Guid.NewGuid().ToString("N");
        public WebSocket Socket { get; }
        public string RemoteIp { get; }
        public string Name { get; set; }
        public DateTime ConnectedAt { get; } = DateTime.UtcNow;
        public volatile bool Paired;
        public int RateHz { get; set; } = LinkProtocol.DefaultRateHz;
        public SemaphoreSlim SendLock { get; } = new(1, 1);

        public TokenBucket NavBucket { get; } = new(LinkProtocol.MaxNavdataPerSecond);
        public SemaphoreSlim NavSlots { get; } = new(MaxNavdataInFlight, MaxNavdataInFlight);

        public bool TakeCommandToken() => _commands.TryTake();
    }
}
