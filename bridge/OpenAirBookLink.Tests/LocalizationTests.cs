using System.Globalization;
using OpenAirBookLink.Core.Localization;
using Xunit;

namespace OpenAirBookLink.Tests;

public class LocalizationTests
{
    [Fact]
    public void EveryLanguageHasEveryKeyAndNoEmptyText()
    {
        var english = L.Texts("en");
        foreach (var code in L.Codes)
        {
            var texts = L.Texts(code);
            Assert.Equal(english.Keys.OrderBy(k => k, StringComparer.Ordinal), texts.Keys.OrderBy(k => k, StringComparer.Ordinal));
            Assert.DoesNotContain(texts, t => string.IsNullOrWhiteSpace(t.Value));
        }
    }

    [Theory]
    [InlineData("ES", "pl-PL", "es")]
    [InlineData("SYSTEM", "pl-PL", "pl")]
    [InlineData("SYSTEM", "es-ES", "es")]
    [InlineData("SYSTEM", "de-DE", "en")]
    [InlineData(null, "de-DE", "en")]
    public void ResolvesLanguage(string? setting, string culture, string expected) =>
        Assert.Equal(expected, L.Resolve(setting, new CultureInfo(culture)));
}
