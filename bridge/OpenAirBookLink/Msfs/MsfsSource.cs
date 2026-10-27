using System.Runtime.InteropServices;
using OpenAirBookLink.Core.Control;
using OpenAirBookLink.Core.NavData;
using OpenAirBookLink.Core.Protocol;
using OpenAirBookLink.Core.Sources;
using SimConnect.NET;
using SimConnect.NET.SimVar;

namespace OpenAirBookLink.Msfs;

// Połączenie z MSFS 2024 przez SimConnect (pakiet SimConnect.NET z natywnym SimConnect.dll).
// Samo łączy się ponownie, gdy symulator zostanie zamknięty lub uruchomiony później.
public sealed class MsfsSource : IFlightSource
{
    private const uint PauseEventId = 9001;
    private const uint FirstClientEventId = 10000;
    private const uint UserAircraft = 0;
    private const uint PriorityHighest = 1;
    private const double LbsToKg = 0.45359237;
    private const int AxisRange = 16383;

    // Kierunek osi w zdarzeniach AXIS_*: ujemny aileron = w prawo, ujemny elevator = nos w górę.
    // Do potwierdzenia na żywym MSFS 2024 — zmiana znaku tylko tutaj.
    private const int AileronSign = -1;
    private const int ElevatorSign = -1;
    private const int RudderSign = 1;

    private readonly object _lock = new();
    private readonly Dictionary<string, uint> _eventIds = new(StringComparer.Ordinal);
    private readonly MsfsNavData _navData = new();
    private readonly TouchdownLatch _touchdown = new();
    private TouchdownData? _lastTouchdown;
    private CancellationTokenSource? _cts;
    private SimConnectClient? _client;
    private ISimVarSubscription? _subscription;
    private MsfsData? _last;
    private bool _paused;
    private string _status = "Czekam na MSFS…";
    private bool _connected;
    private string? _lastError;

    // Martwe połączenie: po restarcie MSFS lub zerwaniu potoku biblioteka nadal zgłasza IsConnected,
    // a każde wywołanie kończy się E_FAIL. Po tylu kolejnych błędach łączymy się od nowa.
    private const int FailLimit = 8;
    private int _failStreak;

    public MsfsSource(AxisController axes)
    {
        Axes = axes;
        uint id = FirstClientEventId;
        foreach (var spec in CommandAllowlist.All.Where(s => s.Target == CommandTarget.Event)) _eventIds[spec.Name] = id++;
        foreach (var axis in CommandAllowlist.AxisEvents) _eventIds[axis] = id++;
        foreach (var ev in CameraEvents) _eventIds[ev] = id++;
        _navData.Log += m => Log?.Invoke(m);
    }

    public string Kind => "msfs";
    public bool IsConnected => _connected && _last != null;
    public string StatusText => _status;
    public AxisController Axes { get; }
    public INavDataProvider NavData => _navData;

    public event Action? StatusChanged;
    public event Action<string>? Log;

    public void Start()
    {
        if (_cts != null) return;
        _cts = new CancellationTokenSource();
        _ = RunAsync(_cts.Token);
    }

    public FlightState? GetSnapshot()
    {
        MsfsData d;
        bool paused;
        TouchdownData? touchdown;
        lock (_lock)
        {
            if (_last == null) return null;
            d = _last.Value;
            paused = _paused;
            touchdown = Copy(_lastTouchdown);
        }
        var state = Map(d, paused, Axes.IsActive);
        state.Touchdown = touchdown;
        return state;
    }

    public async Task<CommandResult> ExecuteAsync(ValidatedCommand command, CancellationToken ct)
    {
        var client = _client;
        if (client is not { IsConnected: true }) return CommandResult.Fail(ErrorCodes.SimUnavailable, "Brak połączenia z MSFS");
        try
        {
            var result = await ExecuteOnAsync(client, command, ct);
            Interlocked.Exchange(ref _failStreak, 0);
            return result;
        }
        catch (Exception e) when (e is not OperationCanceledException)
        {
            SimCallFailed();
            throw;
        }
    }

    private void SimCallFailed() => Interlocked.Increment(ref _failStreak);

    private async Task<CommandResult> ExecuteOnAsync(SimConnectClient client, ValidatedCommand command, CancellationToken ct)
    {
        if (command.Spec.Target is CommandTarget.CameraPitch or CommandTarget.CameraYaw)
            return await LookAsync(client, command.Spec.Target == CommandTarget.CameraPitch, command.Value, ct);

        if (command.Spec.Target == CommandTarget.CameraState)
        {
            return await SetCameraAsync(client, (int)command.Value, ct);
        }

        if (command.Spec.Target == CommandTarget.TransponderState)
        {
            await client.SimVars.SetAsync("TRANSPONDER STATE:1", "Enum", (double)command.Value, cancellationToken: ct);
            return CommandResult.Success;
        }

        await TransmitAsync(client, command.Spec.Name, command.Data, ct);
        return CommandResult.Success;
    }

