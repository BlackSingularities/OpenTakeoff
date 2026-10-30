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
            return await LookAsync(command.Spec.Target == CommandTarget.CameraPitch, command.Value, ct);

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
        StopLook();
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
        StopLook();
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

    // Zdarzenie widoku — zapas przy zmianie kabina ↔ widok zewnętrzny (wewnętrzne, spoza listy telefonu)
    private static readonly string[] CameraEvents = { "VIEW_MODE" };
    // CAMERA STATE: 2 kabina, 3 zewnętrzna (w MSFS 2020 i 2024 te same numery)
    private const int CamCockpit = 2, CamExternal = 3;
    // Po tylu ms bez komend rozglądania (telefon śle je co 30 ms podczas przytrzymania) kamera wraca na wprost
    private const long LookReleaseDelayMs = 400;
    // Przycisk myszy puszczamy dopiero po tylu ms „na wprost” — przechodzenie przez środek nie klika w MSFS
    private const long LookCenteredHoldMs = 300;
    private readonly MouseLook _mouse = new();
    private long _lookYaw, _lookPitch, _lookLastCommand, _lookLastOffCenter;
    // Odmowa (MSFS nie na pierwszym planie) zgłaszana telefonowi raz na przytrzymanie, a nie 60 razy na sekundę
    private bool _lookRefused;
    private readonly SemaphoreSlim _cameraGate = new(1, 1);

    /// <summary>Czułość rozglądania: zliczenia myszy na stopień (ustawienie mostka, zależy od czułości myszy w MSFS).</summary>
    public double LookCountsPerDegree { get => _mouse.CountsPerDegree; set => _mouse.CountsPerDegree = Math.Clamp(value, 1, 60); }

    /// <summary>
    /// Rozglądanie się w bieżącym widoku, bez zmiany kamery. MSFS 2024 nie przyjmuje zapisu kąta kamery
    /// (CAMERA GAMEPLAY PITCH YAW jest tylko do odczytu, PAN_* ignoruje), a CameraSetRelative6DOF przełącza
    /// w osobną kamerę 6DOF, z której MSFS często nie wraca (dziennik: odrzucane powroty do kabiny). Dlatego obraca
    /// kamerę tak jak użytkownik myszą — patrz MouseLook. Kąty z telefonu są liczone od początku przytrzymania;
    /// po puszczeniu (0/0 albo cisza przez LookReleaseDelayMs) mysz wraca o tyle samo i puszcza przycisk.
    /// </summary>
    private async Task<CommandResult> LookAsync(bool pitchAxis, long valueDeg, CancellationToken ct)
    {
        await _cameraGate.WaitAsync(ct);
        try
        {
            var now = Environment.TickCount64;
            if (pitchAxis) _lookPitch = valueDeg; else _lookYaw = valueDeg;
            _lookLastCommand = now;
            var centered = _lookYaw == 0 && _lookPitch == 0;
            if (!centered) _lookLastOffCenter = now;

            if (!_mouse.Active)
            {
                if (centered) { _lookRefused = false; return CommandResult.Success; }
                if (_lookRefused) return CommandResult.Success;
                var refusal = _mouse.Begin();
                if (refusal != null)
                {
                    _lookRefused = true;
                    Log?.Invoke($"Kamera: rozglądanie niemożliwe — {refusal}");
                    return CommandResult.Fail(ErrorCodes.NotAllowed, refusal);
                }
                Log?.Invoke($"Kamera: rozglądanie w bieżącym widoku (MSFS zgłasza {RawCameraState()}, {CameraAngles()})");
                _ = LookPumpAsync();
            }
            return Step() ? CommandResult.Success
                : CommandResult.Fail(ErrorCodes.NotAllowed, "Okno MSFS przestało być aktywne — rozglądanie przerwane");
        }
        finally
        {
            _cameraGate.Release();
        }
    }

    // Co 15 ms dociąga mysz do ostatnich kątów (duże skoki w kilku krokach) i kończy rozglądanie po puszczeniu.
    private async Task LookPumpAsync()
    {
        while (true)
        {
            await Task.Delay(15);
            await _cameraGate.WaitAsync();
            try
            {
                if (!_mouse.Active) return;
                var now = Environment.TickCount64;
                // Telefon zamilkł (puszczony przycisk bez 0/0, zerwane połączenie) — powrót na wprost
                if (now - _lookLastCommand > LookReleaseDelayMs) { _lookYaw = 0; _lookPitch = 0; }
                if (!Step()) return;
                if (_lookYaw == 0 && _lookPitch == 0 && _mouse.Reached(0, 0) && now - _lookLastOffCenter > LookCenteredHoldMs)
                {
                    _mouse.Release();
                    _lookRefused = false;
                    Log?.Invoke($"Kamera: koniec rozglądania ({CameraAngles()})");
                    return;
                }
            }
            catch (Exception e)
            {
                _mouse.Release();
                Log?.Invoke($"Kamera: rozglądanie przerwane ({e.Message})");
                return;
            }
            finally
            {
                _cameraGate.Release();
            }
        }
    }

    // Wymaga _cameraGate. false = MSFS przestał być na pierwszym planie (przycisk myszy już puszczony).
    private bool Step()
    {
        if (!_mouse.Active) return false;
        if (_mouse.Step(_lookYaw, _lookPitch)) return true;
        Log?.Invoke("Kamera: okno MSFS przestało być aktywne — rozglądanie przerwane");
        _lookRefused = true;
        return false;
    }

    // Puszcza przycisk myszy (rozłączenie, zamknięcie mostka) — nigdy nie zostaje wciśnięty
    private void StopLook()
    {
        _cameraGate.Wait();
        try { _mouse.Release(); _lookYaw = 0; _lookPitch = 0; _lookRefused = false; }
        finally { _cameraGate.Release(); }
    }

    private string CameraAngles()
    {
        lock (_lock)
            return _last is { } d ? $"kamera {d.CamYawRad * 180 / Math.PI:+0;-0;0}° / {-d.CamPitchRad * 180 / Math.PI:+0;-0;0}°" : "brak danych kamery";
    }

    private int RawCameraState()
    {
        lock (_lock) return _last is { } d ? (int)d.CameraState : -1;
    }

    /// <summary>
    /// Zmiana kamery (kabina ↔ widok zewnętrzny). Dotknięcie widoku, który już jest, nic nie robi. Trwające
    /// rozglądanie najpierw się kończy (powrót myszy), żeby nowy widok nie zaczął się obrócony.
    /// </summary>
    private async Task<CommandResult> SetCameraAsync(SimConnectClient client, int target, CancellationToken ct)
    {
        if (_mouse.Active)
        {
            await _cameraGate.WaitAsync(ct);
            try { _lookYaw = 0; _lookPitch = 0; _lookLastOffCenter = 0; }
            finally { _cameraGate.Release(); }
            for (var i = 0; i < 100 && _mouse.Active; i++) await Task.Delay(20, ct);
        }
        await _cameraGate.WaitAsync(ct);
        try
        {
            var raw = RawCameraState();
            Log?.Invoke($"Kamera: zmiana na {target} (MSFS zgłasza {raw})");
            if (raw == target) return CommandResult.Success;
            if (await WriteCameraAsync(client, target, ct)) return CommandResult.Success;
            return CommandResult.Fail(ErrorCodes.SimError, target == CamCockpit
                ? "MSFS nie przełączył na widok z kabiny"
                : "MSFS nie przełączył na widok zewnętrzny");
        }
        finally
        {
            _cameraGate.Release();
        }
    }

    // Wymaga _cameraGate.
    private async Task<bool> WriteCameraAsync(SimConnectClient client, int target, CancellationToken ct)
    {
        if (await TryWriteCameraAsync(client, target, ct)) return true;
        // Zapas: część wersji MSFS ignoruje zapis CAMERA STATE bez błędu. Między kabiną a widokiem zewnętrznym
        // przełącza wtedy zdarzenie VIEW_MODE (jak klawisz zmiany widoku) — tylko gdy MSFS jest w jednym z nich,
        // bo z innych kamer VIEW_MODE wędruje po widokach nieprzewidywalnie.
        if (RawCameraState() is CamCockpit or CamExternal)
        {
            await TransmitAsync(client, "VIEW_MODE", 0, ct);
            if (await WaitForCameraAsync(target, ct)) return true;
        }
        // Z innej kamery (np. 6DOF po starszym mostku) do kabiny często wchodzi się tylko przez widok zewnętrzny
        else if (target == CamCockpit && await TryWriteCameraAsync(client, CamExternal, ct, attempts: 40)
            && await TryWriteCameraAsync(client, CamCockpit, ct, attempts: 40)) return true;
        Log?.Invoke($"Kamera: MSFS nie przełączył widoku na {target} (zgłasza {RawCameraState()})");
        return false;
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
        StopLook();
        _cts?.Cancel();
        _cts = null;
    }

    private static class Native
    {
        [DllImport("SimConnect.dll")]
        public static extern int SimConnect_MapClientEventToSimEvent(IntPtr hSimConnect, uint eventId,
            [MarshalAs(UnmanagedType.LPStr)] string eventName);
    }
}
