using System.Net.Http;
using System.Text;
using System.Text.Json;

namespace OpenAirBookLink;

/// <summary>
/// Zgłoszenia do redzikowski.com:
///  - raport o błędzie – awaria zapisana na dysku i wysłana przy następnym starcie tylko za zgodą
///    użytkownika (trafia do „Raportów błędów” w panelu),
///  - zgłoszenie błędu albo sugestia funkcjonalności pisane ręcznie („Sugestie i zgłoszenia”).
/// Adres e-mail jest zawsze opcjonalny.
/// </summary>
internal static class Feedback
{
    private const string Endpoint = "https://redzikowski.com/api/feedback";
    private static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(20) };

    private static string CrashFile => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "OpenTakeoff-Link", "feedback-pending-crash.txt");

    /// <summary>Wywołaj na początku Main – przechwytuje awarie, których program nie obsłużył.</summary>
    public static void Install()
    {
        Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);
        Application.ThreadException += (_, e) =>
        {
            SaveCrash(e.Exception);
            OfferPendingCrash(null);
        };
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
        {
            if (e.ExceptionObject is Exception ex) SaveCrash(ex);
        };
    }

    private static void SaveCrash(Exception error)
    {
        try
        {
            if (File.Exists(CrashFile)) return;
            Directory.CreateDirectory(Path.GetDirectoryName(CrashFile)!);
            File.WriteAllText(CrashFile, $"OpenTakeoff Link {Updater.Current}\n\n{error}");
        }
        catch
        {
            // Nie da się zapisać – trudno.
        }
    }

    /// <summary>Awaria z poprzedniego uruchomienia (albo przed chwilą): pytanie o wysłanie raportu.</summary>
    public static void OfferPendingCrash(IWin32Window? owner)
    {
        string text;
        try
        {
            if (!File.Exists(CrashFile)) return;
            text = File.ReadAllText(CrashFile);
            File.Delete(CrashFile);
        }
        catch
        {
            return;
        }
        if (string.IsNullOrWhiteSpace(text)) return;
        using var form = new FeedbackForm(text);
        form.ShowDialog(owner);
    }

    public static void ShowForm(IWin32Window? owner)
    {
        using var form = new FeedbackForm(null);
        form.ShowDialog(owner);
    }

    public static bool EmailOk(string email)
    {
        var e = email.Trim();
        if (e.Length == 0) return true;
        var at = e.IndexOf('@');
        return at > 0 && e.IndexOf('.', at) > at + 1 && !e.Contains(' ') && !e.EndsWith('.');
    }

    /// <summary>Zwraca null, gdy się udało, inaczej opis błędu.</summary>
    public static async Task<string?> SendAsync(string kind, string message, string email, bool includeDevice, string? stack)
    {
        var body = new Dictionary<string, object?>
        {
            ["kind"] = kind,
            ["app"] = "opentakeoff",
            ["appName"] = "OpenTakeoff Link",
            ["appVersion"] = Updater.Current.ToString(),
            ["platform"] = "windows",
            ["locale"] = System.Globalization.CultureInfo.CurrentUICulture.Name,
            ["message"] = message.Trim(),
            ["email"] = email.Trim(),
        };
        if (includeDevice)
        {
            body["os"] = $"{System.Runtime.InteropServices.RuntimeInformation.OSDescription} ({System.Runtime.InteropServices.RuntimeInformation.OSArchitecture})";
            body["extra"] = new Dictionary<string, object> { ["dotnet"] = Environment.Version.ToString(), ["cpus"] = Environment.ProcessorCount };
        }
        if (stack is not null) body["stack"] = stack.Length > 60_000 ? stack[..60_000] : stack;
        try
        {
            using var content = new StringContent(JsonSerializer.Serialize(body), Encoding.UTF8, "application/json");
            using var response = await Http.PostAsync(Endpoint, content);
            if (response.IsSuccessStatusCode) return null;
            if ((int)response.StatusCode == 429) return "Za dużo zgłoszeń – spróbuj później.";
            return $"Serwer odrzucił zgłoszenie (HTTP {(int)response.StatusCode}).";
        }
        catch
        {
            return "Brak połączenia z redzikowski.com.";
        }
    }
}

/// <summary>Okno: formularz „Zgłoś błąd lub sugestię” albo raport o awarii.</summary>
internal sealed class FeedbackForm : Form
{
    private readonly string? _crash;
    private readonly RadioButton _bug = new() { Text = "Błąd", Checked = true, AutoSize = true };
    private readonly RadioButton _feature = new() { Text = "Sugestia funkcji", AutoSize = true };
    private readonly Label _prompt = new() { AutoSize = true, Margin = new Padding(0, 10, 0, 4) };
    private readonly TextBox _message = new() { Multiline = true, ScrollBars = ScrollBars.Vertical, Height = 130, Dock = DockStyle.Fill, MaxLength = 8000 };
    private readonly TextBox _email = new() { Dock = DockStyle.Fill, MaxLength = 200, PlaceholderText = "np. jan@example.com" };
    private readonly CheckBox _device = new() { Text = "Dołącz wersję programu i systemu", Checked = true, AutoSize = true, Margin = new Padding(0, 8, 0, 0) };
    private readonly Label _error = new() { AutoSize = true, ForeColor = Color.Firebrick, Visible = false, MaximumSize = new Size(460, 0) };
    private readonly Button _send = new() { AutoSize = true };
    private readonly Button _cancel = new() { AutoSize = true };
    private bool _busy;

