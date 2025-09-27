using Avalonia.Controls;
using Omnius.Axus.Ui.Desktop.Shared;
using Omnius.Core.Base;
using Reactive.Bindings;
using Reactive.Bindings.Extensions;

namespace Omnius.Axus.Ui.Desktop.View.Windows;

public abstract class SettingsWindowModelBase : AsyncDisposableBase
{
    public abstract ValueTask InitializeAsync(CancellationToken cancellationToken = default);

    public SettingsWindowStatus? Status { get; protected set; }
    public AsyncReactiveCommand? OkCommand { get; protected set; }
    public AsyncReactiveCommand? CancelCommand { get; protected set; }
}

public class SettingsWindowModel : SettingsWindowModelBase
{
    private readonly UiStatus _uiState;
    private readonly IDialogService _dialogService;

    private readonly CompositeDisposable _disposable = new();

    public SettingsWindowModel(UiStatus uiState, IDialogService dialogService)
    {
        _uiState = uiState;
        _dialogService = dialogService;

        this.Status = _uiState.SettingsWindow ??= new SettingsWindowStatus();

        var serviceProvider = Bootstrapper.Instance.GetServiceProvider();

        this.OkCommand = new AsyncReactiveCommand().AddTo(_disposable);
        this.OkCommand.Subscribe(this.OkAsync).AddTo(_disposable);
        this.CancelCommand = new AsyncReactiveCommand().AddTo(_disposable);
        this.CancelCommand.Subscribe(this.CancelAsync).AddTo(_disposable);
    }

    public override async ValueTask InitializeAsync(CancellationToken cancellationToken = default)
    {
        await this.LoadAsync();
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

        await this.SaveAsync();
    }

    private async Task CancelAsync(object? state)
    {
        if (state is Window window)
        {
            window.Close();
        }
    }

    private async ValueTask LoadAsync(CancellationToken cancellationToken = default)
    {
    }

    private async ValueTask SaveAsync(CancellationToken cancellationToken = default)
    {
    }
}
