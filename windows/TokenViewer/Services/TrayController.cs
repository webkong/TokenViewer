using System.Drawing;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using TokenViewerWindows.Services;

namespace TokenViewerWindows;

/// <summary>Reachability policy: when the tray icon is hidden there is no other
/// entry point, so closing the main window must exit the app.</summary>
public static class TrayReachabilityPolicy
{
    public static bool ShouldExitOnClose(bool showMenuBarIcon) => !showMenuBarIcon;
}

public sealed class TrayController : IDisposable
{
    private readonly NotifyIcon _icon;
    private readonly Action _onTogglePopover;
    private readonly Action _onOpenMainWindow;
    private readonly Action _onSyncNow;
    private readonly Action _onQuit;

    public TrayController(Action onTogglePopover, Action onOpenMainWindow, Action onSyncNow, Action onQuit)
    {
        _onTogglePopover = onTogglePopover;
        _onOpenMainWindow = onOpenMainWindow;
        _onSyncNow = onSyncNow;
        _onQuit = onQuit;

        var l10n = L10n.Instance;
        var menu = new ContextMenuStrip();
        menu.Items.Add(l10n["dashboard"], null, (_, _) => _onOpenMainWindow());
        menu.Items.Add(l10n["syncNow"], null, (_, _) => _onSyncNow());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add(l10n["quit"], null, (_, _) => _onQuit());

        _icon = new NotifyIcon
        {
            Icon = LoadAppIcon(),
            Visible = false,
            Text = l10n["appName"],
            ContextMenuStrip = menu,
        };
        // Left-click toggles the popover; right-click opens the context menu
        // (handled by ContextMenuStrip, so it does not also fire MouseClick).
        _icon.MouseClick += (_, e) =>
        {
            if (e.Button == MouseButtons.Left) _onTogglePopover();
        };
    }

    public void Attach(bool visible) => _icon.Visible = visible;

    /// <summary>Show/hide the tray icon immediately (driven by the show-menu-bar-icon
    /// setting).</summary>
    public void SetVisible(bool visible) => _icon.Visible = visible;

    public void Dispose()
    {
        _icon.Visible = false;
        _icon.Dispose();
    }

    /// <summary>Best-effort tray icon rect in physical pixels via
    /// <c>Shell_NotifyIconGetRect</c>. Returns null when the API/reflection fails,
    /// so callers fall back to the cursor position.</summary>
    public System.Windows.Rect? GetIconRect()
    {
        try
        {
            var identity = GetIconIdentity(_icon);
            if (identity is null) return null;
            var (hwnd, id) = identity.Value;

            // .NET 8 WinForms registers the icon in legacy uID + hWnd mode (no
            // GUID), so Shell_NotifyIconGetRect identifies it by Guid.Empty.
            var identifier = new NOTIFYICONIDENTIFIER
            {
                cbSize = (uint)Marshal.SizeOf<NOTIFYICONIDENTIFIER>(),
                hWnd = hwnd,
                uID = id,
                guidItem = Guid.Empty,
            };
            return IsSuccess(Shell_NotifyIconGetRect(ref identifier, out var rc))
                ? new System.Windows.Rect(rc.Left, rc.Top, rc.Right - rc.Left, rc.Bottom - rc.Top)
                : null;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>HRESULT success check — S_OK is 0; any non-zero value is failure.</summary>
    public static bool IsSuccess(int hresult) => hresult == 0;

    /// <summary>Recover the WinForms <see cref="NotifyIcon"/>'s hidden-window HWND
    /// and uID via reflection (best-effort; null on any mismatch). .NET 8 stores
    /// them in <c>_window</c> (with a public <c>Handle</c>) and <c>_id</c>.</summary>
    public static (IntPtr Hwnd, uint Id)? GetIconIdentity(NotifyIcon icon)
    {
        try
        {
            var type = icon.GetType();
            var windowField = type.GetField("_window", BindingFlags.Instance | BindingFlags.NonPublic);
            var idField = type.GetField("_id", BindingFlags.Instance | BindingFlags.NonPublic);
            if (windowField is null || idField is null) return null;

            var window = windowField.GetValue(icon);
            var handleProp = window?.GetType().GetProperty("Handle", BindingFlags.Instance | BindingFlags.Public);
            if (window is null || handleProp is null) return null;

            var hwnd = (IntPtr)handleProp.GetValue(window)!;
            var id = Convert.ToUInt32(idField.GetValue(icon));
            return (hwnd, id);
        }
        catch
        {
            return null;
        }
    }

    private static Icon LoadAppIcon()
    {
        var stream = System.Windows.Application.GetResourceStream(
            new Uri("pack://application:,,,/Resources/TokenViewer.ico"))?.Stream;
        return stream is null ? SystemIcons.Application : new Icon(stream);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NOTIFYICONIDENTIFIER
    {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID;
        public Guid guidItem;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left, Top, Right, Bottom;
    }

    [DllImport("shell32.dll", SetLastError = true)]
    private static extern int Shell_NotifyIconGetRect(ref NOTIFYICONIDENTIFIER identifier, out RECT iconLocation);
}
