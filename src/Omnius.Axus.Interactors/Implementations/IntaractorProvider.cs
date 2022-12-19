using Omnius.Core;
using Omnius.Core.Storages;

namespace Omnius.Axus.Interactors;

public partial class InteractorProvider : AsyncDisposableBase, IInteractorProvider
{
    private static readonly NLog.Logger _logger = NLog.LogManager.GetCurrentClassLogger();

    private string _databaseDirectoryPath;
    private IServiceMediator? _serviceMediator;
    private readonly IBytesPool _bytesPool;

    private IProfilePublisher? _profilePublisher;
    private IProfileSubscriber? _profileSubscriber;
    private IBarkUploader? _barkPublisher;
    private IBarkDownloader? _barkSubscriber;
    private IDisposable? _barkSubscriberOnGetTrustedSignaturesRegister;
    private IFileUploader? _fileUploader;
    private IFileDownloader? _fileDownloader;

    private readonly AsyncLock _asyncLock = new();

    public static async ValueTask<InteractorProvider> CreateAsync(string databaseDirectoryPath, IServiceMediator serviceMediator, IBytesPool bytesPool, CancellationToken cancellationToken = default)
    {
        var result = new InteractorProvider(databaseDirectoryPath, serviceMediator, bytesPool);
        await result.InitAsync(cancellationToken);
        return result;
    }

    internal InteractorProvider(string databaseDirectoryPath, IServiceMediator serviceMediator, IBytesPool bytesPool)
    {
        _databaseDirectoryPath = databaseDirectoryPath;
        _serviceMediator = serviceMediator;
        _bytesPool = bytesPool;
    }

    private async ValueTask InitAsync(CancellationToken cancellationToken = default)
    {
        if (_serviceMediator is null) throw new NullReferenceException(nameof(_serviceMediator));

        var profilePublisherOptions = new ProfileUploaderOptions(Path.Combine(_databaseDirectoryPath, "profile_publisher"));
        _profilePublisher = await ProfileUploader.CreateAsync(_serviceMediator, SingleValueFileStorage.Factory, _bytesPool, profilePublisherOptions, cancellationToken);

        var profileSubscriberOptions = new ProfileDownloaderOptions(Path.Combine(_databaseDirectoryPath, "profile_subscriber"));
        _profileSubscriber = await ProfileDownloader.CreateAsync(_serviceMediator, SingleValueFileStorage.Factory, KeyValueLiteDatabaseStorage.Factory, _bytesPool, profileSubscriberOptions, cancellationToken);

        var barkPublisherOptions = new BarkUploaderOptions(Path.Combine(_databaseDirectoryPath, "bark_publisher"));
        _barkPublisher = await BarkUploader.CreateAsync(_serviceMediator, SingleValueFileStorage.Factory, _bytesPool, barkPublisherOptions, cancellationToken);

        var barkSubscriberOptions = new BarkDownloaderOptions(Path.Combine(_databaseDirectoryPath, "bark_subscriber"));
        _barkSubscriber = await BarkDownloader.CreateAsync(_serviceMediator, SingleValueFileStorage.Factory, KeyValueLiteDatabaseStorage.Factory, _bytesPool, barkSubscriberOptions, cancellationToken);
        _barkSubscriberOnGetTrustedSignaturesRegister = _barkSubscriber.OnGetTrustedSignatures.Listen(() => _profileSubscriber.GetSignaturesAsync());

        var fileUploaderOptions = new FileUploaderOptions(Path.Combine(_databaseDirectoryPath, "file_uploader"));
        _fileUploader = await FileUploader.CreateAsync(_serviceMediator, _bytesPool, fileUploaderOptions, cancellationToken);

        var fileDownloaderOptions = new FileDownloaderOptions(Path.Combine(_databaseDirectoryPath, "file_downloader"));
        _fileDownloader = await FileDownloader.CreateAsync(_serviceMediator, SingleValueFileStorage.Factory, _bytesPool, fileDownloaderOptions, cancellationToken);
    }

    protected override async ValueTask OnDisposeAsync()
    {
        if (_fileDownloader is not null) await _fileDownloader.DisposeAsync();
        _fileDownloader = null;

        if (_fileUploader is not null) await _fileUploader.DisposeAsync();
        _fileUploader = null;

        if (_barkSubscriber is not null) await _barkSubscriber.DisposeAsync();
        _barkSubscriber = null;

        if (_barkPublisher is not null) await _barkPublisher.DisposeAsync();
        _barkPublisher = null;

        if (_profileSubscriber is not null) await _profileSubscriber.DisposeAsync();
        _profileSubscriber = null;

        _barkSubscriberOnGetTrustedSignaturesRegister?.Dispose();
        _barkSubscriberOnGetTrustedSignaturesRegister = null;

        if (_profilePublisher is not null) await _profilePublisher.DisposeAsync();
        _profilePublisher = null;
    }

    public IProfilePublisher GetProfilePublisher() => _profilePublisher!;
    public IProfileSubscriber GetProfileSubscriber() => _profileSubscriber!;
    public IBarkUploader GetBarkPublisher() => _barkPublisher!;
    public IBarkDownloader GetBarkSubscriber() => _barkSubscriber!;
    public IFileUploader GetFileUploader() => _fileUploader!;
    public IFileDownloader GetFileDownloader() => _fileDownloader!;
}
