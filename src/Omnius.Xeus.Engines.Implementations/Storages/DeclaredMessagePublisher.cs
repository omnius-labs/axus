using System;
using System.Buffers;
using System.Collections.Generic;
using System.IO;
using System.IO.Pipelines;
using System.Threading;
using System.Threading.Tasks;
using Omnius.Core;
using Omnius.Core.Cryptography;
using Omnius.Core.Cryptography.Functions;
using Omnius.Core.Extensions;
using Omnius.Core.Io;
using Omnius.Core.RocketPack;
using Omnius.Core.Serialization;
using Omnius.Xeus.Engines.Models;
using Omnius.Xeus.Engines.Storages.Internal.Models;
using Omnius.Xeus.Engines.Storages.Internal.Repositories;

namespace Omnius.Xeus.Engines.Storages
{
    public sealed partial class DeclaredMessagePublisher : AsyncDisposableBase, IDeclaredMessagePublisher
    {
        private static readonly NLog.Logger _logger = NLog.LogManager.GetCurrentClassLogger();

        private readonly DeclaredMessagePublisherOptions _options;
        private readonly IBytesPool _bytesPool;

        private readonly DeclaredMessagePublisherRepository _repository;

        internal sealed class DeclaredMessagePublisherFactory : IDeclaredMessagePublisherFactory
        {
            public async ValueTask<IDeclaredMessagePublisher> CreateAsync(DeclaredMessagePublisherOptions options, IBytesPool bytesPool)
            {
                var result = new DeclaredMessagePublisher(options, bytesPool);
                await result.InitAsync();

                return result;
            }
        }

        public static IDeclaredMessagePublisherFactory Factory { get; } = new DeclaredMessagePublisherFactory();

        private DeclaredMessagePublisher(DeclaredMessagePublisherOptions options, IBytesPool bytesPool)
        {
            _options = options;
            _bytesPool = bytesPool;

            _repository = new DeclaredMessagePublisherRepository(Path.Combine(_options.ConfigDirectoryPath, "push-declared-message-storage.db"));
        }

        internal async ValueTask InitAsync()
        {
            await _repository.MigrateAsync();
        }

        protected override async ValueTask OnDisposeAsync()
        {
            _repository.Dispose();
        }

        public async ValueTask<DeclaredMessagePublisherReport> GetReportAsync(CancellationToken cancellationToken = default)
        {
            var pushDeclaredMessageReports = new List<PushDeclaredMessageReport>();

            foreach (var status in _repository.Items.FindAll())
            {
                pushDeclaredMessageReports.Add(new PushDeclaredMessageReport(status.Signature));
            }

            return new DeclaredMessagePublisherReport(pushDeclaredMessageReports.ToArray());
        }

        public async ValueTask CheckConsistencyAsync(Action<ConsistencyReport> callback, CancellationToken cancellationToken = default)
        {
        }

        public async ValueTask<IEnumerable<OmniSignature>> GetSignaturesAsync(CancellationToken cancellationToken = default)
        {
            var results = new List<OmniSignature>();

            foreach (var status in _repository.Items.FindAll())
            {
                results.Add(status.Signature);
            }

            return results;
        }

        public async ValueTask<bool> ContainsMessageAsync(OmniSignature signature, DateTime since = default)
        {
            var status = _repository.Items.Get(signature);
            if (status == null)
            {
                return false;
            }

            return true;
        }

        public async ValueTask PublishMessageAsync(DeclaredMessage message, CancellationToken cancellationToken = default)
        {
            if (message.Certificate is null)
            {
                throw new ArgumentNullException(nameof(message.Certificate));
            }

            _repository.Items.Add(new DeclaredMessagePublisherItem(message.Certificate.GetOmniSignature(), message.CreationTime.ToDateTime()));

            var filePath = this.ComputeCacheFilePath(message.Certificate.GetOmniSignature());

            using var fileStream = new UnbufferedFileStream(filePath, FileMode.Create, FileAccess.ReadWrite, FileShare.None, FileOptions.None, _bytesPool);
            var pipeWriter = PipeWriter.Create(fileStream);
            message.Export(pipeWriter, _bytesPool);
            await pipeWriter.CompleteAsync();
        }

        public async ValueTask UnpublishMessageAsync(OmniSignature signature, CancellationToken cancellationToken = default)
        {
            _repository.Items.Remove(signature);
        }

        public async ValueTask<DateTime?> ReadMessageCreationTimeAsync(OmniSignature signature, CancellationToken cancellationToken = default)
        {
            var status = _repository.Items.Get(signature);
            if (status == null)
            {
                return null;
            }

            return status.CreationTime;
        }

        public async ValueTask<DeclaredMessage?> ReadMessageAsync(OmniSignature signature, CancellationToken cancellationToken = default)
        {
            var status = _repository.Items.Get(signature);
            if (status == null)
            {
                return null;
            }

            string filePath = this.ComputeCacheFilePath(signature);

            using var fileStream = new UnbufferedFileStream(filePath, FileMode.Open, FileAccess.ReadWrite, FileShare.None, FileOptions.None, _bytesPool);
            var pipeReader = PipeReader.Create(fileStream);
            var readResult = await pipeReader.ReadAsync(cancellationToken);
            var message = DeclaredMessage.Import(readResult.Buffer, _bytesPool);
            await pipeReader.CompleteAsync();

            return message;
        }

        private string ComputeCacheFilePath(OmniSignature signature)
        {
            return Path.Combine(_options.ConfigDirectoryPath, "cache", StringConverter.SignatureToString(signature));
        }
    }
}