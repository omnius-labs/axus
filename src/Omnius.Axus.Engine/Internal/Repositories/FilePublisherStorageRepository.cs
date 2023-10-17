using System.Buffers;
using System.Runtime.CompilerServices;
using System.Text;
using Omnius.Axus.Core.Engine;
using Omnius.Axus.Core.Engine.Models;
using Omnius.Axus.Messages;
using Omnius.Core;
using Omnius.Core.Cryptography;
using Omnius.Core.Helpers;
using Omnius.Core.RocketPack;
using Omnius.Core.Serialization;
using Omnius.Core.Sql;
using SqlKata.Compilers;
using SqlKata.Execution;
using Omnius.Axus.Core.Engine.Repositories.Helpers;
using Omnius.Axus.Core.Engine.Repositories.Models;

namespace Omnius.Axus.Core.Implementations.Internal.Repositories;

internal sealed class FilePublisherStorageRepository : AsyncDisposableBase
{
    private readonly SQLiteConnectionBuilder _connectionBuilder;
    private readonly IBytesPool _bytesPool;

    private const ConvertBaseType _convertBaseType = ConvertBaseType.Base64;

    public FilePublisherStorageRepository(string dirPath, IBytesPool bytesPool)
    {
        DirectoryHelper.CreateDirectory(dirPath);

        _connectionBuilder = new SQLiteConnectionBuilder(Path.Combine(dirPath, "sqlite.db"));
        _bytesPool = bytesPool;

        this.FileItems = new FilePublishedItemRepository(_connectionBuilder, _bytesPool);
        this.BlockInternalItems = new BlockPublishedInternalItemRepository(_connectionBuilder, _bytesPool);
        this.BlockExternalItems = new BlockPublishedExternalItemRepository(_connectionBuilder, _bytesPool);
    }

    protected override async ValueTask OnDisposeAsync()
    {
    }

    public async ValueTask MigrateAsync(CancellationToken cancellationToken = default)
    {
        await this.FileItems.MigrateAsync(cancellationToken);
        await this.BlockInternalItems.MigrateAsync(cancellationToken);
        await this.BlockExternalItems.MigrateAsync(cancellationToken);
    }

    public FilePublishedItemRepository FileItems { get; }
    public BlockPublishedInternalItemRepository BlockInternalItems { get; }
    public BlockPublishedExternalItemRepository BlockExternalItems { get; }

    public sealed class FilePublishedItemRepository
    {
        private readonly SQLiteConnectionBuilder _connectionBuilder;
        private readonly IBytesPool _bytesPool;

        private readonly AsyncLock _asyncLock = new();