    private async Task RunAsync(CancellationToken ct)
    {
        var axisTask = AxisLoopAsync(ct);
        while (!ct.IsCancellationRequested)
        {
            try
            {
                await ConnectAsync(ct);
                while (!ct.IsCancellationRequested && _client is { IsConnected: true } && Volatile.Read(ref _failStreak) < FailLimit)
                    await Task.Delay(1000, ct);
                if (!ct.IsCancellationRequested)
                    Log?.Invoke(Volatile.Read(ref _failStreak) >= FailLimit
                        ? "MSFS nie przyjmuje komend (martwe połączenie SimConnect) — łączę ponownie"
                        : "MSFS rozłączony");
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                break;
            }
            catch (DllNotFoundException e)
            {
                SetStatus(false, "Brak SimConnect.dll: " + e.Message);
            }
            catch (Exception e)
            {
                // MSFS nie działa albo jeszcze się ładuje — normalny stan, spróbuj za chwilę.
                SetStatus(false, "Czekam na MSFS…");
                if (e.Message != _lastError) Log?.Invoke($"MSFS niedostępny — ponawiam co 5 s ({e.Message})");
                _lastError = e.Message;
            }

            await DisconnectAsync();
            try { await Task.Delay(TimeSpan.FromSeconds(5), ct); }
            catch (OperationCanceledException) { break; }
        }
        await DisconnectAsync();
        try { await axisTask; } catch (OperationCanceledException) { }
    }

    private async Task ConnectAsync(CancellationToken ct)
    {
        Interlocked.Exchange(ref _failStreak, 0);
        // Wyniki z poprzedniego (np. martwego) połączenia nie obowiązują
        _look6DofFailed = false; _lookActive = false; CancelLookRelease();
        var client = new SimConnectClient("OpenTakeoff Link");
        _client = client;
        await client.ConnectAsync(cancellationToken: ct);

        foreach (var (name, id) in _eventIds)
        {
            var hr = Native.SimConnect_MapClientEventToSimEvent(client.Handle, id, name);
            if (hr < 0) Log?.Invoke($"Nie zmapowano zdarzenia {name}: 0x{hr:X8}");
        }

        client.SystemEventReceived += (_, e) =>
        {
            if (e.EventId != PauseEventId) return;
            lock (_lock) _paused = e.Data != 0;
        };
        await client.SubscribeToEventAsync("Pause", PauseEventId, ct);

        _subscription = client.SimVars.Subscribe<MsfsData>(SimConnectPeriod.VisualFrame, data =>
        {
            var first = false;
            lock (_lock)
            {
                first = _last == null;
                _last = data;
                _lastTouchdown = _touchdown.Update(TouchdownSampleOf(data));
            }
            if (first) StatusChanged?.Invoke();
        }, cancellationToken: ct);

        try
        {
            _navData.Attach(client);
        }
        catch (Exception e) when (e is InvalidOperationException or EntryPointNotFoundException)
        {
            Log?.Invoke($"Dane nawigacyjne MSFS niedostępne: {e.Message}");
        }

        var sim = client.IsMSFS2024 ? "MSFS 2024" : "MSFS";
        _lastError = null;
        SetStatus(true, $"Połączono z {sim}");
        Log?.Invoke($"Połączono z {sim} przez SimConnect");
    }

    private async Task DisconnectAsync()
    {
        var client = _client;
        _client = null;
        _navData.Detach();
        _subscription?.Dispose();
        _subscription = null;
        lock (_lock) _last = null;
        if (_connected) SetStatus(false, "Czekam na MSFS…");
        if (client == null) return;
        try
        {
            await client.DisconnectAsync();
        }
        catch (Exception)
        {
        }
        client.Dispose();
    }

