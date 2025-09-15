using Microsoft.Extensions.DependencyInjection;
using Omnius.Axus.Ui.Desktop.View;
using Omnius.Axus.Ui.Desktop.View.Windows;
using Omnius.Core.Avalonia;
using Omnius.Core.Base;
using Omnius.Core.Base.Helpers;

namespace Omnius.Axus.Ui.Desktop.Shared;

public partial class Bootstrapper : AsyncDisposableBase
{
    private static readonly NLog.Logger _logger = NLog.LogManager.GetCurrentClassLogger();

    private AxusEnvironment? _axusEnvironment;
    private ServiceProvider? _serviceProvider;

    public static Bootstrapper Instance { get; } = new Bootstrapper();

    private const string UI_STATUS_FILE_NAME = "ui_status.json";

    private Bootstrapper()
    {
    }

    public async ValueTask BuildAsync(AxusEnvironment axusEnvironment, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(axusEnvironment);

        _axusEnvironment = axusEnvironment;

        try
        {
            var tempDirectoryPath = Path.Combine(_axusEnvironment.StorageDirectoryPath, "tmp");
            if (Directory.Exists(tempDirectoryPath)) Directory.Delete(tempDirectoryPath, true);
            DirectoryHelper.CreateDirectory(tempDirectoryPath);

            var bytesPool = BytesPool.Shared;

            var uiStatus = await UiStatus.LoadAsync(Path.Combine(_axusEnvironment.StateDirectoryPath, UI_STATUS_FILE_NAME));

            var serviceCollection = new ServiceCollection();

            serviceCollection.AddSingleton(_axusEnvironment);
            serviceCollection.AddSingleton<IBytesPool>(bytesPool);
            serviceCollection.AddSingleton(uiStatus);

            serviceCollection.AddSingleton<IApplicationDispatcher, ApplicationDispatcher>();
            serviceCollection.AddSingleton<IMainWindowProvider, MainWindowProvider>();
            serviceCollection.AddSingleton<IClipboardService, ClipboardService>();
            serviceCollection.AddSingleton<IDialogService, DialogService>();

            serviceCollection.AddTransient<MainWindowModel>();
            serviceCollection.AddTransient<SettingsWindowModel>();

            _serviceProvider = serviceCollection.BuildServiceProvider();
        }
        catch (OperationCanceledException e)
        {
            _logger.Debug(e, "Operation Canceled");
        }
        catch (Exception e)
        {
            _logger.Error(e, "Unexpected Exception");

            throw;
        }
    }

    protected override async ValueTask OnDisposeAsync()
    {
        if (_axusEnvironment is null) return;
        if (_serviceProvider is null) return;

        var uiStatus = _serviceProvider.GetRequiredService<UiStatus>();
        await uiStatus.SaveAsync(Path.Combine(_axusEnvironment.StateDirectoryPath, UI_STATUS_FILE_NAME));
    }

    public ServiceProvider GetServiceProvider()
    {
        return _serviceProvider ?? throw new NullReferenceException();
    }
}
