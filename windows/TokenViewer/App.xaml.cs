using System.IO;
using System.Linq;
using System.Windows;
using TokenViewerWindows.ViewModels;
using TokenViewerWindows.Services;
using TokenViewerWindows.Views;

namespace TokenViewerWindows;

public partial class App : System.Windows.Application
{
    private CoreBridge? _core;
    private TrayController? _tray;
    private ShellViewModel? _shell;
    private MainWindow? _mainWindow;
    private PopoverWindow? _popover;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        _core = CoreBridge.CreateDefault();
        if (!_core.IsReady && !TryRecoverInitFailure())
        {
            // Never continue with a zero handle as if the app were ready.
            Shutdown();
            return;
        }

        var launchedAtStartup = e.Args.Any(a => string.Equals(a, LaunchAtStartupManager.StartupArgument, StringComparison.OrdinalIgnoreCase));
        _shell = new ShellViewModel(_core, Dispatcher);
        _popover = new PopoverWindow(_shell, OpenMainWindow, ShutdownApp);
        _tray = new TrayController(
            onTogglePopover: TogglePopover,
            onOpenMainWindow: ShowMainWindow,
            onSyncNow: () => _ = _shell?.Sync.SyncAsync(),
            onQuit: ShutdownApp);

        // Show-menu-bar-icon setting takes effect immediately; hiding the tray
        // forces the main window to stay the reachable entry point.
        _shell.Settings.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(SettingsViewModel.ShowMenuBarIcon))
            {
                _tray?.SetVisible(_shell.Settings.ShowMenuBarIcon);
                if (!_shell.Settings.ShowMenuBarIcon)
                {
                    ShowMainWindow();
                }
            }
        };

        _mainWindow = new MainWindow(_shell);
        _mainWindow.ShouldExitOnClose = () => !_shell.Settings.ShowMenuBarIcon;
        if (!launchedAtStartup)
        {
            _mainWindow.Show();
        }
        _tray.Attach(_shell.Settings.ShowMenuBarIcon);
        _ = _shell.Sync.SyncAsync();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _popover?.Close();
        _tray?.Dispose();
        _core?.Dispose();
        base.OnExit(e);
    }

    private void TogglePopover()
    {
        if (_popover is null) return;
        var mouse = System.Windows.Forms.Control.MousePosition;
        var anchor = TrayPlacement.ResolveAnchor(
            _tray?.GetIconRect(),
            new System.Windows.Point(mouse.X, mouse.Y));
        _popover.Toggle(anchor);
    }

    private void OpenMainWindow(string tab)
    {
        ShowMainWindow();
        _mainWindow?.SelectTab(tab);
    }

    private bool TryRecoverInitFailure()
    {
        var dbPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            ".tokenviewer", "data.db");

        while (_core is { IsReady: false })
        {
            var result = System.Windows.MessageBox.Show(
                $"{L10n.Instance["initFailed"]}\n{dbPath}",
                "TokenViewer",
                System.Windows.MessageBoxButton.YesNo,
                System.Windows.MessageBoxImage.Error);
            if (result != System.Windows.MessageBoxResult.Yes)
            {
                return false;
            }
            _core = CoreBridge.CreateDefault();
        }
        return _core is { IsReady: true };
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
