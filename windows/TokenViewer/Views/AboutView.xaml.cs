using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using TokenViewerWindows.Services;
using UserControl = System.Windows.Controls.UserControl;

namespace TokenViewerWindows.Views;

public partial class AboutView : UserControl
{
    public AboutView()
    {
        InitializeComponent();
        var sources = AgentRegistry.UserFacingSources;
        AgentChips.ItemsSource = sources.Select(AgentRegistry.DisplayName).OrderBy(n => n, StringComparer.OrdinalIgnoreCase).ToList();
        AgentCountLabel.Text = L10n.Instance.AboutAgentCount(sources.Count);
        CopyrightLabel.Text = L10n.Instance.CopyrightFooter(DateTime.Now.Year);
    }

    private void OnToggleAgentsClick(object sender, RoutedEventArgs e)
    {
        AgentChips.Visibility = AgentChips.Visibility == Visibility.Visible ? Visibility.Collapsed : Visibility.Visible;
    }

    private void OnGitHubClick(object sender, RoutedEventArgs e) =>
        OpenUrl("https://github.com/webkong/TokenViewer");

    private void OnWebsiteClick(object sender, RoutedEventArgs e) =>
        OpenUrl("https://tokenviewer.webkong.top");

    private static void OpenUrl(string url)
    {
        try
        {
            Process.Start(new ProcessStartInfo { FileName = url, UseShellExecute = true });
        }
        catch
        {
            // Best-effort external open.
        }
    }
}
