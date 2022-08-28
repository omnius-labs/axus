using Omnius.Core.Cryptography;

namespace Omnius.Axus.Interactors.Internal.Models;

internal record SubscribedBarkItem
{
    public SubscribedBarkItem(OmniSignature signature, OmniHash rootHash, DateTime createdTime)
    {
        this.Signature = signature;
        this.RootHash = rootHash;
        this.CreatedTime = createdTime;
    }

    public OmniSignature Signature { get; }

    public OmniHash RootHash { get; }

    public DateTime CreatedTime { get; }
}
