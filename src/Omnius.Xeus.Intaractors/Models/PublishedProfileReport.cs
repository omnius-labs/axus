using System;
using Omnius.Core.Cryptography;

namespace Omnius.Xeus.Intaractors.Models
{
    public record PublishedProfileReport
    {
        public PublishedProfileReport(DateTime creationTime, OmniSignature signature)
        {
            this.CreationTime = creationTime;
            this.Signature = signature;
        }

        public DateTime CreationTime { get; }

        public OmniSignature Signature { get; }
    }
}
