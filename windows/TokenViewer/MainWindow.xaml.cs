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

    private void OnClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        e.Cancel = true;
        Hide();
    }
}
