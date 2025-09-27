using Avalonia;
using Avalonia.Markup.Xaml;
using Omnius.Core.Avalonia;

namespace Omnius.Axus.Ui.Desktop.View.Windows;

public partial class MainWindow : RestorableWindow
{
    public MainWindow()
        : base()
    {
        this.InitializeComponent();
    }

    public MainWindow(string configDirectoryPath)
        : base(configDirectoryPath)
    {
        this.InitializeComponent();

#if DEBUG
        this.AttachDevTools();
#endif
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);

        this.Closed += new EventHandler((_, _) => this.OnClosed());
    }

    private async void OnClosed()
    {
        if (this.DataContext is IAsyncDisposable disposable)
        {
            await disposable.DisposeAsync();
        }
    }
}
