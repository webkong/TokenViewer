using System.Threading;
using System.Windows;
using System.Windows.Forms;
using TokenViewerWindows;
using TokenViewerWindows.Views;
using Xunit;

namespace TokenViewerWindows.Tests;

public class TrayPlacementTests
{
    private static readonly Rect WorkArea = new(0, 0, 1920, 1040);
    private static readonly Size Popover = new(420, 680);

    [Fact]
    public void Clamp_keeps_popover_inside_work_area()
    {
        var topLeft = TrayPlacement.Clamp(new Point(-500, -500), Popover, WorkArea);
        Assert.True(topLeft.X >= WorkArea.Left);
        Assert.True(topLeft.Y >= WorkArea.Top);

        var bottomRight = TrayPlacement.Clamp(new Point(2000, 2000), Popover, WorkArea);
        Assert.True(bottomRight.X + Popover.Width <= WorkArea.Right);
        Assert.True(bottomRight.Y + Popover.Height <= WorkArea.Bottom);
    }

    [Fact]
    public void ComputePosition_opens_upward_for_bottom_taskbar()
    {
        var anchor = new Rect(1800, 1030, 20, 20); // near bottom edge
        var pos = TrayPlacement.ComputePosition(anchor, Popover, WorkArea);
        Assert.True(pos.Y < anchor.Top); // opens above the icon
        Assert.True(pos.Y >= WorkArea.Top);
        Assert.True(pos.X + Popover.Width <= WorkArea.Right);
    }

    [Fact]
    public void ComputePosition_opens_downward_for_top_taskbar()
    {
        var anchor = new Rect(100, 0, 20, 20); // near top edge
        var pos = TrayPlacement.ComputePosition(anchor, Popover, WorkArea);
        Assert.True(pos.Y > anchor.Top); // opens below the icon
        Assert.True(pos.Y + Popover.Height <= WorkArea.Bottom);
    }

    [Fact]
    public void ComputePosition_result_is_always_inside_work_area()
    {
        var anchors = new[]
        {
            new Rect(0, 0, 20, 20),        // top-left
            new Rect(1900, 1020, 20, 20),  // bottom-right
            new Rect(0, 520, 20, 20),      // left-middle
            new Rect(1900, 520, 20, 20),   // right-middle
            new Rect(960, 0, 20, 20),      // top-middle
            new Rect(960, 1020, 20, 20),   // bottom-middle
        };

        foreach (var anchor in anchors)
        {
            var pos = TrayPlacement.ComputePosition(anchor, Popover, WorkArea);
            Assert.True(pos.X >= WorkArea.Left, $"x too small for {anchor}");
            Assert.True(pos.Y >= WorkArea.Top, $"y too small for {anchor}");
            Assert.True(pos.X + Popover.Width <= WorkArea.Right, $"x too large for {anchor}");
            Assert.True(pos.Y + Popover.Height <= WorkArea.Bottom, $"y too large for {anchor}");
        }
    }

    [Fact]
    public void ToDips_scales_physical_pixels_to_dips()
    {
        var physical = new Rect(1920, 0, 1920, 1080);
        var dips = TrayPlacement.ToDips(physical, 1.5);
        Assert.Equal(1280, dips.X, 5);
        Assert.Equal(0, dips.Y, 5);
        Assert.Equal(1280, dips.Width, 5);
        Assert.Equal(720, dips.Height, 5);
    }

    [Fact]
    public void ComputePosition_handles_secondary_monitor_at_150_percent()
    {
        // Secondary monitor physical (1920,0)-(3840,1080) at 150% DPI → DIP (1280,0)-(2560,720).
        var workAreaDip = new Rect(1280, 0, 1280, 720);
        var anchorDip = new Rect(2540, 700, 20, 20); // near bottom-right of the secondary DIP monitor
        var pos = TrayPlacement.ComputePosition(anchorDip, Popover, workAreaDip);
        Assert.True(pos.X >= workAreaDip.Left);
        Assert.True(pos.Y >= workAreaDip.Top);
        Assert.True(pos.X + Popover.Width <= workAreaDip.Right);
        Assert.True(pos.Y + Popover.Height <= workAreaDip.Bottom);
    }

    [Fact]
    public void ResolveAnchor_prefers_valid_icon_rect_over_cursor()
    {
        var icon = new Rect(100, 200, 20, 20);
        var cursor = new Point(400, 500);
        Assert.Equal(icon, TrayPlacement.ResolveAnchor(icon, cursor));
    }

    [Fact]
    public void ResolveAnchor_falls_back_to_cursor_when_icon_invalid_or_missing()
    {
        var cursor = new Point(400, 500);
        Assert.Equal(new Rect(400, 500, 0, 0), TrayPlacement.ResolveAnchor(null, cursor));
        Assert.Equal(new Rect(400, 500, 0, 0), TrayPlacement.ResolveAnchor(new Rect(10, 10, 0, 0), cursor));
    }

    [Fact]
    public void NotifyIcon_identity_returns_nonzero_hwnd()
    {
        RunSta(() =>
        {
            using var icon = new NotifyIcon { Icon = System.Drawing.SystemIcons.Application };
            icon.Visible = true;

            var identity = TrayController.GetIconIdentity(icon);

            Assert.NotNull(identity);
            Assert.NotEqual(IntPtr.Zero, identity.Value.Hwnd);
        });
    }

    [Theory]
    [InlineData(0, true)]
    [InlineData(1, false)]
    [InlineData(-2147467259, false)] // E_FAIL
    public void IsSuccess_treats_only_s_ok_as_success(int hresult, bool expected)
    {
        Assert.Equal(expected, TrayController.IsSuccess(hresult));
    }

    private static void RunSta(Action action)
    {
        Exception? captured = null;
        var thread = new Thread(() =>
        {
            try
            {
                action();
            }
            catch (Exception ex)
            {
                captured = ex;
            }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (captured is not null)
        {
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(captured).Throw();
        }
    }
}
