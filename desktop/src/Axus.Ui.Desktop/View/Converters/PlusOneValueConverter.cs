using System.Globalization;
using Avalonia.Data.Converters;

namespace Omnius.Axus.Ui.Desktop.View.Converters;

public class PlusOneValueConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is int intValue)
        {
            var v = intValue + 1;

            if (targetType == typeof(string)) return v.ToString(CultureInfo.InvariantCulture);
            return v;
        }

        return 1;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is double doubleValue) return (int)doubleValue - 1;
        else if (value is int intValue) return intValue - 1;
        else if (value is string strValue && int.TryParse(strValue, CultureInfo.InvariantCulture, out var parsedIntValue)) return parsedIntValue - 1;

        return 1;
    }
}
