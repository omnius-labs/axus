using Omnius.Axus.Ui.Desktop.Shared;
using Omnius.Core.Base;
using Reactive.Bindings;
using Reactive.Bindings.Extensions;

namespace Omnius.Axus.Ui.Desktop.View.Windows;

public abstract class MainWindowModelBase : AsyncDisposableBase
{
    public MainWindowStatus? Status { get; protected set; }
    public AsyncReactiveCommand? SettingsCommand { get; protected set; }
}

public class MainWindowModel : MainWindowModelBase
{
    private readonly IDialogService _dialogService;

    private readonly CompositeDisposable _disposable = new();

    public MainWindowModel(UiStatus uiStatus, IDialogService dialogService)
    {
        _dialogService = dialogService;

        this.Status = uiStatus.MainWindow ??= new MainWindowStatus();

        this.SettingsCommand = new AsyncReactiveCommand().AddTo(_disposable);
        this.SettingsCommand.Subscribe(this.ShowSettingsWindow).AddTo(_disposable);
    }

    protected override async ValueTask OnDisposeAsync()
    {
        _disposable.Dispose();
    }

    private async Task ShowSettingsWindow()
    {
        await _dialogService.ShowSettingsWindowAsync();
    }
}
