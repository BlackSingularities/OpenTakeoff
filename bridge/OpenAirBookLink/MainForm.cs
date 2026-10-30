using System.Drawing.Drawing2D;
using OpenAirBookLink.Core.Protocol;

namespace OpenAirBookLink;

// Okno programu: stan MSFS, kod parowania, adresy IP, telefony, dziennik. Zamknięcie chowa do zasobnika.
public sealed class MainForm : Form
{
    private static readonly Color Ok = Color.FromArgb(0x1E, 0x8E, 0x3E);
    private static readonly Color Waiting = Color.FromArgb(0xC2, 0x7C, 0x0E);
    private static readonly Color Accent = Color.FromArgb(0x1A, 0x5F, 0xB4);

    private readonly LinkHost _host;
    private readonly bool _startMinimized;
    private readonly NotifyIcon _tray;
    private readonly ToolStripMenuItem _traySim;
    private readonly Label _simDot = new() { AutoSize = true, Font = new Font("Segoe UI", 14f), Text = "●" };
    private readonly Label _simStatus = new() { AutoSize = true, Font = new Font("Segoe UI", 10.5f, FontStyle.Bold) };
    private readonly CheckBox _simMode = new() { AutoSize = true, Text = "Tryb symulacji — lot testowy EPWA → EPGD bez MSFS" };
    private readonly Label _code = new() { AutoSize = true, Font = new Font("Consolas", 30f, FontStyle.Bold) };
    private readonly Label _ips = new() { AutoSize = true, MaximumSize = new Size(560, 0) };
    private readonly ListView _clients = new() { View = View.Details, FullRowSelect = true, Dock = DockStyle.Fill, HeaderStyle = ColumnHeaderStyle.Nonclickable };
    private readonly ListBox _log = new() { Dock = DockStyle.Fill, IntegralHeight = false, HorizontalScrollbar = true, Font = new Font("Consolas", 9f) };
    private readonly System.Windows.Forms.Timer _refresh = new() { Interval = 1000 };
    private readonly System.Collections.Concurrent.ConcurrentQueue<string> _pendingLog = new();
    private bool _exiting;
    private bool _trayHintShown;

    // Samoaktualizacja: pasek nad oknem widoczny tylko, gdy jest nowa wersja
    private readonly string[] _args;
    private readonly FlowLayoutPanel _updateBar = new()
    {
        Dock = DockStyle.Top, AutoSize = true, Visible = false, WrapContents = false,
        BackColor = Color.FromArgb(0xE3, 0xEE, 0xFB), Padding = new Padding(8, 6, 8, 6), Margin = new Padding(0, 0, 0, 10),
    };
    private readonly Label _updateText = new() { AutoSize = true, MaximumSize = new Size(440, 0), Margin = new Padding(0, 7, 12, 0) };
    private readonly Button _updateButton = new() { AutoSize = true };
    private readonly System.Windows.Forms.Timer _updateTimer = new() { Interval = 10_000 };
    private LinkUpdate? _update;
    private string? _downloaded;
    private bool _updateBusy;

