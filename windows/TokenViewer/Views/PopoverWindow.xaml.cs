using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using TokenViewerWindows.ViewModels;
using Point = System.Windows.Point;
using Size = System.Windows.Size;
using Rect = System.Windows.Rect;
using KeyEventArgs = System.Windows.Input.KeyEventArgs;

namespace TokenViewerWindows.Views;

/// <summary>Borderless, topmost tray popover hosting the compact Usage panel.
/// Transient: hides on deactivation and on ESC; never destroys the shared
/// view models.</summary>
public partial class PopoverWindow : Window
{
    private readonly ShellViewModel _shell;
    private readonly Action<string> _openMainWindow;
    private readonly Action _onQuit;
    private bool _suppressNextToggle;

    public PopoverWindow(ShellViewModel shell, Action<string> openMainWindow, Action onQuit)
    {
        InitializeComponent();
        _shell = shell;
        _openMainWindow = openMainWindow;
        _onQuit = onQuit;
        DataContext = shell.Usage;
    }

    /// <summary>The shared shell, exposed so the XAML can bind to the shared
    /// <see cref="SyncCoordinator"/> (e.g. the sync button's IsEnabled).</summary>
    public ShellViewModel Shell => _shell;

    /// <summary>Show the popover anchored to a physical-pixel rect (tray icon or
    /// cursor fallback). The anchor and its monitor's work area are converted to
    /// DIP at that monitor's DPI scale, then clamped.</summary>
    public void ShowNear(Rect anchorPhysical)
    {
        var (workAreaPhysical, dpiScale) = NativeScreen.MonitorFor(anchorPhysical);
        var anchor = TrayPlacement.ToDips(anchorPhysical, dpiScale);
        var workArea = TrayPlacement.ToDips(workAreaPhysical, dpiScale);

        var pos = TrayPlacement.ComputePosition(anchor, new Size(Width, Height), workArea);
        Left = pos.X;
        Top = pos.Y;
        Show();
        Activate();
    }

    /// <summary>Toggle visibility. When the click that caused deactivation lands on
    /// the tray icon again, the deactivation has already hidden us — suppress the
    /// immediate re-show so a single click still closes the popover.</summary>
    public void Toggle(Rect anchorPhysical)
    {
        if (_suppressNextToggle)
        {
            _suppressNextToggle = false;
            return;
        }
        if (IsVisible) Hide();
        else ShowNear(anchorPhysical);
    }

    protected override void OnDeactivated(EventArgs e)
    {
        base.OnDeactivated(e);
        _suppressNextToggle = true;
        Hide();
        Dispatcher.BeginInvoke(new Action(() => _suppressNextToggle = false));
    }

    protected override void OnPreviewKeyDown(KeyEventArgs e)
    {
        base.OnPreviewKeyDown(e);
        if (e.Key == Key.Escape)
        {
            Hide();
            e.Handled = true;
        }
    }

    private void OnSyncClick(object sender, RoutedEventArgs e) => _ = _shell.Sync.SyncAsync();
    private void OnSettingsClick(object sender, RoutedEventArgs e) => _openMainWindow("settings");
    private void OnDashboardClick(object sender, RoutedEventArgs e) => _openMainWindow("usage");
    private void OnQuitClick(object sender, RoutedEventArgs e) => _onQuit();
}

/// <summary>Pure popover placement logic (all coordinates in DIP).</summary>
public static class TrayPlacement
{
    /// <summary>Compute the popover top-left from the anchor rect and work area,
    /// opening on the opposite side of the taskbar edge the anchor is docked on.</summary>
    public static Point ComputePosition(Rect anchor, Size popover, Rect workArea)
    {
        var topHalf = anchor.Top < workArea.Top + workArea.Height / 2;
        var leftHalf = anchor.Left < workArea.Left + workArea.Width / 2;

        var x = leftHalf ? anchor.Right : anchor.Left - popover.Width;
        var y = topHalf ? anchor.Bottom : anchor.Top - popover.Height;

        return Clamp(new Point(x, y), popover, workArea);
    }

