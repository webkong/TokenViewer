using System.Windows;
using System.Windows.Controls;
using TokenViewerWindows.Services;
using TokenViewerWindows.ViewModels;
using UserControl = System.Windows.Controls.UserControl;

namespace TokenViewerWindows.Views;

public partial class SettingsView : UserControl
{
    public SettingsView() => InitializeComponent();

    private void OnRebuildClick(object sender, RoutedEventArgs e)
    {
        if (DataContext is not SettingsViewModel vm) return;
        var l = L10n.Instance;
        var result = System.Windows.MessageBox.Show(
            $"{l["rebuildConfirm"]}\n\n{l["rebuildDataDesc"]}",
            l["rebuildData"],
            System.Windows.MessageBoxButton.YesNo,
            System.Windows.MessageBoxImage.Warning);
        if (result == System.Windows.MessageBoxResult.Yes)
        {
            vm.RebuildCommand.Execute(null);
        }
    }

    private void OnResetClick(object sender, RoutedEventArgs e)
    {
        if (DataContext is not SettingsViewModel vm) return;
        var l = L10n.Instance;
        var result = System.Windows.MessageBox.Show(
            l["resetSettingsConfirmMessage"],
            l["resetSettingsConfirm"],
            System.Windows.MessageBoxButton.YesNo,
            System.Windows.MessageBoxImage.Warning);
        if (result == System.Windows.MessageBoxResult.Yes)
        {
            vm.ResetSettingsCommand.Execute(null);
        }
    }
}
