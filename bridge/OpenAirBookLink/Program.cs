namespace OpenAirBookLink;

internal static class Program
{
    // Argumenty: --simulate / --no-simulate (nadpisują zapisany tryb), --minimized (start w zasobniku).
    [STAThread]
    private static void Main(string[] args)
    {
        using var mutex = new Mutex(true, "OpenTakeoff-Link-single-instance", out var first);
        // Po samoaktualizacji poprzednia wersja jeszcze się zamyka — czekamy, aż zwolni mutex (i port).
        if (!first && args.Contains(Updater.UpdatedArg))
        {
            try { first = mutex.WaitOne(TimeSpan.FromSeconds(20)); }
            catch (AbandonedMutexException) { first = true; }
        }
        if (!first)
        {
            MessageBox.Show("OpenTakeoff Link już działa — sprawdź ikonę w zasobniku obok zegara.",
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
            MessageBox.Show($"Nie można otworzyć portu {Core.Protocol.LinkProtocol.WebSocketPort}: {e.Message}\n" +
                            "Czy inny program go nie zajmuje?", "OpenTakeoff Link", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return;
        }
        Feedback.OfferPendingCrash(null);
        Application.Run(form);
    }
}