    // Co 50 ms: wychylenia z telefonu (lub jednorazowy neutral po auto-neutralu) do MSFS.
    private async Task AxisLoopAsync(CancellationToken ct)
    {
        using var timer = new PeriodicTimer(TimeSpan.FromMilliseconds(50));
        while (await timer.WaitForNextTickAsync(ct))
        {
            var output = Axes.Tick(Environment.TickCount64);
            var aux = Axes.TickAux(Environment.TickCount64);
            var client = _client;
            if ((output == null && aux == null) || client is not { IsConnected: true }) continue;
            try
            {
                if (output != null)
                {
                    await TransmitAsync(client, "AXIS_AILERONS_SET", ToAxis(output.Aileron * AileronSign), ct);
                    await TransmitAsync(client, "AXIS_ELEVATOR_SET", ToAxis(output.Elevator * ElevatorSign), ct);
                    await TransmitAsync(client, "AXIS_RUDDER_SET", ToAxis(output.Rudder * RudderSign), ct);
                }
                if (aux?.Speedbrake is { } sb) await TransmitAsync(client, "AXIS_SPOILER_SET", ToLever(sb), ct);
                if (aux?.Brake is { } br)
                {
                    // Hamulce kół tylko na ziemi — w powietrzu zawsze puszczone
                    bool onGround;
                    lock (_lock) onGround = _last is { } d && B(d.OnGround);
                    var v = ToLever(onGround ? br : 0);
                    await TransmitAsync(client, "AXIS_LEFT_BRAKE_SET", v, ct);
                    await TransmitAsync(client, "AXIS_RIGHT_BRAKE_SET", v, ct);
                }
                Interlocked.Exchange(ref _failStreak, 0);
            }
            catch (Exception e) when (e is not OperationCanceledException)
            {
                SimCallFailed();
                Log?.Invoke($"Osie: {e.Message}");
            }
        }
    }

    private static uint ToAxis(double v) => unchecked((uint)(int)Math.Round(Math.Clamp(v, -1, 1) * AxisRange));

    // Dźwignia/pedał 0…1 na pełny zakres osi: −16383 = schowane / puszczone, 16383 = do końca.
    private static uint ToLever(double v) => unchecked((uint)(int)Math.Round((Math.Clamp(v, 0, 1) * 2 - 1) * AxisRange));

    // Biblioteka serializuje wywołania na uchwycie SimConnect i rzuca wyjątek przy błędzie HRESULT.
    private Task TransmitAsync(SimConnectClient client, string eventName, uint data, CancellationToken ct) =>
        client.InputEvents.TransmitClientEventAsync(UserAircraft, _eventIds[eventName], data, PriorityHighest,
            SimConnectEventOptions.GroupIdIsPriority, ct);

    // ---------- rozglądanie się ----------

    // Zdarzenia widoku — zapas, gdy CameraSetRelative6DOF jest niedostępne (wewnętrzne, spoza listy telefonu)
    private static readonly string[] CameraEvents = { "PAN_VIEW", "PAN_RESET_COCKPIT", "AXIS_PAN_HEADING", "AXIS_PAN_PITCH", "VIEW_MODE" };
    // CAMERA STATE: 2 kabina, 3 zewnętrzna, 4 dron, 7 kamera 6DOF (włączana przez CameraSetRelative6DOF)
    private const int CamCockpit = 2, CamExternal = 3, CamDrone = 4, CamSixDof = 7;
    // Po tylu ms bez komend rozglądania (telefon śle je co 30 ms podczas przytrzymania) kamera wraca do widoku sprzed rozglądania
    private static readonly TimeSpan LookReleaseDelay = TimeSpan.FromMilliseconds(400);
    // Poza kabiną kamera krąży wokół samolotu w tej odległości (m) od punktu oka
    private const float OrbitRadiusM = 45f, OrbitBaseElevationDeg = 12f;
    private bool _look6DofFailed, _lookActive;
    private long _lookYaw, _lookPitch;
    // Widok, z którego zaczęło się rozglądanie — do niego kamera wraca po puszczeniu przycisku
    private int _lookOrigin = CamCockpit;
    private CancellationTokenSource? _lookRelease;
    private readonly SemaphoreSlim _cameraGate = new(1, 1);

