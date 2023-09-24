using System.Runtime.CompilerServices;
using Omnius.Axus.Engines.Internal.Models;
using Omnius.Axus.Messages;
using Omnius.Core;
using Omnius.Core.Cryptography;
using Omnius.Core.Helpers;
using Omnius.Core.RocketPack;
using Omnius.Core.Sql;
using SqlKata.Compilers;
using SqlKata.Execution;

namespace Omnius.Axus.Engines.Internal.Repositories;

internal sealed partial class ShoutPublisherStorageRepository : AsyncDisposableBase
{
    private readonly SQLiteConnectionBuilder _connectionBuilder;
    private readonly IBytesPool _bytesPool;

    public ShoutPublisherStorageRepository(string dirPath, IBytesPool bytesPool)
    {
        DirectoryHelper.CreateDirectory(dirPath);

        _connectionBuilder = new SQLiteConnectionBuilder(Path.Combine(dirPath, "sqlite.db"));
        _bytesPool = bytesPool;

        this.ShoutItems = new ShoutPublishedItemRepository(_connectionBuilder, _bytesPool);
    }

    protected override async ValueTask OnDisposeAsync()
    {
    }

    public async ValueTask MigrateAsync(CancellationToken cancellationToken = default)
    {
        await this.ShoutItems.MigrateAsync(cancellationToken);
    }

    public ShoutPublishedItemRepository ShoutItems { get; }

    public sealed class ShoutPublishedItemRepository
    {
        private readonly SQLiteConnectionBuilder _connectionBuilder;
        private readonly IBytesPool _bytesPool;

        private readonly AsyncLock _asyncLock = new();

        public ShoutPublishedItemRepository(SQLiteConnectionBuilder connectionBuilder, IBytesPool bytesPool)
        {
            _connectionBuilder = connectionBuilder;
            _bytesPool = bytesPool;
        }

        internal async ValueTask MigrateAsync(CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
@"
CREATE TABLE IF NOT EXISTS items (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    signature TEXT NOT NULL,
    channel TEXT NOT NULL,
    properties BLOB NOT NULL,
    created_time INTEGER NOT NULL,
    updated_time INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS files_root_hash_unique_index ON files(root_hash);
CREATE INDEX IF NOT EXISTS files_root_hash_index ON files(root_hash);
";
                await connection.ExecuteNonQueryAsync(query, cancellationToken);
            }
        }

        public async ValueTask<bool> ExistsAsync(OmniSignature signature, string channel, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
SELECT COUNT(1)
    FROM items
    WHERE signature = @signature AND channel = @channel
    LIMIT 1;
";
                var parameters = new (string, object?)[]
                {
                    ("@signature", signature.ToString()),
                    ("@channel", channel),
                };

                var result = await connection.ExecuteScalarAsync(query, parameters, cancellationToken);
                return (long)result! == 1;
            }
        }

        public async IAsyncEnumerable<ShoutPublishedItem> GetItemsAsync([EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                var compiler = new SqliteCompiler();
                using var db = new QueryFactory(connection, compiler);

                const int ChunkSize = 5000;
                int offset = 0;
                int limit = ChunkSize;

                for (; ; )
                {
                    var rows = await db.Query("items")
                        .Select("signature", "channel", "properties", "created_time", "updated_time")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return new ShoutPublishedItem
                        {
                            Signature = OmniSignature.Parse(row.signature),
                            Channel = row.channel,
                            Properties = RocketMessageConverter.FromBytes<RocketArray<AttachedProperty>>((byte[])row.properties).Values,
                            CreatedTime = Timestamp64.FromSeconds(row.created_time).ToDateTime(),
                            UpdatedTime = Timestamp64.FromSeconds(row.updated_time).ToDateTime(),
                        };
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask<ShoutPublishedItem?> GetItemAsync(OmniSignature signature, string channel, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                var compiler = new SqliteCompiler();
                using var db = new QueryFactory(connection, compiler);

                var rows = await db.Query("files")
                    .Select("signature", "channel", "properties", "created_time", "updated_time")
                    .Where("signature", signature.ToString())
                    .Where("channel", channel)
                    .Limit(1)
                    .GetAsync(cancellationToken: cancellationToken);
                if (!rows.Any()) return null;

                var row = rows.First();

                return new ShoutPublishedItem
                {
                    Signature = OmniSignature.Parse(row.signature),
                    Channel = row.channel,
                    Properties = RocketMessageConverter.FromBytes<RocketArray<AttachedProperty>>((byte[])row.properties).Values,
                    CreatedTime = Timestamp64.FromSeconds(row.created_time).ToDateTime(),
                    UpdatedTime = Timestamp64.FromSeconds(row.updated_time).ToDateTime(),
                };
            }
        }

        public async ValueTask UpsertAsync(ShoutPublishedItem item, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
INSERT INTO items (signature, channel, properties, created_time, updated_time)
    VALUES (@signature, @channel, @properties, @created_time, @updated_time)
    ON CONFLICT(signature, channel) DO UPDATE SET
        properties = @properties,
        created_time = @created_time,
        updated_time = @updated_time;
";
                var parameters = new (string, object?)[]
                {
                    ("@signature", item.Signature.ToString()),
                    ("@channel", item.Channel),
                    ("@properties", RocketMessageConverter.ToBytes(new RocketArray<AttachedProperty>(item.Properties.ToArray()))),
                    ("@created_time", item.CreatedTime),
                    ("@updated_time", item.UpdatedTime)
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }

        public async ValueTask DeleteAsync(OmniSignature signature, string channel, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
DELETE
    FROM items
    WHERE signature = @signature AND channel = @channel;
";
                var parameters = new (string, object?)[]
                {
                    ("@signature", signature.ToString()),
                    ("@channel", channel),
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }
    }
}