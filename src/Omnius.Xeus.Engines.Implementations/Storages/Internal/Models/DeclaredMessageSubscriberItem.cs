using System;
using Omnius.Core.Cryptography;

namespace Omnius.Xeus.Engines.Storages.Internal.Models
{
    internal sealed class DeclaredMessageSubscriberItem
    {
        public DeclaredMessageSubscriberItem(OmniSignature signature, string registrant)
        {
            this.Signature = signature;
            this.Registrant = registrant;
        }

        public OmniSignature Signature { get; }

        public string Registrant { get; }
    }
}