    public FeedbackForm(string? crash)
    {
        _crash = crash;
        Text = crash is null ? "Zgłoś błąd lub sugestię – OpenTakeoff Link" : "Raport o błędzie – OpenTakeoff Link";
        Font = new Font("Segoe UI", 9.5f);
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = MinimizeBox = false;
        ShowInTaskbar = crash is not null;
        StartPosition = FormStartPosition.CenterScreen;
        AutoSize = true;
        AutoSizeMode = AutoSizeMode.GrowAndShrink;

        var root = new TableLayoutPanel { ColumnCount = 1, AutoSize = true, Padding = new Padding(16), Width = 500 };
        root.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 470));
        Controls.Add(root);

        if (crash is not null)
        {
            root.Controls.Add(new Label
            {
                AutoSize = true, MaximumSize = new Size(470, 0),
                Text = "Program napotkał nieoczekiwany błąd. Czy wysłać raport, żeby można go było naprawić? " +
                       "Raport zawiera opis błędu, wersję programu i systemu – żadnych Twoich danych.",
            });
            root.Controls.Add(new TextBox
            {
                Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Both, WordWrap = false, Height = 120,
                Dock = DockStyle.Fill, Font = new Font("Consolas", 8.5f), Text = crash.Replace("\n", Environment.NewLine),
                Margin = new Padding(0, 8, 0, 0),
            });
            _prompt.Text = "Co robiłeś(-aś) przed błędem? (opcjonalnie)";
            _message.Height = 70;
            _send.Text = "Wyślij raport";
            _cancel.Text = "Nie wysyłaj";
        }
        else
        {
            var kinds = new FlowLayoutPanel { AutoSize = true, Margin = new Padding(0) };
            kinds.Controls.Add(_bug);
            kinds.Controls.Add(_feature);
            root.Controls.Add(kinds);
            _bug.CheckedChanged += (_, _) => UpdatePrompt();
            _send.Text = "Wyślij";
            _cancel.Text = "Anuluj";
            UpdatePrompt();
        }
        root.Controls.Add(_prompt);
        root.Controls.Add(_message);
        root.Controls.Add(new Label { AutoSize = true, Margin = new Padding(0, 10, 0, 4), Text = "Twój e-mail (opcjonalnie – jeśli chcesz odpowiedź)" });
        root.Controls.Add(_email);
        if (crash is null) root.Controls.Add(_device);
        root.Controls.Add(_error);

        var buttons = new FlowLayoutPanel { FlowDirection = FlowDirection.RightToLeft, AutoSize = true, Dock = DockStyle.Fill, Margin = new Padding(0, 12, 0, 0) };
        buttons.Controls.Add(_send);
        buttons.Controls.Add(_cancel);
        buttons.Controls.Add(new Label { AutoSize = true, ForeColor = SystemColors.GrayText, Margin = new Padding(0, 8, 12, 0), Text = "Trafi do autora przez redzikowski.com" });
        root.Controls.Add(buttons);

        AcceptButton = _send;
        CancelButton = _cancel;
        _cancel.Click += (_, _) => Close();
        _send.Click += async (_, _) => await SendAsync();
        _message.TextChanged += (_, _) => UpdateSend();
        FormClosing += (_, e) => e.Cancel = _busy;
        UpdateSend();
    }

    private void UpdatePrompt() =>
        _prompt.Text = _feature.Checked ? "Czego brakuje albo co można zrobić lepiej?" : "Co się stało? Co robiłeś(-aś) tuż przed błędem?";

    private void UpdateSend() => _send.Enabled = !_busy && (_crash is not null || _message.Text.Trim().Length >= 3);

    private async Task SendAsync()
    {
        if (!Feedback.EmailOk(_email.Text))
        {
            ShowError("To nie wygląda na adres e-mail.");
            return;
        }
        _error.Visible = false;
        _busy = true;
        _cancel.Enabled = false;
        var label = _send.Text;
        _send.Text = "Wysyłanie…";
        UpdateSend();
        var kind = _crash is not null ? "crash" : _feature.Checked ? "feature" : "bug";
        var error = await Feedback.SendAsync(kind, _message.Text, _email.Text, _crash is not null || _device.Checked, _crash);
        _busy = false;
        _cancel.Enabled = true;
        _send.Text = label;
        UpdateSend();
        if (error is not null)
        {
            ShowError(error);
            return;
        }
        MessageBox.Show(this,
            (_crash is null ? "Dziękuję! Zgłoszenie dotarło." : "Dziękuję! Raport został wysłany.") + "\n" +
            (_email.Text.Trim().Length > 0 ? "Przeczytam je i odezwę się na podany adres, jeśli będzie trzeba." : "Przeczytam je i wezmę pod uwagę przy kolejnych wersjach."),
            "OpenTakeoff Link", MessageBoxButtons.OK, MessageBoxIcon.Information);
        Close();
    }

    private void ShowError(string text)
    {
        _error.Text = text;
        _error.Visible = true;
    }
}
