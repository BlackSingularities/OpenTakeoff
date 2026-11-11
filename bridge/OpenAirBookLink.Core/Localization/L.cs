using System.Collections.Concurrent;
using System.Globalization;
using System.Text.Json;

namespace OpenAirBookLink.Core.Localization;

// Języki programu: teksty w plikach Lang/<kod>.json (klucz → tekst, {0} = argument), wbudowanych w exe.
// Nowy język: dodaj plik JSON, kod w Codes i pozycję na liście w oknie.
public static class L
{
    public static readonly IReadOnlyList<string> Codes = new[] { "pl", "en", "es" };

    private static readonly ConcurrentDictionary<string, IReadOnlyDictionary<string, string>> Cache = new();
    private static volatile string _current = "en";

    public static string Current => _current;

    // Ustawienie "pl" / "en" / "es" albo SYSTEM (język Windows, a gdy go nie ma w programie — angielski).
    public static string Resolve(string? setting, CultureInfo system)
    {
        var code = setting?.ToLowerInvariant();
        if (code != null && Codes.Contains(code)) return code;
        return Codes.Contains(system.TwoLetterISOLanguageName) ? system.TwoLetterISOLanguageName : "en";
    }

    public static void Use(string code) => _current = code;

    public static IReadOnlyDictionary<string, string> Texts(string code) => Cache.GetOrAdd(code, Load);

    // Tekst dla klucza; brak w danym języku → angielski, brak także tam → sam klucz.
    public static string T(string key, params object?[] args)
    {
        if (!Texts(_current).TryGetValue(key, out var text) && !Texts("en").TryGetValue(key, out text)) return key;
        for (var i = 0; i < args.Length; i++) text = text.Replace("{" + i + "}", Convert.ToString(args[i], CultureInfo.CurrentCulture));
        return text;
    }

    private static IReadOnlyDictionary<string, string> Load(string code)
    {
        using var stream = typeof(L).Assembly.GetManifestResourceStream($"lang.{code}.json")
            ?? throw new InvalidOperationException($"Brak pliku języka {code}");
        return JsonSerializer.Deserialize<Dictionary<string, string>>(stream)
            ?? throw new InvalidOperationException($"Pusty plik języka {code}");
    }
}
