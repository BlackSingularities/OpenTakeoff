using OpenAirBookLink.Core.Localization;
namespace OpenAirBookLink;

internal static class Program
{
    // Argumenty: --simulate / --no-simulate (nadpisują zapisany tryb), --minimized (start w zasobniku).
    [STAThread]
    private static void Main(string[] args)
    {
        // --lang=xx wymusza język na to uruchomienie
        var language = args.FirstOrDefault(a => a.StartsWith("--lang="))?["--lang=".Length..] ?? LinkHost.SavedLanguage;
        L.Use(L.Resolve(language, System.Globalization.CultureInfo.InstalledUICulture));

        using var mutex = new Mutex(true, "OpenTakeoff-Link-single-instance", out var first);
        // Po samoaktualizacji i zmianie języka poprzednia wersja jeszcze się zamyka — czekamy, aż zwolni mutex (i port).
        if (!first && (args.Contains(Updater.UpdatedArg) || args.Contains(MainForm.RestartArg)))
        {
            try { first = mutex.WaitOne(TimeSpan.FromSeconds(20)); }
            catch (AbandonedMutexException) { first = true; }
        }
        if (!first)
        {
            MessageBox.Show(L.T("app.already_running"),
                "OpenTakeoff Link", MessageBoxButtons.OK, MessageBoxIcon.Information);
            return;
        }

        bool? simulate = args.Contains("--simulate") ? true : args.Contains("--no-simulate") ? false : null;
        var minimized = args.Contains("--minimized");
        Task.Run(Updater.CleanUp);

        ApplicationConfiguration.Initialize();
        Feedback.Install();
        using var host = new LinkHost(simulate);
        using var form = new MainForm(host, minimized, args);
        try
        {
            host.Start();
        }
        catch (System.Net.Sockets.SocketException e)
        {
            MessageBox.Show(L.T("app.port_busy", Core.Protocol.LinkProtocol.WebSocketPort, e.Message), "OpenTakeoff Link", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return;
        }
        Feedback.OfferPendingCrash(null);
        Application.Run(form);
    }
}