    /// <summary>
    /// Rozglądanie się w każdym widoku. MSFS 2024 ignoruje zapis CAMERA GAMEPLAY PITCH YAW i zdarzenia PAN_*
    /// (sprawdzone na żywo), więc kamerę obraca CameraSetRelative6DOF. W kabinie obraca się wokół oka pilota
    /// (przesunięcie 0 — FLT_MAX MSFS 2024 bierze dosłownie i stawia kamerę przed samolotem); w widoku zewnętrznym,
    /// z drona i innych kamera krąży wokół samolotu (orbita o promieniu OrbitRadiusM), zamiast wskakiwać do kabiny.
    /// Ta funkcja przełącza MSFS w osobny tryb kamery (CAMERA STATE 7); do widoku sprzed rozglądania wracamy dopiero
    /// po puszczeniu przycisku (brak komend przez LookReleaseDelay), a nie przy każdym przejściu przez „na wprost”.
    /// </summary>
    private async Task<CommandResult> LookAsync(SimConnectClient client, bool pitchAxis, long valueDeg, CancellationToken ct)
    {
        CancelLookRelease();
        await _cameraGate.WaitAsync(ct);
        try
        {
            if (pitchAxis) _lookPitch = valueDeg; else _lookYaw = valueDeg;
            var centered = Math.Abs(_lookYaw) < 1 && Math.Abs(_lookPitch) < 1;

            if (!_look6DofFailed)
            {
                if (!_lookActive)
                {
                    if (centered) return CommandResult.Success;
                    var raw = RawCameraState();
                    // Kamera 7 bez aktywnego rozglądania = MSFS utknął po poprzednim — traktujemy jak kabinę
                    _lookOrigin = raw is CamSixDof or < 0 ? CamCockpit : raw;
                    Log?.Invoke($"Kamera: rozglądanie (MSFS zgłasza {raw}, powrót do {_lookOrigin})");
                }
                var (dx, dy, dz, camPitch) = _lookOrigin == CamCockpit ? (0f, 0f, 0f, (float)-_lookPitch) : Orbit(_lookYaw, _lookPitch);
                var hr = Native.SimConnect_CameraSetRelative6DOF(client.Handle, dx, dy, dz, camPitch, 0f, _lookYaw);
                if (hr >= 0)
                {
                    _lookActive = true;
                    if (centered) ScheduleLookRelease(client);
                    return CommandResult.Success;
                }
                _look6DofFailed = true;
                Log?.Invoke($"Kamera: CameraSetRelative6DOF niedostępne (0x{hr:X8}) — rozglądanie przez zdarzenia widoku");
            }

            // Zapas: zdarzenia widoku
            if (centered)
            {
                await TransmitAsync(client, "PAN_RESET_COCKPIT", 0, ct);
                return CommandResult.Success;
            }
            if (!pitchAxis) await TransmitAsync(client, "PAN_VIEW", (uint)(((_lookYaw % 360) + 360) % 360), ct);
            await TransmitAsync(client, pitchAxis ? "AXIS_PAN_PITCH" : "AXIS_PAN_HEADING", unchecked((uint)(int)(pitchAxis ? -_lookPitch : _lookYaw)), ct);
            return CommandResult.Success;
        }
        finally
        {
            _cameraGate.Release();
        }
    }

    private void CancelLookRelease() => Interlocked.Exchange(ref _lookRelease, null)?.Cancel();

