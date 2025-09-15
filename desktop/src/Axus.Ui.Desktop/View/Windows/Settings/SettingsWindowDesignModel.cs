using Avalonia.Controls;
using Omnius.Axus.Ui.Desktop.Shared;
using Omnius.Core.Base;
using Reactive.Bindings;
using Reactive.Bindings.Extensions;

namespace Omnius.Axus.Ui.Desktop.View.Windows;

public class SettingsWindowDesignModel : SettingsWindowModelBase
{
    private readonly CompositeDisposable _disposable = new();

    public SettingsWindowDesignModel()
    {
        this.Status = new SettingsWindowStatus();

        this.OkCommand = new AsyncReactiveCommand().AddTo(_disposable);
        this.OkCommand.Subscribe(this.OkAsync).AddTo(_disposable);

        this.CancelCommand = new AsyncReactiveCommand().AddTo(_disposable);
        this.CancelCommand.Subscribe(this.CancelAsync).AddTo(_disposable);
    }

    public override async ValueTask InitializeAsync(CancellationToken cancellationToken = default)
    {
    }

    protected override async ValueTask OnDisposeAsync()
    {
        _disposable.Dispose();
    }

    private async Task OkAsync(object? state)
    {
        if (state is Window window)
        {
            window.Close();
        }
    }

    private async Task CancelAsync(object? state)
    {
        if (state is Window window)
        {
            window.Close();
        }
    }
}