    /// <summary>Clamp a proposed top-left so the popover is fully inside the work area.</summary>
    public static Point Clamp(Point proposed, Size popover, Rect workArea)
    {
        var x = Math.Clamp(proposed.X, workArea.Left, Math.Max(workArea.Left, workArea.Right - popover.Width));
        var y = Math.Clamp(proposed.Y, workArea.Top, Math.Max(workArea.Top, workArea.Bottom - popover.Height));
        return new Point(x, y);
    }

    /// <summary>Convert a physical-pixel rect to DIP at the given DPI scale
    /// (e.g. 1.5 = 150%).</summary>
    public static Rect ToDips(Rect physical, double dpiScale)
    {
        if (dpiScale <= 0) dpiScale = 1.0;
        return new Rect(
            physical.X / dpiScale,
            physical.Y / dpiScale,
            physical.Width / dpiScale,
            physical.Height / dpiScale);
    }

    /// <summary>Pick the anchor: the icon rect when valid, else a zero-size rect at
    /// the cursor. This is the API-result/fallback decision, kept pure for testing.</summary>
    public static Rect ResolveAnchor(Rect? iconRect, Point cursor)
    {
        if (iconRect is { } r && r.Width > 0 && r.Height > 0) return r;
        return new Rect(cursor.X, cursor.Y, 0, 0);
    }
}

/// <summary>Best-effort multi-monitor work-area + DPI lookup (Win32). Falls back to
/// the primary work area at 96 DPI when the APIs are unavailable.</summary>
internal static class NativeScreen
{
    private const uint MonitorDefaultToNearest = 2;
    private const int MdtEffectiveDpi = 0;

    public static (Rect WorkArea, double DpiScale) MonitorFor(Rect anchor)
    {
        try
        {
            var monitor = MonitorFromPoint(new POINT { X = (int)anchor.Left, Y = (int)anchor.Top }, MonitorDefaultToNearest);
            if (monitor != IntPtr.Zero)
            {
                var info = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
                if (GetMonitorInfo(monitor, ref info))
                {
                    var scale = 1.0;
                    if (GetDpiForMonitor(monitor, MdtEffectiveDpi, out var dpiX, out _) == 0 && dpiX > 0)
                        scale = dpiX / 96.0;

                    var work = new Rect(
                        info.rcWork.Left, info.rcWork.Top,
                        info.rcWork.Right - info.rcWork.Left,
                        info.rcWork.Bottom - info.rcWork.Top);
                    return (work, scale);
                }
            }
        }
        catch
        {
            // Fall through to the 96-DPI primary fallback.
        }

        var wa = SystemParameters.WorkArea;
        return (new Rect(wa.Left, wa.Top, wa.Width, wa.Height), 1.0);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X; public int Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MONITORINFO
    {
        public int cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public int dwFlags;
    }

    [DllImport("user32.dll")]
    private static extern IntPtr MonitorFromPoint(POINT pt, uint dwFlags);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);

    [DllImport("shcore.dll")]
    private static extern int GetDpiForMonitor(IntPtr hmonitor, int dpiType, out uint dpiX, out uint dpiY);
}

public sealed class FilterVisibleLimitsConverter : System.Windows.Data.IMultiValueConverter
{
    public object Convert(object[] values, Type targetType, object? parameter, System.Globalization.CultureInfo culture)
    {
        if (values.Length < 2 || values[0] is not IReadOnlyList<TokenViewerWindows.Models.AgentLimit> agents || values[1] is not string raw)
            return Array.Empty<TokenViewerWindows.Models.AgentLimit>();
        return agents.Where(a => LimitsVisibility.IsVisible(raw, a.Name)).ToList();
    }

    public object[] ConvertBack(object value, Type[] targetTypes, object? parameter, System.Globalization.CultureInfo culture) =>
        throw new NotSupportedException();
}
