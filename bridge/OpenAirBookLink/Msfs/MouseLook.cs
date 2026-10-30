using System.Diagnostics;
using System.Runtime.InteropServices;

namespace OpenAirBookLink.Msfs;

/// <summary>
/// Rozglądanie się tym samym sposobem, co myszą w MSFS: przytrzymany prawy przycisk i ruch myszy. Działa w bieżącej
/// kamerze (kabina, widok zewnętrzny, dron) i nie zmienia CAMERA STATE — MSFS 2024 nie pozwala zapisać kąta kamery
/// (CAMERA GAMEPLAY PITCH YAW jest tylko do odczytu), a CameraSetRelative6DOF przełącza w osobną kamerę 6DOF,
/// z której symulator często nie chce wyjść.
///
/// Ruch idzie przez SendInput do okna na pierwszym planie, więc mostek działa tylko wtedy, gdy na pierwszym planie
/// jest MSFS. Przed wciśnięciem przycisku kursor stoi u góry okna, nad szybą, a nie nad przełącznikami w kabinie.
/// Po puszczeniu mysz wraca dokładnie o tyle samo, ile się przesunęła — kamera wraca do widoku sprzed rozglądania.
/// Nie jest bezpieczny wątkowo: wywołania serializuje bramka kamery w MsfsSource.
/// </summary>
internal sealed class MouseLook
{
    private static readonly string[] SimProcesses = { "FlightSimulator2024", "FlightSimulator" };
    // Największy ruch w jednym kroku — większe skoki rozkładają się na kolejne kroki (płynny obrót, bez szarpnięć)
    private const int MaxStep = 160;
    // Kursor przed wciśnięciem przycisku: środek okna w poziomie, tyle wysokości od góry (szyba / niebo, nie panel)
    private const double ParkFromTop = 0.14;

    private bool _held;
    private IntPtr _window;
    private int _sentX, _sentY;
    private POINT _cursor;

    public bool Active => _held;

    /// <summary>Ile zliczeń myszy na stopień obrotu (zależy od czułości myszy w MSFS).</summary>
    public double CountsPerDegree { get; set; } = 8;

    /// <summary>Wciska prawy przycisk nad oknem MSFS. Zwraca null albo powód, dla którego nie można zacząć.</summary>
    public string? Begin()
    {
        if (_held) return null;
        var window = SimWindow(out var reason);
        if (window == IntPtr.Zero) return reason;
        if (!GetClientRect(window, out var rc) || rc.Right - rc.Left < 50 || rc.Bottom - rc.Top < 50)
            return "Nie można odczytać okna MSFS";
        var park = new POINT { X = (rc.Right - rc.Left) / 2, Y = (int)((rc.Bottom - rc.Top) * ParkFromTop) };
        if (!ClientToScreen(window, ref park)) return "Nie można odczytać okna MSFS";

        GetCursorPos(out _cursor);
        SetCursorPos(park.X, park.Y);
        if (Send(Mouse(0, 0, MOUSEEVENTF_RIGHTDOWN)) == 0)
        {
            SetCursorPos(_cursor.X, _cursor.Y);
            return "Windows zablokował sterowanie myszą (SendInput)";
        }
        _window = window;
        _held = true;
        _sentX = 0; _sentY = 0;
        return null;
    }

    /// <summary>
    /// Krok w stronę kątów [yawDeg] (+ w prawo) i [pitchDeg] (+ w górę) od początku rozglądania.
    /// Zwraca false, gdy MSFS przestał być na pierwszym planie — wtedy przycisk jest już puszczony.
    /// </summary>
    public bool Step(double yawDeg, double pitchDeg)
    {
        if (!_held) return false;
        if (GetForegroundWindow() != _window) { Release(); return false; }
        var (tx, ty) = Counts(yawDeg, pitchDeg, CountsPerDegree);
        var dx = Math.Clamp(tx - _sentX, -MaxStep, MaxStep);
        var dy = Math.Clamp(ty - _sentY, -MaxStep, MaxStep);
        if (dx == 0 && dy == 0) return true;
        if (Send(Mouse(dx, dy, MOUSEEVENTF_MOVE)) == 0) return true;
        _sentX += dx; _sentY += dy;
        return true;
    }

