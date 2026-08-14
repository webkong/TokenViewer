using System.IO;
using System.Text.RegularExpressions;
using TokenViewerWindows.Services;
using Xunit;

namespace TokenViewerWindows.Tests;

public class LocalizationCoverageTests
{
    private static readonly string[] UsageXamlFiles =
    {
        "UsageView.xaml",
        "TrendChartControl.xaml",
        "HeatmapControl.xaml",
        "LimitsView.xaml",
        "SettingsView.xaml",
        "AboutView.xaml",
        "PopoverWindow.xaml",
    };

    // Brand names, technical identifiers and symbols that are not localizable.
    private static readonly HashSet<string> WhitelistedLiterals = new(StringComparer.OrdinalIgnoreCase)
    {
        "TokenViewer", "GitHub", "tokenviewer.webkong.top", "↻", "⚙",
    };

    private static string RepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            if (File.Exists(Path.Combine(dir.FullName, "windows", "TokenViewer", "TokenViewer.csproj")))
                return dir.FullName;
            dir = dir.Parent;
        }
        throw new InvalidOperationException("repo root not found");
    }

    private static string ViewPath(string fileName) =>
        Path.Combine(RepoRoot(), "windows", "TokenViewer", "Views", fileName);

    [Fact]
    public void Usage_xaml_contains_no_hardcoded_user_strings()
    {
        // Catches Text=/Header=/Content=/ToolTip= attributes holding a literal
        // with letters. Bindings ({...}), non-word literals, and whitelisted
        // brand/technical identifiers are allowed.
        var pattern = new Regex("(Text|Header|Content|ToolTip)=\"([^\"]*)\"", RegexOptions.Compiled);
        foreach (var fileName in UsageXamlFiles)
        {
            var xml = File.ReadAllText(ViewPath(fileName));
            foreach (Match m in pattern.Matches(xml))
            {
                var value = m.Groups[2].Value;
                if (value.Length == 0 || value.Contains('{')) continue;
                if (WhitelistedLiterals.Contains(value)) continue;
                if (Regex.IsMatch(value, "[A-Za-z]"))
                {
                    Assert.Fail($"hardcoded string in {fileName}: {m.Groups[1].Value}=\"{value}\"");
                }
            }
        }
    }

    [Fact]
    public void Every_l10n_key_used_in_xaml_exists_in_catalog()
    {
        var pattern = new Regex(@"\{views:L10n ([A-Za-z0-9_]+)\}", RegexOptions.Compiled);
        foreach (var fileName in UsageXamlFiles)
        {
            var xml = File.ReadAllText(ViewPath(fileName));
            foreach (Match m in pattern.Matches(xml))
            {
                var key = m.Groups[1].Value;
                Assert.True(L10n.Catalog.ContainsKey(key), $"{fileName} references missing key '{key}'");
            }
        }
    }
}
