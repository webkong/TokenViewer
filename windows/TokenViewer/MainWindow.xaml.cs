using System.ComponentModel;
using System.Windows;
using TokenViewerWindows.ViewModels;

namespace TokenViewerWindows;

public partial class MainWindow : Window
{
    public ShellViewModel Shell { get; }
    private bool _allowClose;

    public MainWindow(ShellViewModel shell)
    {
        InitializeComponent();
        Shell = shell;
        DataContext = Shell;

        Closing += OnClosing;
    }

    public void AllowClose() => _allowClose = true;

    /// <summary>When true, closing the window exits the app instead of hiding
    /// (used when the tray icon is hidden and no other entry point remains).</summary>
    public Func<bool> ShouldExitOnClose { get; set; } = () => false;

    public void SelectTab(string tab)
    {
        switch (tab)
        {
            case "settings":
                SettingsTab.IsSelected = true;
                break;
            case "limits":
                LimitsTab.IsSelected = true;
                break;
            default:
                UsageTab.IsSelected = true;
                break;
        }
    }

    private void OnClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        if (ShouldExitOnClose()) return; // no reachable entry point → exit
        e.Cancel = true;
        Hide();
    }
}
