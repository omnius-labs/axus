namespace Omnius.Axus.Interactors;

// public class CachedBarkMessageRepositoryTest
// {
//     [Fact]
//     public async Task InsertBulkAndFetchByTagTest()
//     {
//         var bytesPool = BytesPool.Shared;
//         var tempDirDeleter = FixtureFactory.GenTempDirectory(out var tempDir);
//         var repo = new CachedBarkMessageRepository(tempDir, bytesPool);

//         await repo.MigrateAsync();

//         var insertMessages = new[] {
//             GenRandomCachedBarkMessage(),
//             GenRandomCachedBarkMessage(),
//             GenRandomCachedBarkMessage()
//         };
//         repo.InsertBulk(insertMessages);

//         var fetchedMessages = repo.FetchMessageByTag("tag");
//         insertMessages.Should().BeEquivalentTo(fetchedMessages);
//     }

//     private static CachedBarkContent GenRandomCachedBarkMessage()
//     {
//         return new CachedBarkContent(
//             OmniDigitalSignature.Create("aaa", OmniDigitalSignatureAlgorithmType.EcDsa_P521_Sha2_256).GetOmniSignature(),
//             new BarkMessage(
//                 "tag",
//                 Timestamp64.FromDateTime(DateTime.UtcNow),
//                 "comment",
//                 new OmniHash(OmniHashAlgorithmType.Sha2_256, FixtureFactory.GetRandomBytes(32))
//             )
//         );
//     }
// }
