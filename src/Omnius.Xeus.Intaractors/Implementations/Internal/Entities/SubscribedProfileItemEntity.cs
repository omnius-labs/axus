using System;
using Omnius.Core.Cryptography;
using Omnius.Xeus.Intaractors.Internal.Models;

namespace Omnius.Xeus.Intaractors.Internal.Entities;

internal record SubscribedProfileItemEntity
{
    public OmniSignatureEntity? Signature { get; set; }

    public OmniHashEntity? RootHash { get; set; }

    public DateTime CreationTime { get; set; }

    public static SubscribedProfileItemEntity Import(DownloadingProfileItem value)
    {
        return new SubscribedProfileItemEntity()
        {
            Signature = OmniSignatureEntity.Import(value.Signature),
            RootHash = OmniHashEntity.Import(value.RootHash),
            CreationTime = value.CreationTime,
        };
    }

    public DownloadingProfileItem Export()
    {
        return new DownloadingProfileItem(this.Signature?.Export() ?? OmniSignature.Empty, this.RootHash?.Export() ?? OmniHash.Empty, this.CreationTime);
    }
}