    // Powrót do zwykłej kabiny po puszczeniu przycisku; kolejna komenda rozglądania go odwołuje.
    private void ScheduleLookRelease(SimConnectClient client)
    {
        var cts = new CancellationTokenSource();
        Interlocked.Exchange(ref _lookRelease, cts)?.Cancel();
        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(LookReleaseDelay, cts.Token);
                await _cameraGate.WaitAsync(cts.Token);
            }
            catch (OperationCanceledException) { return; }
            try
            {
                if (_lookActive && client.IsConnected) await RestoreCameraAsync(client, _lookOrigin, CancellationToken.None);
            }
            catch (Exception e)
            {
                Log?.Invoke($"Kamera: powrót do widoku nie powiódł się ({e.Message})");
            }
            finally
            {
                _cameraGate.Release();
            }
        });
    }

    private int RawCameraState()
    {
        lock (_lock) return _last is { } d ? (int)d.CameraState : -1;
    }

    /// <summary>
    /// Zmiana kamery (kabina / zewnętrzna / dron). Dotknięcie widoku, który już jest, nic nie robi. Po rozglądaniu
    /// MSFS siedzi w kamerze 6DOF i nie zawsze przyjmuje od razu nowy CAMERA STATE — wtedy przez inny widok.
    /// </summary>
    private async Task<CommandResult> SetCameraAsync(SimConnectClient client, int target, CancellationToken ct)
    {
        CancelLookRelease();
        await _cameraGate.WaitAsync(ct);
        try
        {
            var raw = RawCameraState();
            Log?.Invoke($"Kamera: zmiana na {target} (MSFS zgłasza {raw}, rozglądanie {(_lookActive ? "aktywne" : "nie")})");
            if (_lookActive || raw == CamSixDof) await RestoreCameraAsync(client, target, ct);
            else if (raw != target) await WriteCameraAsync(client, target, ct);
            return CommandResult.Success;
        }
        finally
        {
            _cameraGate.Release();
        }
    }

    /// <summary>
    /// Kamera krążąca wokół samolotu: położenie na sferze o promieniu OrbitRadiusM (x w prawo, y w górę, z do przodu),
    /// patrząca na samolot. Obrót w prawo przesuwa kamerę w lewo i za samolot; „w górę” obniża kamerę.
    /// Zwraca przesunięcie i pochylenie kamery (+ = w dół) dla CameraSetRelative6DOF.
    /// </summary>
    internal static (float dx, float dy, float dz, float pitch) Orbit(long yawDeg, long pitchDeg)
    {
        var elev = Math.Clamp(OrbitBaseElevationDeg - pitchDeg, -10f, 80f) * Math.PI / 180;
        var yaw = yawDeg * Math.PI / 180;
        var horiz = OrbitRadiusM * Math.Cos(elev);
        return ((float)(-horiz * Math.Sin(yaw)), (float)(OrbitRadiusM * Math.Sin(elev)), (float)(-horiz * Math.Cos(yaw)), (float)(elev * 180 / Math.PI));
    }

    /// <summary>
    /// Wyjście z kamery 6DOF do [target]. MSFS 2024 często nie przyjmuje wprost zapisu CAMERA STATE z kamery 7
    /// (dziennik: kabina i widok zewnętrzny odrzucane po kilka razy), a wychodzi z niej przez drona — więc:
    /// wprost, potem przez drona, a do kabiny jeszcze przez widok zewnętrzny. Wymaga _cameraGate.
    /// </summary>
    private async Task RestoreCameraAsync(SimConnectClient client, int target, CancellationToken ct)
    {
        _lookActive = false;
        _lookYaw = 0; _lookPitch = 0;
        if (await TryWriteCameraAsync(client, target, ct)) return;
        if (target != CamDrone) await TryWriteCameraAsync(client, CamDrone, ct, attempts: 40);
        if (await TryWriteCameraAsync(client, target, ct, attempts: 40)) return;
        if (target == CamCockpit && await TryWriteCameraAsync(client, CamExternal, ct, attempts: 40)
            && await TryWriteCameraAsync(client, CamCockpit, ct, attempts: 40)) return;
        Log?.Invoke($"Kamera: MSFS nie wrócił do widoku {target} (zgłasza {RawCameraState()})");
    }

    private async Task WriteCameraAsync(SimConnectClient client, int target, CancellationToken ct)
    {
        if (await TryWriteCameraAsync(client, target, ct)) return;
        // Zapas: część wersji MSFS ignoruje zapis CAMERA STATE bez błędu. Między kabiną a widokiem zewnętrznym
        // przełącza wtedy zdarzenie VIEW_MODE (jak klawisz zmiany widoku) — tylko gdy MSFS jest w jednym z nich,
        // bo z innych kamer (np. 6DOF) VIEW_MODE wędruje po widokach nieprzewidywalnie.
        if (target is CamCockpit or CamExternal && RawCameraState() is CamCockpit or CamExternal)
        {
            await TransmitAsync(client, "VIEW_MODE", 0, ct);
            if (await WaitForCameraAsync(target, ct)) return;
        }
        Log?.Invoke($"Kamera: MSFS nie przełączył widoku na {target} (zgłasza {RawCameraState()})");
    }

    private async Task<bool> TryWriteCameraAsync(SimConnectClient client, int target, CancellationToken ct, int attempts = 20)
    {
        try
        {
            await client.SimVars.SetAsync("CAMERA STATE", "Enum", (double)target, cancellationToken: ct);
            return await WaitForCameraAsync(target, ct, attempts);
        }
        catch (Exception e) when (e is not OperationCanceledException)
        {
            SimCallFailed();
            Log?.Invoke($"Kamera: zapis CAMERA STATE nie powiódł się ({e.Message})");
            return false;
        }
    }

    // MSFS 2024 potrafi przełączać widok dłużej niż kilka klatek — czekamy do ok. 0,8 s (z kamery 7 dłużej)
    private async Task<bool> WaitForCameraAsync(int target, CancellationToken ct, int attempts = 20)
    {
        for (var i = 0; i < attempts; i++)
        {
            await Task.Delay(40, ct);
            if (RawCameraState() == target) return true;
        }
        return false;
    }


    private void SetStatus(bool connected, string text)
    {
        var changed = connected != _connected || text != _status;
        _connected = connected;
        _status = text;
        if (changed) StatusChanged?.Invoke();
    }

    private static TouchdownSample TouchdownSampleOf(in MsfsData d) => new(
        Environment.TickCount64, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), B(d.OnGround),
        d.Vs, d.GForce, -d.Pitch, -d.Bank, d.HdgTrue, d.Lat, d.Lon, d.Ias, d.Gs,
        TdVsFpm: d.TdVs, TdPitchDeg: -d.TdPitch, TdBankDeg: -d.TdBank, TdLat: d.TdLat, TdLon: d.TdLon);

    private static TouchdownData? Copy(TouchdownData? t) => t == null ? null : new TouchdownData
    {
        Seq = t.Seq, T = t.T, VsFpm = t.VsFpm, GForce = t.GForce, PitchDeg = t.PitchDeg, BankDeg = t.BankDeg,
        HeadingTrueDeg = t.HeadingTrueDeg, Lat = Math.Round(t.Lat, 6), Lon = Math.Round(t.Lon, 6), IasKt = t.IasKt, GsKt = t.GsKt,
    };

    internal static FlightState Map(MsfsData d, bool paused, bool axesActive)
    {
        var lbsPerGal = d.FuelLbsPerGal > 0 ? d.FuelLbsPerGal : 6.7;
        var engines = new List<EngineData>();
        var count = Math.Clamp((int)d.EngineCount, 0, 4);
        var raw = new[]
        {
            (d.N1_1, d.N2_1, d.Rpm1, d.Egt1, d.Ff1, d.Throttle1), (d.N1_2, d.N2_2, d.Rpm2, d.Egt2, d.Ff2, d.Throttle2),
            (d.N1_3, d.N2_3, d.Rpm3, d.Egt3, d.Ff3, d.Throttle3), (d.N1_4, d.N2_4, d.Rpm4, d.Egt4, d.Ff4, d.Throttle4),
        };
        for (var i = 0; i < count; i++)
        {
            var (n1, n2, rpm, egt, ff, thr) = raw[i];
            engines.Add(new EngineData
            {
                N1Pct = R(n1, 1), N2Pct = R(n2, 1), Rpm = R(rpm, 0), EgtC = R(egt, 0), FuelFlowKgH = R(ff * LbsToKg, 0),
                ThrottlePct = R(thr, 0),
            });
        }

        DateTime? simTime = null;
        if (d.ZuluYear >= 1900 && d.ZuluDayOfYear >= 1)
        {
            simTime = new DateTime((int)d.ZuluYear, 1, 1, 0, 0, 0, DateTimeKind.Utc)
                .AddDays(d.ZuluDayOfYear - 1).AddSeconds(Math.Floor(d.ZuluTime));
        }

        return new FlightState
        {
            Source = "msfs",
            Aircraft = Aircraft(d, paused),
            SimTimeUtc = simTime,
            Position = new PositionData { Lat = d.Lat, Lon = d.Lon, AltMslFt = R(d.AltMsl, 0), AltAglFt = R(d.AltAgl, 0), AltIndFt = R(d.AltInd, 0) },
            Speed = new SpeedData { IasKt = R(d.Ias, 1), TasKt = R(d.Tas, 1), GsKt = R(d.Gs, 1), Mach = R(d.Mach, 3), VsFpm = R(d.Vs, 0) },
            // MSFS: pochylenie dodatnie = nos w dół, przechylenie dodatnie = w lewo → odwracamy.
            Attitude = new AttitudeData
            {
                PitchDeg = R(-d.Pitch, 1), BankDeg = R(-d.Bank, 1),
                HdgTrueDeg = R(d.HdgTrue, 1), HdgMagDeg = R(d.HdgMag, 1), TrackTrueDeg = R(d.Track, 1),
            },
            Autopilot = new AutopilotData
            {
                Master = B(d.ApMaster), Fd = B(d.ApFd), Athr = B(d.ApAthrArm) || B(d.ApAthrActive),
                HdgDeg = R(d.ApHdg, 0), AltFt = R(d.ApAlt, 0), VsFpm = R(d.ApVs, 0), SpdKt = R(d.ApSpd, 0),
                Modes = new AutopilotModes
                {
                    Hdg = B(d.ApHdgMode), Nav = B(d.ApNavMode),
                    Apr = B(d.ApAprMode) && !B(d.ApAprIsLoc), Loc = B(d.ApAprMode) && B(d.ApAprIsLoc),
                    Alt = B(d.ApAltMode), Vs = B(d.ApVsMode), Flc = B(d.ApFlcMode),
                },
            },
            Radios = new RadiosData
            {
                Com1 = new RadioPair { ActiveMhz = R(d.Com1Active, 3), StandbyMhz = R(d.Com1Standby, 3) },
                Com2 = new RadioPair { ActiveMhz = R(d.Com2Active, 3), StandbyMhz = R(d.Com2Standby, 3) },
                Nav1 = new RadioPair { ActiveMhz = R(d.Nav1Active, 3), StandbyMhz = R(d.Nav1Standby, 3) },
                Nav2 = new RadioPair { ActiveMhz = R(d.Nav2Active, 3), StandbyMhz = R(d.Nav2Standby, 3) },
                Transponder = new TransponderData
                {
                    Code = CommandAllowlist.DecodeSquawk((uint)d.XpdrCode),
                    Mode = TransponderData.ModeName((int)d.XpdrState),
                    Ident = B(d.XpdrIdent),
                },
            },
            Controls = new ControlsData
            {
                GearDown = B(d.GearHandle), GearPct = R(d.GearPct, 0),
                FlapsIndex = (int)d.FlapsIndex, FlapsPositions = (int)d.FlapsPositions + 1, FlapsPct = R(d.FlapsPct, 0),
                SpoilersPct = R(d.SpoilersPct, 0), SpoilersArmed = B(d.SpoilersArmed),
                ParkingBrake = B(d.ParkingBrake), ElevatorTrimPct = R(d.ElevTrimPct, 1),
                ThrottlePct = R(d.Throttle1, 0), MixturePct = R(d.Mixture1, 0), PropPct = R(d.Prop1, 0), CameraState = (int)d.CameraState,
                CameraPitchDeg = R(-d.CamPitchRad * 180 / Math.PI, 0), CameraYawDeg = R(d.CamYawRad * 180 / Math.PI, 0),
            },
            Lights = new LightsData
            {
                Beacon = B(d.LightBeacon), Strobe = B(d.LightStrobe), Nav = B(d.LightNav), Landing = B(d.LightLanding),
                Taxi = B(d.LightTaxi), Logo = B(d.LightLogo), Wing = B(d.LightWing), Panel = B(d.LightPanel), Cabin = B(d.LightCabin),
            },
            Fuel = new FuelData
            {
                TotalKg = R(d.FuelTotalLbs * LbsToKg, 0),
                Tanks = new FuelTanks
                {
                    LeftKg = R(d.FuelLeftGal * lbsPerGal * LbsToKg, 0),
                    CenterKg = R(d.FuelCenterGal * lbsPerGal * LbsToKg, 0),
                    RightKg = R(d.FuelRightGal * lbsPerGal * LbsToKg, 0),
                },
            },
            Engines = engines,
            Environment = new EnvironmentData { WindDirDeg = R(d.WindDir, 0), WindSpdKt = R(d.WindSpd, 0), OatC = R(d.Oat, 1), QnhHpa = R(d.Qnh, 1) },
            Axes = new AxesData { Active = axesActive },
            Avionics = Avionics(d),
        };
    }

    // GPS WP BEARING / DESIRED TRACK są rzeczywiste — na wyświetlacze magnetyczne przez MAGVAR (E +).
    private static double Mag(double trueDeg, double magVar) => R(((trueDeg - magVar) % 360 + 360) % 360, 1);

    internal static AvionicsData Avionics(in MsfsData d)
    {
        var magVar = d.MagVar > 180 ? d.MagVar - 360 : d.MagVar;
        return new AvionicsData
        {
            BaroHpa = R(d.Qnh, 1), BaroStd = B(d.BaroStd), PressureAltFt = R(d.PressureAlt, 0),
            RadioAltFt = R(d.RadioAlt, 0), DecisionHeightFt = R(d.DecisionHeight, 0),
            // Kulka: ±128 = pełne wychylenie
            SlipBall = R(Math.Clamp(d.SlipBall / 128.0, -1, 1), 3), TurnRateDps = R(d.TurnRate, 2),
            GForce = R(d.GForce, 2), AoaDeg = R(d.Aoa, 1),
            // Jak PLANE PITCH/BANK: w MSFS dodatnie = nos w dół / w lewo
            AttIndPitchDeg = R(-d.AttIndPitch, 1), AttIndBankDeg = R(-d.AttIndBank, 1), HdgIndDeg = R(d.HdgInd, 1),
            SuctionInHg = R(d.Suction, 2), MagVarDeg = R(magVar, 1), TatC = R(d.Tat, 1),
            Speeds = new SpeedLimits { Vs0Kt = R(d.Vs0, 0), Vs1Kt = R(d.Vs1, 0), VcKt = R(d.Vc, 0), VmoKt = R(d.BarberPole, 0) },
            StallWarning = B(d.StallWarning), OverspeedWarning = B(d.OverspeedWarning),
            Marker = (int)d.Marker switch { 1 => "outer", 2 => "middle", 3 => "inner", _ => "none" },
            Nav1 = new NavReceiver
            {
                HasNav = B(d.Nav1Has), HasLoc = B(d.Nav1HasLoc), HasGs = B(d.Nav1HasGs), HasDme = B(d.Nav1HasDme),
                Ident = AircraftClassifier.CleanAtcString(d.Nav1Ident), ObsDeg = R(d.Nav1Obs, 0),
                LocCourseDeg = R(d.Nav1LocCourse, 0), RadialDeg = R(d.Nav1Radial, 0),
                // CDI ±127 i GSI ±119 = pełne wychylenie (2,5 kropki)
                CdiDots = Dots(d.Nav1Cdi, 127), GsDots = Dots(d.Nav1Gsi, 119),
                DmeNm = R(d.Nav1Dme, 1), ToFrom = ToFrom(d.Nav1ToFrom),
            },
            Nav2 = new NavReceiver
            {
                HasNav = B(d.Nav2Has), HasLoc = B(d.Nav2HasLoc), HasDme = B(d.Nav2HasDme),
                Ident = AircraftClassifier.CleanAtcString(d.Nav2Ident), ObsDeg = R(d.Nav2Obs, 0), RadialDeg = R(d.Nav2Radial, 0),
                CdiDots = Dots(d.Nav2Cdi, 127), DmeNm = R(d.Nav2Dme, 1), ToFrom = ToFrom(d.Nav2ToFrom),
            },
            Adf = new AdfReceiver { HasSignal = d.AdfSignal > 0, RelBearingDeg = R(d.AdfRadial, 0) },
            Gps = new GpsData
            {
                ActiveFlightPlan = B(d.GpsActivePlan), ApproachActive = B(d.GpsApproach), DrivesNav1 = B(d.GpsDrivesNav1),
                NextId = AircraftClassifier.CleanAtcString(d.GpsNextId), PrevId = AircraftClassifier.CleanAtcString(d.GpsPrevId),
                NextDistNm = R(d.GpsNextDist, 1), NextBearingMagDeg = Mag(d.GpsNextBearingTrue, magVar),
                DesiredTrackMagDeg = Mag(d.GpsDtkTrue, magVar), CrossTrackNm = R(d.GpsXtk, 2),
                NextEteSec = R(d.GpsNextEte, 0), DestEteSec = R(d.GpsDestEte, 0),
                NextLat = R(d.GpsNextLat, 6), NextLon = R(d.GpsNextLon, 6), PrevLat = R(d.GpsPrevLat, 6), PrevLon = R(d.GpsPrevLon, 6),
                WpIndex = (int)d.GpsWpIndex, WpCount = (int)d.GpsWpCount,
            },
            Fd = new FlightDirector { Active = B(d.ApFd), PitchDeg = R(-d.FdPitch, 1), BankDeg = R(-d.FdBank, 1) },
            Ap = new ApModesExtra
            {
                AltArmed = B(d.ApAltArm), GsArmed = B(d.ApGsArm), GsActive = B(d.ApGsHold), BackCourse = B(d.ApBc),
                WingLeveler = B(d.ApWingLeveler), PitchHold = B(d.ApPitchHold), AirspeedHold = B(d.ApSpdHold),
                MachHold = B(d.ApMachHold), Toga = B(d.ApToga), YawDamper = B(d.ApYawDamper), MachVar = R(d.ApMachVar, 3),
            },
            BrakeLeftPct = R(d.BrakeLeft, 0), BrakeRightPct = R(d.BrakeRight, 0),
        };
    }

    private static double Dots(double raw, double full) => R(Math.Clamp(raw / full * 2.5, -2.5, 2.5), 2);

    private static string ToFrom(double v) => (int)v switch { 1 => "to", 2 => "from", _ => "off" };

    private static AircraftInfo Aircraft(in MsfsData d, bool paused)
    {
        var engineType = AircraftClassifier.EngineTypeName((int)d.EngineType);
        var title = d.Title ?? "";
        return new AircraftInfo
        {
            Title = title, OnGround = B(d.OnGround), Paused = paused,
            EngineType = engineType,
            AtcModel = AircraftClassifier.CleanAtcString(d.AtcModel),
            AtcType = AircraftClassifier.CleanAtcString(d.AtcType),
            Category = AircraftClassifier.Classify(engineType, (int)d.EngineCount, d.MaxGrossWeightLbs,
                $"{title} {AircraftClassifier.CleanAtcString(d.AtcModel)} {AircraftClassifier.CleanAtcString(d.AtcType)}", d.SimCategory ?? ""),
        };
    }

    private static bool B(double v) => v != 0;
    private static double R(double v, int digits) => double.IsFinite(v) ? Math.Round(v, digits) : 0;

    public void Dispose()
    {
        _cts?.Cancel();
        _cts = null;
    }

    private static class Native
    {
        [DllImport("SimConnect.dll")]
        public static extern int SimConnect_MapClientEventToSimEvent(IntPtr hSimConnect, uint eventId,
            [MarshalAs(UnmanagedType.LPStr)] string eventName);

        [DllImport("SimConnect.dll")]
        public static extern int SimConnect_CameraSetRelative6DOF(IntPtr hSimConnect, float deltaX, float deltaY, float deltaZ,
            float pitchDeg, float bankDeg, float headingDeg);
    }
}
