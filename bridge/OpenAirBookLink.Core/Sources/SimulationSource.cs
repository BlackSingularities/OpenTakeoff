using OpenAirBookLink.Core.Localization;
using System.Diagnostics;
using OpenAirBookLink.Core.Control;
using OpenAirBookLink.Core.NavData;
using OpenAirBookLink.Core.Protocol;

namespace OpenAirBookLink.Core.Sources;

// Tryb symulacji: lot testowy bez MSFS, krok co 50 ms.
public sealed class SimulationSource : IFlightSource
{
    private readonly object _lock = new();
    private readonly FlightSimulation _sim = new();
    private readonly double _timeScale;
    private readonly Stopwatch _clock = Stopwatch.StartNew();
    private Timer? _timer;
    private long _lastTickMs;

    // startOnFinalNm: zacznij od podejścia ILS 29 do EPGD z tej odległości (szybki test przyziemienia).
    public SimulationSource(AxisController axes, double timeScale = 1.0, double? startOnFinalNm = null)
    {
        Axes = axes;
        _timeScale = timeScale;
        if (startOnFinalNm is { } nm) _sim.PositionOnFinal(nm);
    }

    public string Kind => "simulation";
    public bool IsConnected => _timer != null;
    public string StatusText => L.T("sim.status", PhaseName(_sim.Phase), Math.Round(_sim.DistanceToArrivalNm));
    public AxisController Axes { get; }
    public INavDataProvider NavData { get; } = new SimulationNavData();
    INavDataProvider? IFlightSource.NavData => NavData;

    public event Action? StatusChanged;

    public void Start()
    {
        if (_timer != null) return;
        _lastTickMs = _clock.ElapsedMilliseconds;
        _timer = new Timer(_ => Tick(), null, 0, 50);
        StatusChanged?.Invoke();
    }

    public FlightState? GetSnapshot()
    {
        lock (_lock) return _timer == null ? null : _sim.State.Clone();
    }

    public Task<CommandResult> ExecuteAsync(ValidatedCommand command, CancellationToken ct)
    {
        lock (_lock) return Task.FromResult(_sim.Execute(command));
    }

    private void Tick()
    {
        var now = _clock.ElapsedMilliseconds;
        var axes = Axes.Peek(Environment.TickCount64);
        Axes.Tick(Environment.TickCount64); // zdejmij flagę jednorazowego neutralu
        var aux = Axes.TickAux(Environment.TickCount64);
        var before = _sim.Phase;
        lock (_lock)
        {
            if (aux?.Speedbrake is { } sb) _sim.State.Controls.SpoilersPct = Math.Round(sb * 100);
            var dt = Math.Min(0.5, (now - _lastTickMs) / 1000.0) * _timeScale;
            _lastTickMs = now;
            _sim.Step(dt, axes, DateTime.UtcNow);
        }
        if (_sim.Phase != before) StatusChanged?.Invoke();
    }

    private static string PhaseName(SimPhase p) => p switch
    {
        SimPhase.Parked => L.T("sim.parked"),
        SimPhase.TakeoffRoll => L.T("sim.takeoff"),
        SimPhase.Climb => L.T("sim.climb"),
        SimPhase.Cruise => L.T("sim.cruise"),
        SimPhase.Descent => L.T("sim.descent"),
        SimPhase.Approach => L.T("sim.approach"),
        SimPhase.Rollout => L.T("sim.rollout"),
        _ => L.T("sim.arrived"),
    };

    public void Dispose()
    {
        _timer?.Dispose();
        _timer = null;
    }
}