    public MainForm(LinkHost host, bool startMinimized, string[] args)
    {
        _host = host;
        _startMinimized = startMinimized;
        _args = args;

        Text = $"OpenTakeoff Link {host.Version}";
        Icon = CreateIcon();
        Font = new Font("Segoe UI", 9.5f);
        MinimumSize = new Size(620, 640);
        Size = new Size(680, 760);
        StartPosition = FormStartPosition.CenterScreen;

        var root = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 1, RowCount = 5, Padding = new Padding(12) };
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.Percent, 40));
        root.RowStyles.Add(new RowStyle(SizeType.Percent, 60));
        Controls.Add(root);

        // Aktualizacja
        _updateText.ForeColor = Accent;
        _updateButton.Click += (_, _) => OnUpdateButton();
        _updateBar.Controls.Add(_updateText);
        _updateBar.Controls.Add(_updateButton);
        root.Controls.Add(_updateBar);

        // Symulator
        var simBox = Group("Symulator");
        var simFlow = Flow();
        var simLine = Flow(FlowDirection.LeftToRight);
        simLine.Controls.Add(_simDot);
        _simStatus.Margin = new Padding(0, 6, 0, 0);
        simLine.Controls.Add(_simStatus);
        simFlow.Controls.Add(simLine);
        _simMode.Checked = host.SimulationMode;
        _simMode.CheckedChanged += (_, _) => host.SetSimulationMode(_simMode.Checked);
        simFlow.Controls.Add(_simMode);
        // Rozglądanie z telefonu porusza kamerą jak mysz z prawym przyciskiem — tempo zależy od czułości myszy w MSFS
        var lookLine = Flow(FlowDirection.LeftToRight);
        lookLine.Controls.Add(new Label { AutoSize = true, Margin = new Padding(0, 6, 6, 0), Text = "Czułość rozglądania z telefonu (%):" });
        var look = new NumericUpDown
        {
            Minimum = LinkHost.LookSensitivityMin, Maximum = LinkHost.LookSensitivityMax, Increment = 10,
            Value = Math.Clamp(host.LookSensitivity, LinkHost.LookSensitivityMin, LinkHost.LookSensitivityMax), Width = 70,
        };
        look.ValueChanged += (_, _) => host.SetLookSensitivity((int)look.Value);
        lookLine.Controls.Add(look);
        lookLine.Controls.Add(new Label { AutoSize = true, Margin = new Padding(8, 6, 0, 0), ForeColor = SystemColors.GrayText, Text = "okno MSFS musi być aktywne" });
        simFlow.Controls.Add(lookLine);
        simBox.Controls.Add(simFlow);
        root.Controls.Add(simBox);

        // Parowanie
        var pairBox = Group("Parowanie telefonu");
        var pairFlow = Flow();
        pairFlow.Controls.Add(new Label { AutoSize = true, Text = "Wpisz ten kod w aplikacji OpenTakeoff na telefonie:" });
        _code.ForeColor = Accent;
        pairFlow.Controls.Add(_code);
        var buttons = Flow(FlowDirection.LeftToRight);
        var newCode = new Button { AutoSize = true, Text = "Nowy kod" };
        newCode.Click += (_, _) => { host.Pairing.RegenerateCode(); UpdateCode(); };
        var revoke = new Button { AutoSize = true, Text = "Unieważnij sparowania" };
        revoke.Click += (_, _) =>
        {
            if (MessageBox.Show(this, "Wszystkie telefony będą musiały sparować się ponownie kodem. Kontynuować?",
                    "OpenTakeoff Link", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
            host.Pairing.RevokeAll();
            host.Pairing.RegenerateCode();
            UpdateCode();
            AddLog("Unieważniono wszystkie sparowania");
        };
        buttons.Controls.Add(newCode);
        buttons.Controls.Add(revoke);
        pairFlow.Controls.Add(buttons);
        pairFlow.Controls.Add(_ips);
        pairBox.Controls.Add(pairFlow);
        root.Controls.Add(pairBox);

        // Telefony
        var clientsBox = Group("Połączone telefony");
        clientsBox.Dock = DockStyle.Fill;
        _clients.Columns.Add("Telefon", 170);
        _clients.Columns.Add("Adres IP", 120);
        _clients.Columns.Add("Stan", 110);
        _clients.Columns.Add("Od", 80);
        _clients.Columns.Add("Hz", 50);
        clientsBox.Controls.Add(_clients);
        root.Controls.Add(clientsBox);

        // Dziennik
        var logBox = Group("Dziennik zdarzeń");
        logBox.Dock = DockStyle.Fill;
        logBox.Controls.Add(_log);
        root.Controls.Add(logBox);

        // Zasobnik
        _traySim = new ToolStripMenuItem("Tryb symulacji") { CheckOnClick = true, Checked = host.SimulationMode };
        _traySim.CheckedChanged += (_, _) => _simMode.Checked = _traySim.Checked;
        var menu = new ContextMenuStrip();
        menu.Items.Add("Pokaż okno", null, (_, _) => ShowFromTray());
        menu.Items.Add(_traySim);
        if (Updater.ExePath is not null)
            menu.Items.Add("Sprawdź aktualizacje", null, (_, _) => _ = CheckForUpdateAsync(manual: true));
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Zakończ", null, (_, _) => ExitApp());
        _tray = new NotifyIcon { Icon = Icon, Text = "OpenTakeoff Link", Visible = true, ContextMenuStrip = menu };
        _tray.DoubleClick += (_, _) => ShowFromTray();

        host.Log += m =>
        {
            var line = $"{DateTime.Now:HH:mm:ss}  {m}";
            AppendLogFile(line);
            if (IsHandleCreated) BeginInvokeSafe(() => AddLine(line));
            else _pendingLog.Enqueue(line); // zdarzenia z host.Start() przed pokazaniem okna
        };
        host.SourceStatusChanged += () => BeginInvokeSafe(UpdateStatus);
        host.Server.ClientsChanged += () => BeginInvokeSafe(UpdateClients);
        host.Pairing.CodeChanged += () => BeginInvokeSafe(UpdateCode);
        _simMode.CheckedChanged += (_, _) => { _traySim.Checked = _simMode.Checked; UpdateStatus(); };

        _refresh.Tick += (_, _) => { UpdateStatus(); UpdateClients(); ApplyUpdateIfIdle(); };
        _refresh.Start();

        // Pierwsze sprawdzenie 10 s po starcie, potem co 6 godzin. Buildy spoza CI (dotnet run) pomijamy.
        if (Updater.ExePath is not null)
        {
            _updateTimer.Tick += (_, _) => { _updateTimer.Interval = 6 * 3600 * 1000; _ = CheckForUpdateAsync(manual: false); };
            _updateTimer.Start();
        }

        UpdateCode();
        UpdateStatus();
        UpdateClients();
        _ips.Text = IpText();
        AddLog($"OpenTakeoff Link {host.Version} uruchomiony" + (args.Contains(Updater.UpdatedArg) ? " (po aktualizacji)" : ""));
    }

    private async Task CheckForUpdateAsync(bool manual)
    {
        if (_updateBusy || _downloaded is not null) return;
        _updateBusy = true;
        try
        {
            var update = await Updater.CheckAsync();
            if (update is null)
            {
                if (manual) _tray.ShowBalloonTip(3000, "OpenTakeoff Link", $"Masz najnowszą wersję ({_host.Version}).", ToolTipIcon.Info);
                return;
            }
            _update = update;
            AddLog($"Dostępna nowa wersja {update.Version}");

            // Exe w folderze bez prawa zapisu (np. Program Files) — tylko odsyłamy na stronę wydania.
            if (!Updater.CanWriteNextToExe())
            {
                ShowUpdateBar($"Dostępna wersja {update.Version}. Brak prawa zapisu w folderze programu — pobierz ją ręcznie.", "Otwórz stronę wydania");
                return;
            }

            ShowUpdateBar($"Pobieranie wersji {update.Version}…", null);
            var progress = new Progress<double>(p => _updateText.Text = $"Pobieranie wersji {update.Version}… {p:P0}");
            _downloaded = await Updater.DownloadAsync(update, progress);
            AddLog($"Pobrano wersję {update.Version}");
            if (!ApplyUpdateIfIdle())
            {
                ShowUpdateBar($"Wersja {update.Version} gotowa — zainstaluje się sama po rozłączeniu telefonów.", "Zainstaluj teraz");
                _tray.ShowBalloonTip(4000, "OpenTakeoff Link — aktualizacja",
                    $"Wersja {update.Version} zainstaluje się po rozłączeniu telefonów.", ToolTipIcon.Info);
            }
        }
        catch (Exception e) // wynik zadania nikt nie odbiera — każdy błąd musi trafić do dziennika
        {
            AddLog($"Aktualizacja nieudana: {e.Message}");
            if (manual) _tray.ShowBalloonTip(3000, "OpenTakeoff Link", $"Nie udało się sprawdzić aktualizacji: {e.Message}", ToolTipIcon.Warning);
            if (_update is not null && _downloaded is null) ShowUpdateBar($"Nie udało się pobrać wersji {_update.Version}.", "Spróbuj ponownie");
        }
        finally
        {
            _updateBusy = false;
        }
    }

    private void OnUpdateButton()
    {
        if (_update is null) return;
        if (_downloaded is not null) ApplyUpdate();
        else if (!Updater.CanWriteNextToExe())
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(_update.PageUrl) { UseShellExecute = true });
        else _ = CheckForUpdateAsync(manual: true);
    }

    // Nie przerywamy lotu: nowa wersja wchodzi sama dopiero, gdy żaden telefon nie jest połączony.
    private bool ApplyUpdateIfIdle()
    {
        if (_downloaded is null || _exiting || _host.Server.Clients.Count > 0) return false;
        ApplyUpdate();
        return true;
    }

    private void ApplyUpdate()
    {
        if (_downloaded is null || _update is null) return;
        var downloaded = _downloaded;
        _downloaded = null;
        // Nowa instancja startuje w tym samym stanie okna (widoczne albo w zasobniku).
        var args = _args.Where(a => a != "--minimized").ToList();
        if (!Visible) args.Add("--minimized");
        try
        {
            AddLog($"Instalowanie wersji {_update.Version} i ponowne uruchomienie");
            Updater.ApplyAndRestart(downloaded, args);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception)
        {
            AddLog($"Nie udało się podmienić pliku programu: {e.Message}");
            ShowUpdateBar($"Nie udało się zainstalować wersji {_update.Version}: {e.Message}", "Spróbuj ponownie");
            return;
        }
        ExitApp();
    }

    private void ShowUpdateBar(string text, string? button)
    {
        _updateText.Text = text;
        _updateButton.Text = button ?? "";
        _updateButton.Visible = button is not null;
        _updateBar.Visible = true;
    }

    protected override void SetVisibleCore(bool value)
    {
        // Start z --minimized: od razu w zasobniku.
        if (_startMinimized && !IsHandleCreated)
        {
            CreateHandle();
            value = false;
        }
        base.SetVisibleCore(value);
    }

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        if (!_exiting && e.CloseReason == CloseReason.UserClosing)
        {
            e.Cancel = true;
            Hide();
            if (!_trayHintShown)
            {
                _trayHintShown = true;
                _tray.ShowBalloonTip(3000, "OpenTakeoff Link działa dalej",
                    "Program jest w zasobniku obok zegara. Zakończysz go z menu ikony.", ToolTipIcon.Info);
            }
            return;
        }
        _refresh.Stop();
        _tray.Visible = false;
        base.OnFormClosing(e);
    }

    private void ShowFromTray()
    {
        Show();
        WindowState = FormWindowState.Normal;
        Activate();
    }

    private void ExitApp()
    {
        _exiting = true;
        Close();
    }

    private void UpdateCode()
    {
        var c = _host.Pairing.Code;
        _code.Text = $"{c[..3]} {c[3..]}";
    }

    private void UpdateStatus()
    {
        var source = _host.Source;
        var connected = source.IsConnected;
        _simDot.ForeColor = connected ? Ok : Waiting;
        _simStatus.Text = source.StatusText + (_host.Axes.IsActive ? "  ·  sterowanie osiami z telefonu" : "");
        _tray.Text = Truncate($"OpenTakeoff Link — {source.StatusText}", 63);
    }

    private void UpdateClients()
    {
        var list = _host.Server.Clients;
        _clients.BeginUpdate();
        _clients.Items.Clear();
        foreach (var c in list)
        {
            var item = new ListViewItem(c.Name);
            item.SubItems.Add(c.RemoteIp);
            item.SubItems.Add(c.Paired ? (_host.Axes.Owner == c.Id ? "steruje osiami" : "sparowany") : "czeka na kod");
            item.SubItems.Add(c.ConnectedAt.ToLocalTime().ToString("HH:mm:ss"));
            item.SubItems.Add(c.Paired ? c.RateHz.ToString() : "–");
            _clients.Items.Add(item);
        }
        if (list.Count == 0) _clients.Items.Add(new ListViewItem("Brak połączeń") { ForeColor = SystemColors.GrayText });
        _clients.EndUpdate();
        _ips.Text = IpText();
    }

    private static string IpText()
    {
        var ips = LinkHost.LocalAddresses();
        var list = ips.Count == 0 ? "brak sieci" : string.Join(", ", ips);
        return $"Adresy tego komputera: {list}   ·   port {LinkProtocol.WebSocketPort} (WebSocket), {LinkProtocol.DiscoveryPort} (UDP)";
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        BeginInvoke(() => { while (_pendingLog.TryDequeue(out var line)) AddLine(line); });
    }

    private void AddLog(string message)
    {
        var line = $"{DateTime.Now:HH:mm:ss}  {message}";
        AppendLogFile(line);
        AddLine(line);
    }

    // Dziennik także w pliku (%APPDATA%\OpenAirBook Link\link.log) — do diagnozy bez zrzutów ekranu; ponad 1 MB zaczyna od nowa
    private static readonly string LogFile = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "OpenAirBook Link", "link.log");
    private static readonly object LogFileLock = new();

    private static void AppendLogFile(string line)
    {
        try
        {
            lock (LogFileLock)
            {
                Directory.CreateDirectory(Path.GetDirectoryName(LogFile)!);
                if (File.Exists(LogFile) && new FileInfo(LogFile).Length > 1_000_000) File.Delete(LogFile);
                File.AppendAllText(LogFile, $"{DateTime.Now:yyyy-MM-dd} {line}{Environment.NewLine}");
            }
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private void AddLine(string line)
    {
        _log.Items.Add(line);
        while (_log.Items.Count > 500) _log.Items.RemoveAt(0);
        _log.TopIndex = Math.Max(0, _log.Items.Count - 1);
    }

    private void BeginInvokeSafe(Action action)
    {
        if (IsDisposed || !IsHandleCreated) return;
        try { BeginInvoke(action); }
        catch (InvalidOperationException) { }
    }

    private static string Truncate(string s, int max) => s.Length <= max ? s : s[..(max - 1)] + "…";

    private static GroupBox Group(string title) => new()
    {
        Text = title,
        Dock = DockStyle.Top,
        AutoSize = true,
        Padding = new Padding(10, 6, 10, 8),
        Margin = new Padding(0, 0, 0, 10),
    };

    private static FlowLayoutPanel Flow(FlowDirection direction = FlowDirection.TopDown) => new()
    {
        FlowDirection = direction,
        AutoSize = true,
        Dock = direction == FlowDirection.TopDown ? DockStyle.Fill : DockStyle.None,
        WrapContents = false,
        Margin = new Padding(0, 2, 0, 2),
    };

    // Ikona rysowana w kodzie: niebieskie koło z białym samolotem (strzałką).
    private static Icon CreateIcon()
    {
        using var bmp = new Bitmap(32, 32);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            using var bg = new SolidBrush(Accent);
            g.FillEllipse(bg, 1, 1, 30, 30);
            using var fg = new SolidBrush(Color.White);
            g.FillPolygon(fg, new PointF[]
            {
                new(16, 4), new(18.5f, 13), new(28, 18), new(28, 20.5f), new(18.5f, 18), new(18, 24),
                new(21, 27), new(21, 28.5f), new(16, 27), new(11, 28.5f), new(11, 27), new(14, 24),
                new(13.5f, 18), new(4, 20.5f), new(4, 18), new(13.5f, 13),
            });
        }
        return Icon.FromHandle(bmp.GetHicon());
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            _tray.Dispose();
            _refresh.Dispose();
            _updateTimer.Dispose();
        }
        base.Dispose(disposing);
    }
}
