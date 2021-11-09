using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using LiteDB;
using Nito.AsyncEx;
using Omnius.Core;
using Omnius.Core.Helpers;
using Omnius.Xeus.Intaractors.Internal.Entities;
using Omnius.Xeus.Intaractors.Internal.Models;
using Omnius.Xeus.Utils;

namespace Omnius.Xeus.Intaractors.Internal.Repositories;

internal sealed class FileUploaderRepository : DisposableBase
{
    private readonly LiteDatabase _database;

    public FileUploaderRepository(string dirPath)
    {
        DirectoryHelper.CreateDirectory(dirPath);

        _database = new LiteDatabase(Path.Combine(dirPath, "lite.db"));
        _database.UtcDate = true;

        this.Items = new UploadingFileItemRepository(_database);
    }

    protected override void OnDispose(bool disposing)
    {
        _database.Dispose();
    }

    public async ValueTask MigrateAsync(CancellationToken cancellationToken = default)
    {
        await this.Items.MigrateAsync(cancellationToken);
    }

    public UploadingFileItemRepository Items { get; }

    public sealed class UploadingFileItemRepository
    {
        private const string CollectionName = "uploading_file_items";

        private readonly LiteDatabase _database;

        private readonly AsyncReaderWriterLock _asyncLock = new();

        public UploadingFileItemRepository(LiteDatabase database)
        {
            _database = database;
        }

        internal async ValueTask MigrateAsync(CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.WriterLockAsync(cancellationToken))
            {
                if (_database.GetDocumentVersion(CollectionName) <= 0)
                {
                    var col = this.GetCollection();
                    col.EnsureIndex(x => x.FilePath, true);
                }

                _database.SetDocumentVersion(CollectionName, 1);
            }
        }

        private ILiteCollection<UploadingFileItemEntity> GetCollection()
        {
            var col = _database.GetCollection<UploadingFileItemEntity>(CollectionName);
            return col;
        }

        public bool Exists(string filePath)
        {
            using (_asyncLock.ReaderLock())
            {
                var col = this.GetCollection();
                return col.Exists(n => n.FilePath == filePath);
            }
        }

        public IEnumerable<UploadingFileItem> FindAll()
        {
            using (_asyncLock.ReaderLock())
            {
                var col = this.GetCollection();
                return col.FindAll().Select(n => n.Export()).ToArray();
            }
        }

        public UploadingFileItem? FindOne(string filePath)
        {
            using (_asyncLock.ReaderLock())
            {
                var col = this.GetCollection();
                return col.FindById(filePath).Export();
            }
        }

        public void Upsert(UploadingFileItem item)
        {
            using (_asyncLock.WriterLock())
            {
                var itemEntity = UploadingFileItemEntity.Import(item);

                var col = this.GetCollection();

                _database.BeginTrans();

                col.DeleteMany(n => n.FilePath == item.FilePath);
                col.Insert(itemEntity);

                _database.Commit();
            }
        }

        public void Delete(string filePath)
        {
            using (_asyncLock.WriterLock())
            {
                var col = this.GetCollection();
                col.DeleteMany(n => n.FilePath == filePath);
            }
        }
    }
}