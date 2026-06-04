using System.Linq;
using System.Windows;
using TokenViewerWindows.ViewModels;
using TokenViewerWindows.Services;

namespace TokenViewerWindows;

public partial class App : Application
{
    private CoreBridge? _core;
    private TrayController? _tray;
    private ShellViewModel? _shell;
    private MainWindow? _mainWindow;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        var launchedAtStartup = e.Args.Any(a => string.Equals(a, LaunchAtStartupManager.StartupArgument, StringComparison.OrdinalIgnoreCase));
        _core = CoreBridge.CreateDefault();
        _shell = new ShellViewModel(_core, Dispatcher);
        _tray = new TrayController(
            onOpenMainWindow: ShowMainWindow,
            onSyncNow: () => _ = _shell?.Main.SyncAsync(),
            onQuit: ShutdownApp);

        _mainWindow = new MainWindow(_shell);
        if (!launchedAtStartup)
        {
            _mainWindow.Show();
        }
        _tray.Attach();
        _ = _shell.Main.SyncAsync();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _tray?.Dispose();
        _core?.Dispose();
        base.OnExit(e);
    }

    private void ShowMainWindow()
    {
        if (_mainWindow is null)
        {
            _shell ??= new ShellViewModel(_core ?? CoreBridge.CreateDefault(), Dispatcher);
            _mainWindow = new MainWindow(_shell);
        }

        if (!_mainWindow.IsVisible)
        {
            _mainWindow.Show();
        }

        if (_mainWindow.WindowState == WindowState.Minimized)
        {
            _mainWindow.WindowState = WindowState.Normal;
        }

        _mainWindow.Activate();
        _mainWindow.Topmost = true;
        _mainWindow.Topmost = false;
        _mainWindow.Focus();
    }

    private void ShutdownApp()
    {
        _mainWindow?.AllowClose();
        _mainWindow?.Close();
        Shutdown();
    }
}
