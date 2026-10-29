using System.Diagnostics;
using System.Net.Http.Headers;
using System.Reflection;
using System.Security.Cryptography;
using System.Text.Json;

namespace OpenAirBookLink;

public sealed record LinkUpdate(Version Version, string Tag, string ExeUrl, long Size, string? Sha256, string PageUrl);

/// <summary>
/// Samoaktualizacja z GitHub Releases. Tag ma postać vX.Y.Z i tę samą wersję CI wpisuje do exe
/// (-p:Version), więc porównujemy System.Version. Lokalne buildy mają wersję 0.0.0 i się nie aktualizują.
///
/// Podmiana działającego pliku: Windows nie pozwala go usunąć, ale pozwala zmienić nazwę.
/// Nowy plik pobieramy obok jako .new, bieżący przenosimy na .old, .new na miejsce bieżącego,
/// uruchamiamy go z --updated i kończymy proces. Nowa instancja sprząta .old przy starcie.
/// </summary>
public static class Updater
{
    public const string Repo = "BlackSingularities/OpenTakeoff";
    private const string AssetName = "OpenTakeoff-Link.exe";
    public const string UpdatedArg = "--updated";

    // Kolejność ma znaczenie: CreateClient() czyta Current.
    public static Version Current { get; } =
        Assembly.GetExecutingAssembly().GetName().Version is { } v ? new Version(v.Major, v.Minor, Math.Max(v.Build, 0)) : new Version(1, 0, 0);

    private static readonly HttpClient Http = CreateClient();

    /// <summary>Ścieżka exe, z którego działamy; null dla buildów lokalnych (wersja 0.0.0) i spoza single-file (np. dotnet run).</summary>
    public static string? ExePath
    {
        get
        {
            if (Current == new Version(0, 0, 0)) return null;
            var path = Environment.ProcessPath;
            if (path is null || !string.Equals(Path.GetExtension(path), ".exe", StringComparison.OrdinalIgnoreCase)) return null;
            // W trybie single-file Assembly.Location jest pusty; przy zwykłym buildzie obok leży .dll i podmiana exe nic by nie dała.
#pragma warning disable IL3000 // właśnie o to pytamy: pusty Location = single-file
            return string.IsNullOrEmpty(typeof(Updater).Assembly.Location) ? path : null;
#pragma warning restore IL3000
        }
    }

    public static async Task<LinkUpdate?> CheckAsync(CancellationToken ct = default)
    {
        using var req = new HttpRequestMessage(HttpMethod.Get, $"https://redzikowski.com/api/github/repos/{Repo}/releases/latest");
        req.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
        using var resp = await Http.SendAsync(req, ct);
        if (!resp.IsSuccessStatusCode) return null;
        using var doc = JsonDocument.Parse(await resp.Content.ReadAsStringAsync(ct));
        var root = doc.RootElement;
        var tag = root.GetProperty("tag_name").GetString() ?? "";
        if (!Version.TryParse(tag.TrimStart('v'), out var latest) || latest <= Current) return null;

        // Release powstaje przed doklejeniem exe (osobny job) — bez pliku po prostu sprawdzimy później.
        foreach (var asset in root.GetProperty("assets").EnumerateArray())
        {
            if (asset.GetProperty("name").GetString() != AssetName) continue;
            string? sha = null;
            if (asset.TryGetProperty("digest", out var d) && d.GetString() is { } digest && digest.StartsWith("sha256:"))
                sha = digest["sha256:".Length..];
            return new LinkUpdate(
                latest, tag,
                asset.GetProperty("browser_download_url").GetString()!,
                asset.GetProperty("size").GetInt64(),
                sha,
                root.TryGetProperty("html_url", out var page) ? page.GetString() ?? "" : "");
        }
        return null;
    }

