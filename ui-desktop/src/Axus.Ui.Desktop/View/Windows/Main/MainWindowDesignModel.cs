using Omnius.Axus.Ui.Desktop.Shared;
using Omnius.Core.Base;
using Reactive.Bindings;
using Reactive.Bindings.Extensions;

namespace Omnius.Axus.Ui.Desktop.View.Windows;

public class MainWindowDesignModel : MainWindowModelBase
{
    private readonly CompositeDisposable _disposable = new();

    public MainWindowDesignModel()
    {
        this.Status = new MainWindowStatus();

        this.SettingsCommand = new AsyncReactiveCommand().AddTo(_disposable);
        this.SettingsCommand.Subscribe(this.SettingsAsync).AddTo(_disposable);
    }

    protected override async ValueTask OnDisposeAsync()
    {
        _disposable.Dispose();
    }

    private async Task SettingsAsync()
    {
        var window = new SettingsWindow();
        window.DataContext = new SettingsWindowDesignModel();
        await window.ShowDialog(App.Current!.MainWindow);
    }
}
