using System.Windows.Forms;
using Microsoft.Win32;

namespace TokenViewerWindows.Services;

public static class LaunchAtStartupManager
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "TokenViewer";
    public const string StartupArgument = "--startup";

    public static bool IsEnabled
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: false);
            var value = key?.GetValue(ValueName) as string;
            return !string.IsNullOrWhiteSpace(value) && value.Contains(StartupArgument, StringComparison.OrdinalIgnoreCase);
        }
    }

    public static void SetEnabled(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true);
        if (enabled)
        {
            var exe = Environment.ProcessPath ?? Application.ExecutablePath;
            key?.SetValue(ValueName, $"\"{exe}\" {StartupArgument}");
        }
        else
        {
            key?.DeleteValue(ValueName, throwOnMissingValue: false);
        }
    }
}