    /// <summary>Pobiera nowy exe obok bieżącego jako .new i sprawdza rozmiar oraz SHA-256 (jeśli GitHub go podał).</summary>
    public static async Task<string> DownloadAsync(LinkUpdate update, IProgress<double>? progress, CancellationToken ct = default)
    {
        var exe = ExePath ?? throw new InvalidOperationException("Program nie działa z pliku exe — aktualizacja niemożliwa.");
        var target = exe + ".new";
        try
        {
            using var resp = await Http.GetAsync(update.ExeUrl, HttpCompletionOption.ResponseHeadersRead, ct);
            resp.EnsureSuccessStatusCode();
            var total = resp.Content.Headers.ContentLength ?? update.Size;
            await using (var input = await resp.Content.ReadAsStreamAsync(ct))
            await using (var output = new FileStream(target, FileMode.Create, FileAccess.Write, FileShare.None))
            {
                var buffer = new byte[81920];
                long done = 0;
                int n;
                while ((n = await input.ReadAsync(buffer, ct)) > 0)
                {
                    await output.WriteAsync(buffer.AsMemory(0, n), ct);
                    done += n;
                    if (total > 0) progress?.Report(Math.Clamp((double)done / total, 0, 1));
                }
            }

            var length = new FileInfo(target).Length;
            if (update.Size > 0 && length != update.Size)
                throw new IOException($"Niepełne pobranie ({length} z {update.Size} B)");
            if (update.Sha256 is { } expected)
            {
                await using var check = File.OpenRead(target);
                var actual = Convert.ToHexString(await SHA256.HashDataAsync(check, ct));
                if (!actual.Equals(expected, StringComparison.OrdinalIgnoreCase))
                    throw new IOException("Suma SHA-256 pobranego pliku się nie zgadza");
            }
            return target;
        }
        catch
        {
            TryDelete(target);
            throw;
        }
    }

    /// <summary>Podmienia exe na pobrany plik i uruchamia nową wersję. Po sukcesie wywołujący musi zakończyć proces.</summary>
    public static void ApplyAndRestart(string downloaded, IEnumerable<string> args)
    {
        var exe = ExePath ?? throw new InvalidOperationException("Program nie działa z pliku exe.");
        var old = exe + ".old";
        TryDelete(old);
        File.Move(exe, old);
        try
        {
            File.Move(downloaded, exe);
        }
        catch
        {
            File.Move(old, exe); // przywróć działającą wersję
            throw;
        }

        var psi = new ProcessStartInfo(exe) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(exe)! };
        foreach (var a in args.Where(a => a != UpdatedArg)) psi.ArgumentList.Add(a);
        psi.ArgumentList.Add(UpdatedArg);
        try
        {
            Process.Start(psi);
        }
        catch
        {
            File.Move(exe, downloaded, overwrite: true);
            File.Move(old, exe);
            throw;
        }
    }

    /// <summary>Usuwa pozostałości po poprzedniej aktualizacji (.old, nieudane .new).</summary>
    public static void CleanUp()
    {
        if (ExePath is not { } exe) return;
        // Stary proces może jeszcze przez chwilę trzymać plik — kilka prób.
        for (var i = 0; i < 10 && File.Exists(exe + ".old"); i++)
        {
            if (TryDelete(exe + ".old")) break;
            Thread.Sleep(300);
        }
        TryDelete(exe + ".new");
    }

    /// <summary>Czy możemy pisać w folderze z exe (np. Program Files bez uprawnień administratora — nie).</summary>
    public static bool CanWriteNextToExe()
    {
        if (ExePath is not { } exe) return false;
        var probe = Path.Combine(Path.GetDirectoryName(exe)!, $".write-test-{Environment.ProcessId}");
        try
        {
            File.WriteAllText(probe, "");
            File.Delete(probe);
            return true;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    private static bool TryDelete(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
            return true;
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    private static HttpClient CreateClient()
    {
        var client = new HttpClient { Timeout = TimeSpan.FromMinutes(5) };
        client.DefaultRequestHeaders.UserAgent.Add(new ProductInfoHeaderValue("OpenTakeoff-Link", Current.ToString()));
        return client;
    }
}