        public FilePublishedItemRepository(SQLiteConnectionBuilder connectionBuilder, IBytesPool bytesPool)
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
CREATE TABLE IF NOT EXISTS files (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    file_path TEXT,
    root_hash TEXT NOT NULL,
    max_block_size INTEGER NOT NULL,
    property TEXT,
    created_time INTEGER NOT NULL,
    updated_time INTEGER NOT NULL,
    UNIQUE (root_hash, file_path)
);
";
                await connection.ExecuteNonQueryAsync(query, cancellationToken);
            }
        }

        public async ValueTask<bool> ExistsAsync(OmniHash rootHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
SELECT COUNT(1)
    FROM files
    WHERE root_hash = @root_hash
    LIMIT 1;
";
                var parameters = new (string, object?)[]
                {
                    ("@root_hash", rootHash.ToString(_convertBaseType))
                };

                var result = await connection.ExecuteScalarAsync(query, parameters, cancellationToken);
                return (long)result! == 1;
            }
        }

        public async ValueTask<bool> ExistsAsync(string filePath, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
SELECT COUNT(1)
    FROM files
    WHERE file_path = @file_path
    LIMIT 1;
";
                var parameters = new (string, object?)[]
                {
                    ("@file_path", filePath)
                };

                var result = await connection.ExecuteScalarAsync(query, parameters, cancellationToken);
                return (long)result! == 1;
            }
        }

        public async IAsyncEnumerable<FilePublishedItem> GetItemsAsync([EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("files")
                        .Select("root_hash", "file_path", "max_block_size", "property", "created_time", "updated_time")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return new FilePublishedItem
                        {
                            RootHash = OmniHash.Parse((string)row.root_hash),
                            FilePath = (string)row.file_path,
                            MaxBlockSize = (int)row.max_block_size,
                            Property = row.property is null ? null : AttachedProperty.Create((string)row.property),
                            CreatedTime = Timestamp64.FromSeconds((long)row.created_time).ToDateTime(),
                            UpdatedTime = Timestamp64.FromSeconds((long)row.updated_time).ToDateTime(),
                        };
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask<FilePublishedItem?> GetItemAsync(string filePath, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                var compiler = new SqliteCompiler();
                using var db = new QueryFactory(connection, compiler);

                var rows = await db.Query("files")
                    .Select("root_hash", "file_path", "max_block_size", "property", "created_time", "updated_time")
                    .Where("file_path", "=", filePath)
                    .Limit(1)
                    .GetAsync(cancellationToken: cancellationToken);
                if (!rows.Any()) return null;

                var row = rows.First();

                return new FilePublishedItem
                {
                    RootHash = OmniHash.Parse((string)row.root_hash),
                    FilePath = (string)row.file_path,
                    MaxBlockSize = (int)row.max_block_size,
                    Property = row.property is null ? null : AttachedProperty.Create((string)row.property),
                    CreatedTime = Timestamp64.FromSeconds((long)row.created_time).ToDateTime(),
                    UpdatedTime = Timestamp64.FromSeconds((long)row.updated_time).ToDateTime(),
                };
            }
        }

        public async ValueTask<FilePublishedItem?> GetItemAsync(OmniHash rootHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                var compiler = new SqliteCompiler();
                using var db = new QueryFactory(connection, compiler);

                var rows = await db.Query("files")
                    .Select("root_hash", "file_path", "max_block_size", "property", "created_time", "updated_time")
                    .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                    .Limit(1)
                    .GetAsync(cancellationToken: cancellationToken);
                if (!rows.Any()) return null;

                var row = rows.First();

                return new FilePublishedItem
                {
                    RootHash = OmniHash.Parse((string)row.root_hash),
                    FilePath = (string)row.file_path,
                    MaxBlockSize = (int)row.max_block_size,
                    Property = row.property is null ? null : AttachedProperty.Create((string)row.property),
                    CreatedTime = Timestamp64.FromSeconds((long)row.created_time).ToDateTime(),
                    UpdatedTime = Timestamp64.FromSeconds((long)row.updated_time).ToDateTime(),
                };
            }
        }

        public async IAsyncEnumerable<OmniHash> GetRootHashesAsync([EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("files")
                        .Select("root_hash")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return OmniHash.Parse((string)row.root_hash);
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask UpsertAsync(FilePublishedItem item, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
INSERT INTO files (file_path, root_hash, max_block_size, property, created_time, updated_time)
    VALUES (@file_path, @root_hash, @max_block_size, @property, @created_time, @updated_time)
    ON CONFLICT (root_hash, file_path) DO UPDATE SET
        file_path = @file_path,
        root_hash = @root_hash,
        max_block_size = @max_block_size,
        property = @property,
        created_time = @created_time,
        updated_time = @updated_time
    WHERE file_path is not NULL;
";
                var parameters = new (string, object?)[]
                {
                    ("@file_path", item.FilePath),
                    ("@root_hash", item.RootHash.ToString(_convertBaseType)),
                    ("@max_block_size", item.MaxBlockSize),
                    ("@property", item.Property?.Value),
                    ("@created_time", Timestamp64.FromDateTime(item.CreatedTime).Seconds),
                    ("@updated_time", Timestamp64.FromDateTime(item.UpdatedTime).Seconds)
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }

        public async ValueTask DeleteAsync(OmniHash rootHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
DELETE
    FROM files
    WHERE root_hash = @root_hash;
";
                var parameters = new (string, object?)[]
                {
                    ($"@root_hash", rootHash.ToString(_convertBaseType)),
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }

        public async ValueTask DeleteAsync(string filePath, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
DELETE
    FROM files
    WHERE file_path = @file_path;
";
                var parameters = new (string, object?)[]
                {
                    ($"@file_path", filePath),
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }
    }

    public sealed class BlockPublishedInternalItemRepository
    {
        private readonly SQLiteConnectionBuilder _connectionBuilder;
        private readonly IBytesPool _bytesPool;

        private readonly AsyncLock _asyncLock = new();

        public BlockPublishedInternalItemRepository(SQLiteConnectionBuilder connectionBuilder, IBytesPool bytesPool)
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
CREATE TABLE IF NOT EXISTS internal_blocks (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    root_hash TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    depth INTEGER NOT NULL,
    `index` INTEGER NOT NULL,
    UNIQUE(root_hash, block_hash, depth, `index`)
);
CREATE INDEX IF NOT EXISTS index_root_hash_depth_index_for_internal_blocks ON internal_blocks (root_hash, depth ASC, `index` ASC);
";
                await connection.ExecuteNonQueryAsync(query, cancellationToken);
            }
        }

        public async ValueTask<bool> ExistsAsync(OmniHash rootHash, OmniHash blockHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
SELECT COUNT(1)
    FROM internal_blocks
    WHERE root_hash = @root_hash AND block_hash = @block_hash
    LIMIT 1;
";
                var parameters = new (string, object?)[]
                {
                    ($"@root_hash", rootHash.ToString(_convertBaseType)),
                    ($"@block_hash", blockHash.ToString(_convertBaseType)),
                };

                var result = await connection.ExecuteScalarAsync(query, parameters, cancellationToken);
                return (long)result! == 1;
            }
        }

        public async IAsyncEnumerable<BlockPublishedInternalItem> GetItemsAsync([EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("internal_blocks")
                        .Select("root_hash", "block_hash", "depth", "index")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return new BlockPublishedInternalItem
                        {
                            RootHash = OmniHash.Parse((string)row.root_hash),
                            BlockHash = OmniHash.Parse((string)row.block_hash),
                            Depth = (int)row.depth,
                            Index = (int)row.index,
                        };
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async IAsyncEnumerable<BlockPublishedInternalItem> GetItemsAsync(OmniHash rootHash, [EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("internal_blocks")
                        .Select("root_hash", "block_hash", "depth", "index")
                        .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                        .OrderBy("depth", "index")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return new BlockPublishedInternalItem
                        {
                            RootHash = OmniHash.Parse((string)row.root_hash),
                            BlockHash = OmniHash.Parse((string)row.block_hash),
                            Depth = (int)row.depth,
                            Index = (int)row.index,
                        };
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask<BlockPublishedInternalItem?> GetItemAsync(OmniHash rootHash, OmniHash blockHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                var compiler = new SqliteCompiler();
                using var db = new QueryFactory(connection, compiler);

                var rows = await db.Query("internal_blocks")
                    .Select("root_hash", "block_hash", "depth", "index")
                    .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                    .Where("block_hash", "=", blockHash.ToString(_convertBaseType))
                    .Limit(1)
                    .GetAsync(cancellationToken: cancellationToken);
                if (!rows.Any()) return null;

                var row = rows.First();

                return new BlockPublishedInternalItem
                {
                    RootHash = OmniHash.Parse((string)row.root_hash),
                    BlockHash = OmniHash.Parse((string)row.block_hash),
                    Depth = (int)row.depth,
                    Index = (int)row.index,
                };
            }
        }

        public async IAsyncEnumerable<OmniHash> GetBlockHashesAsync(OmniHash rootHash, [EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("internal_blocks")
                        .Select("block_hash")
                        .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                        .OrderBy("depth", "index")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return OmniHash.Parse((string)row.block_hash);
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask UpsertBulkAsync(IEnumerable<BlockPublishedInternalItem> items, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                using var transaction = await connection.BeginTransactionAsync(cancellationToken);

                foreach (var chunkedItems in items.Chunk(500))
                {
                    var queries = new StringBuilder();
                    var parameters = new List<(string, object?)>();

                    foreach (var (i, item) in chunkedItems.Select((n, i) => (i, n)))
                    {
                        var q =
$@"
INSERT INTO internal_blocks (root_hash, block_hash, depth, `index`)
    VALUES (@root_hash_{i}, @block_hash_{i}, @depth_{i}, @index_{i})
    ON CONFLICT (root_hash, block_hash, depth, `index`) DO NOTHING;
";
                        queries.Append(q);

                        var ps = new (string, object?)[]
                        {
                            ($"@root_hash_{i}", item.RootHash.ToString(_convertBaseType)),
                            ($"@block_hash_{i}", item.BlockHash.ToString(_convertBaseType)),
                            ($"@depth_{i}", item.Depth),
                            ($"@index_{i}", item.Index),
                        };
                        parameters.AddRange(ps);
                    }

                    await transaction.ExecuteNonQueryAsync(queries.ToString(), parameters, cancellationToken);
                }

                await transaction.CommitAsync(cancellationToken);
            }
        }

        public async ValueTask DeleteAsync(OmniHash rootHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
DELETE
    FROM internal_blocks
    WHERE root_hash = @root_hash;
";
                var parameters = new (string, object?)[]
                {
                    ($"@root_hash", rootHash.ToString(_convertBaseType)),
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }
    }

    public sealed class BlockPublishedExternalItemRepository
    {
        private readonly SQLiteConnectionBuilder _connectionBuilder;
        private readonly IBytesPool _bytesPool;

        private readonly AsyncLock _asyncLock = new();

        public BlockPublishedExternalItemRepository(SQLiteConnectionBuilder connectionBuilder, IBytesPool bytesPool)
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
CREATE TABLE IF NOT EXISTS external_blocks (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    root_hash TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    `index` INTEGER NOT NULL,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    UNIQUE (root_hash, block_hash, file_path, `index`, offset, length)
);
CREATE INDEX IF NOT EXISTS index_root_hash_depth_index_for_blocks ON external_blocks (root_hash, `index` ASC);
";
                await connection.ExecuteNonQueryAsync(query, cancellationToken);
            }
        }

        public async ValueTask<bool> ExistsAsync(OmniHash rootHash, OmniHash blockHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
SELECT COUNT(1)
    FROM external_blocks
    WHERE root_hash = @root_hash AND block_hash = @block_hash
    LIMIT 1;
";
                var parameters = new (string, object?)[]
                {
                    ($"@root_hash", rootHash.ToString(_convertBaseType)),
                    ($"@block_hash", blockHash.ToString(_convertBaseType)),
                };

                var result = await connection.ExecuteScalarAsync(query, parameters, cancellationToken);
                return (long)result! == 1;
            }
        }

        public async IAsyncEnumerable<BlockPublishedExternalItem> GetItemsAsync([EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("external_blocks")
                        .Select("file_path", "root_hash", "block_hash", "index", "offset", "length")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return new BlockPublishedExternalItem
                        {
                            FilePath = (string)row.file_path,
                            RootHash = OmniHash.Parse((string)row.root_hash),
                            BlockHash = OmniHash.Parse((string)row.block_hash),
                            Index = (int)row.index,
                            Offset = (long)row.offset,
                            Length = (int)row.length,
                        };
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async IAsyncEnumerable<BlockPublishedExternalItem> GetItemsAsync(OmniHash rootHash, [EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("external_blocks")
                        .Select("file_path", "root_hash", "block_hash", "index", "offset", "length")
                        .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                        .OrderBy("index")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return new BlockPublishedExternalItem
                        {
                            FilePath = (string)row.file_path,
                            RootHash = OmniHash.Parse((string)row.root_hash),
                            BlockHash = OmniHash.Parse((string)row.block_hash),
                            Index = (int)row.index,
                            Offset = (long)row.offset,
                            Length = (int)row.length,
                        };
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask<BlockPublishedExternalItem?> GetItemAsync(OmniHash rootHash, OmniHash blockHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                var compiler = new SqliteCompiler();
                using var db = new QueryFactory(connection, compiler);

                var rows = await db.Query("external_blocks")
                    .Select("file_path", "root_hash", "block_hash", "index", "offset", "length")
                    .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                    .Where("block_hash", "=", blockHash.ToString(_convertBaseType))
                    .Limit(1)
                    .GetAsync(cancellationToken: cancellationToken);
                if (!rows.Any()) return null;

                var row = rows.First();

                return new BlockPublishedExternalItem
                {
                    FilePath = (string)row.file_path,
                    RootHash = OmniHash.Parse((string)row.root_hash),
                    BlockHash = OmniHash.Parse((string)row.block_hash),
                    Index = (int)row.index,
                    Offset = (long)row.offset,
                    Length = (int)row.length,
                };
            }
        }

        public async IAsyncEnumerable<OmniHash> GetBlockHashesAsync(OmniHash rootHash, [EnumeratorCancellation] CancellationToken cancellationToken = default)
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
                    var rows = await db.Query("external_blocks")
                        .Select("block_hash")
                        .Where("root_hash", "=", rootHash.ToString(_convertBaseType))
                        .OrderBy("depth", "index")
                        .Offset(offset)
                        .Limit(limit)
                        .GetAsync(cancellationToken: cancellationToken);
                    if (!rows.Any()) yield break;

                    foreach (var row in rows)
                    {
                        yield return OmniHash.Parse((string)row.block_hash);
                    }

                    offset = limit;
                    limit += ChunkSize;
                }
            }
        }

        public async ValueTask UpsertBulkAsync(IEnumerable<BlockPublishedExternalItem> items, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);
                using var transaction = await connection.BeginTransactionAsync(cancellationToken);

                foreach (var chunkedItems in items.Chunk(500))
                {
                    var queries = new StringBuilder();
                    var parameters = new List<(string, object?)>();

                    foreach (var (i, item) in chunkedItems.Select((n, i) => (i, n)))
                    {
                        var q =
$@"
INSERT INTO external_blocks (file_path, root_hash, block_hash, `index`, offset, length)
    VALUES (@file_path_{i}, @root_hash_{i}, @block_hash_{i}, @index_{i}, @offset_{i}, @length_{i})
    ON CONFLICT (file_path, root_hash, block_hash, `index`, offset, length) DO NOTHING;
";
                        queries.Append(q);

                        var ps = new (string, object?)[]
                        {
                            ($"@file_path_{i}", item.FilePath),
                            ($"@root_hash_{i}", item.RootHash.ToString(_convertBaseType)),
                            ($"@block_hash_{i}", item.BlockHash.ToString(_convertBaseType)),
                            ($"@index_{i}", item.Index),
                            ($"@offset_{i}", item.Offset),
                            ($"@length_{i}", item.Length),
                        };
                        parameters.AddRange(ps);
                    }

                    await transaction.ExecuteNonQueryAsync(queries.ToString(), parameters, cancellationToken);
                }

                await transaction.CommitAsync(cancellationToken);
            }
        }

        public async ValueTask DeleteAsync(OmniHash rootHash, CancellationToken cancellationToken = default)
        {
            using (await _asyncLock.LockAsync(cancellationToken))
            {
                using var connection = await _connectionBuilder.CreateAsync(cancellationToken);

                var query =
$@"
DELETE
    FROM external_blocks
    WHERE root_hash = @root_hash;
";
                var parameters = new (string, object?)[]
                {
                    ($"@root_hash", rootHash.ToString(_convertBaseType)),
                };

                var result = await connection.ExecuteNonQueryAsync(query, parameters, cancellationToken);
            }
        }
    }
}