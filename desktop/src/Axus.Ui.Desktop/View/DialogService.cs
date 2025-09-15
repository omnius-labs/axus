using Microsoft.Extensions.DependencyInjection;
using Omnius.Axus.Ui.Desktop.Shared;
using Omnius.Axus.Ui.Desktop.View.Windows;
using Omnius.Core.Avalonia;

namespace Omnius.Axus.Ui.Desktop.View;

public interface IDialogService
{
    ValueTask ShowSettingsWindowAsync(CancellationToken cancellationToken = default);
}

public class DialogService : IDialogService
{
    private readonly AxusEnvironment _axusEnvironment;
    private readonly IApplicationDispatcher _applicationDispatcher;
    private readonly IMainWindowProvider _mainWindowProvider;
    private readonly IClipboardService _clipboardService;

    public DialogService(AxusEnvironment axusEnvironment, IApplicationDispatcher applicationDispatcher, IMainWindowProvider mainWindowProvider, IClipboardService clipboardService)
    {
        _axusEnvironment = axusEnvironment;
        _applicationDispatcher = applicationDispatcher;
        _mainWindowProvider = mainWindowProvider;
        _clipboardService = clipboardService;
    }

    public async ValueTask ShowSettingsWindowAsync(CancellationToken cancellationToken = default)
    {
        await _applicationDispatcher.InvokeAsync(async () =>
        {
            var window = new SettingsWindow(Path.Combine(_axusEnvironment.StateDirectoryPath, "windows", "settings"));
            var serviceProvider = Bootstrapper.Instance.GetServiceProvider();

            var viewModel = serviceProvider.GetRequiredService<SettingsWindowModel>();
            await viewModel.InitializeAsync(cancellationToken);
            window.DataContext = viewModel;

            await window.ShowDialog(_mainWindowProvider.GetMainWindow());
        });
    }
}