    /// <summary>Czy kamera dotarła już do kątów z ostatniego Step (cały ruch wysłany).</summary>
    public bool Reached(double yawDeg, double pitchDeg) => Counts(yawDeg, pitchDeg, CountsPerDegree) == (_sentX, _sentY);

    /// <summary>
    /// Puszczenie przycisku i kursor na dawne miejsce. Kamera wraca na wprost tylko wtedy, gdy wcześniej Step(0, 0)
    /// wysłał cały ruch powrotny (Reached) — przy utracie okna MSFS i rozłączeniu zostaje, gdzie była.
    /// </summary>
    public void Release()
    {
        if (!_held) return;
        _held = false;
        Send(Mouse(0, 0, MOUSEEVENTF_RIGHTUP));
        SetCursorPos(_cursor.X, _cursor.Y);
    }

    /// <summary>Zliczenia myszy dla kątów: w prawo = +x, w górę = −y (jak przy ruchu myszą).</summary>
    internal static (int x, int y) Counts(double yawDeg, double pitchDeg, double countsPerDegree) =>
        ((int)Math.Round(yawDeg * countsPerDegree), (int)Math.Round(-pitchDeg * countsPerDegree));

    /// <summary>Okno MSFS na pierwszym planie albo IntPtr.Zero i powód.</summary>
    private static IntPtr SimWindow(out string? reason)
    {
        reason = null;
        var window = GetForegroundWindow();
        GetWindowThreadProcessId(window, out var pid);
        try
        {
            using var p = Process.GetProcessById((int)pid);
            if (!SimProcesses.Contains(p.ProcessName, StringComparer.OrdinalIgnoreCase))
            {
                reason = "Rozglądanie działa, gdy okno MSFS jest aktywne na komputerze — kliknij w nie raz";
                return IntPtr.Zero;
            }
            if (IsElevated(pid) && !IsElevated((uint)Environment.ProcessId))
            {
                reason = "MSFS działa jako administrator — uruchom OpenTakeoff Link też jako administrator";
                return IntPtr.Zero;
            }
            return window;
        }
        catch (Exception e) when (e is ArgumentException or InvalidOperationException)
        {
            reason = "Rozglądanie działa, gdy okno MSFS jest aktywne na komputerze — kliknij w nie raz";
            return IntPtr.Zero;
        }
    }

    // Okno procesu z uprawnieniami administratora nie przyjmuje SendInput od zwykłego procesu (UIPI) — bez błędu
    private static bool IsElevated(uint pid)
    {
        var process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if (process == IntPtr.Zero) return false;
        try
        {
            if (!OpenProcessToken(process, TOKEN_QUERY, out var token)) return false;
            try
            {
                return GetTokenInformation(token, TokenElevation, out var elevated, sizeof(int), out _) && elevated != 0;
            }
            finally { CloseHandle(token); }
        }
        finally { CloseHandle(process); }
    }

    private static INPUT Mouse(int dx, int dy, uint flags) => new()
    {
        type = INPUT_MOUSE,
        mi = new MOUSEINPUT { dx = dx, dy = dy, dwFlags = flags },
    };

    private static uint Send(INPUT input) => SendInput(1, new[] { input }, Marshal.SizeOf<INPUT>());

    private const uint INPUT_MOUSE = 0;
    private const uint MOUSEEVENTF_MOVE = 0x0001, MOUSEEVENTF_RIGHTDOWN = 0x0008, MOUSEEVENTF_RIGHTUP = 0x0010;
    private const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000, TOKEN_QUERY = 0x0008;
    private const int TokenElevation = 20;

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MOUSEINPUT
    {
        public int dx, dy;
        public uint mouseData, dwFlags, time;
        public IntPtr dwExtraInfo;
    }

    // INPUT dla x64 (mostek budowany tylko na win-x64): typ, wyrównanie do 8 bajtów, MOUSEINPUT — razem 40 bajtów
    [StructLayout(LayoutKind.Explicit)]
    private struct INPUT
    {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public MOUSEINPUT mi;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll")] private static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] private static extern bool ClientToScreen(IntPtr hWnd, ref POINT point);
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")] private static extern bool SetCursorPos(int x, int y);
    [DllImport("kernel32.dll")] private static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll")] private static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll")]
    private static extern bool GetTokenInformation(IntPtr token, int infoClass, out int info, int length, out int returned);
}
