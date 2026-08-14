using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Media;
using TokenViewerWindows.Services;
using TokenViewerWindows.ViewModels;
using UserControl = System.Windows.Controls.UserControl;
using Brush = System.Windows.Media.Brush;
using Brushes = System.Windows.Media.Brushes;
using Color = System.Windows.Media.Color;

namespace TokenViewerWindows.Views;

public partial class LimitsView : UserControl
{
    public LimitsView() => InitializeComponent();

    private void OnRefreshClick(object sender, RoutedEventArgs e)
    {
        if (DataContext is LimitsViewModel vm) _ = vm.RefreshAsync();
    }
}

public sealed class AgentNameConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is string s ? AgentRegistry.DisplayName(s) : "";
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class BrandColorConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not string s) return Brushes.Gray;
        var hex = AgentRegistry.BrandColorHex(s).TrimStart('#');
        if (hex.Length != 6) return Brushes.Gray;
        try
        {
            return new SolidColorBrush(Color.FromRgb(
                byte.Parse(hex[..2], NumberStyles.HexNumber),
                byte.Parse(hex[2..4], NumberStyles.HexNumber),
                byte.Parse(hex[4..6], NumberStyles.HexNumber)));
        }
        catch
        {
            return Brushes.Gray;
        }
    }
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

/// <summary>Deterministic responsive column count: 2 columns at content width
/// ≥ 1100 (1240×860 window), 1 column below (1000×700 window).</summary>
public static class LimitsLayout
{
    public const double TwoColumnThreshold = 1100;

    public static int ColumnsForWidth(double width) => width >= TwoColumnThreshold ? 2 : 1;
}

public sealed class WidthToColumnsConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is double w ? LimitsLayout.ColumnsForWidth(w) : 1;
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

/// <summary>Multi-converter: (resetAt, now) → localized countdown string.</summary>
public sealed class CountdownConverter : IMultiValueConverter
{
    public object Convert(object[] values, Type targetType, object? parameter, CultureInfo culture)
    {
        if (values.Length >= 2 && values[0] is DateTime reset && values[1] is DateTime now)
        {
            return L10n.Instance.CountdownText(reset - now);
        }
        return "";
    }
    public object[] ConvertBack(object value, Type[] targetTypes, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}
