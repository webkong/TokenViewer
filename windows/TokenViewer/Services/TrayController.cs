using System.Drawing;
using System.Windows.Forms;

namespace TokenViewerWindows;

public sealed class TrayController : IDisposable
{
    private readonly NotifyIcon _icon;
    private readonly Action _onOpenMainWindow;
    private readonly Action _onSyncNow;
    private readonly Action _onQuit;

    public TrayController(Action onOpenMainWindow, Action onSyncNow, Action onQuit)
    {
        _onOpenMainWindow = onOpenMainWindow;
        _onSyncNow = onSyncNow;
        _onQuit = onQuit;

        var menu = new ContextMenuStrip();
        menu.Items.Add("Open", null, (_, _) => _onOpenMainWindow());
        menu.Items.Add("Sync Now", null, (_, _) => _onSyncNow());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Quit", null, (_, _) => _onQuit());

        _icon = new NotifyIcon
        {
            Icon = SystemIcons.Application,
            Visible = false,
            Text = "TokenViewer",
            ContextMenuStrip = menu,
        };
        _icon.DoubleClick += (_, _) => _onOpenMainWindow();
    }

    public void Attach()
    {
        _icon.Visible = true;
    }

    public void Dispose()
    {
        _icon.Visible = false;
        _icon.Dispose();
    }
}

