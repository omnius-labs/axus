// <auto-generated/>
#nullable enable

namespace Omnius.Axus.Interactors.Internal.Models;

public sealed partial class CachedProfileContent : global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent>
{
    public static global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent> Formatter => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent>.Formatter;
    public static global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent Empty => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent>.Empty;

    static CachedProfileContent()
    {
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent>.Formatter = new ___CustomFormatter();
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent>.Empty = new global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent(global::Omnius.Core.Cryptography.OmniSignature.Empty, global::Omnius.Core.RocketPack.Timestamp64.Zero, global::Omnius.Axus.Interactors.Internal.Models.ProfileContent.Empty);
    }

    private readonly global::System.Lazy<int> ___hashCode;

    public CachedProfileContent(global::Omnius.Core.Cryptography.OmniSignature signature, global::Omnius.Core.RocketPack.Timestamp64 shoutUpdatedTime, global::Omnius.Axus.Interactors.Internal.Models.ProfileContent value)
    {
        if (signature is null) throw new global::System.ArgumentNullException("signature");
        if (value is null) throw new global::System.ArgumentNullException("value");

        this.Signature = signature;
        this.ShoutUpdatedTime = shoutUpdatedTime;
        this.Value = value;

        ___hashCode = new global::System.Lazy<int>(() =>
        {
            var ___h = new global::System.HashCode();
            if (signature != default) ___h.Add(signature.GetHashCode());
            if (shoutUpdatedTime != default) ___h.Add(shoutUpdatedTime.GetHashCode());
            if (value != default) ___h.Add(value.GetHashCode());
            return ___h.ToHashCode();
        });
    }

    public global::Omnius.Core.Cryptography.OmniSignature Signature { get; }
    public global::Omnius.Core.RocketPack.Timestamp64 ShoutUpdatedTime { get; }
    public global::Omnius.Axus.Interactors.Internal.Models.ProfileContent Value { get; }

    public static global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent Import(global::System.Buffers.ReadOnlySequence<byte> sequence, global::Omnius.Core.IBytesPool bytesPool)
    {
        var reader = new global::Omnius.Core.RocketPack.RocketMessageReader(sequence, bytesPool);
        return Formatter.Deserialize(ref reader, 0);
    }
    public void Export(global::System.Buffers.IBufferWriter<byte> bufferWriter, global::Omnius.Core.IBytesPool bytesPool)
    {
        var writer = new global::Omnius.Core.RocketPack.RocketMessageWriter(bufferWriter, bytesPool);
        Formatter.Serialize(ref writer, this, 0);
    }

    public static bool operator ==(global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent? left, global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent? right)
    {
        return (right is null) ? (left is null) : right.Equals(left);
    }
    public static bool operator !=(global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent? left, global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent? right)
    {
        return !(left == right);
    }
    public override bool Equals(object? other)
    {
        if (other is not global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent) return false;
        return this.Equals((global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent)other);
    }
    public bool Equals(global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent? target)
    {
        if (target is null) return false;
        if (object.ReferenceEquals(this, target)) return true;
        if (this.Signature != target.Signature) return false;
        if (this.ShoutUpdatedTime != target.ShoutUpdatedTime) return false;
        if (this.Value != target.Value) return false;

        return true;
    }
    public override int GetHashCode() => ___hashCode.Value;

    private sealed class ___CustomFormatter : global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent>
    {
        public void Serialize(ref global::Omnius.Core.RocketPack.RocketMessageWriter w, scoped in global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent value, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            if (value.Signature != global::Omnius.Core.Cryptography.OmniSignature.Empty)
            {
                w.Write((uint)1);
                global::Omnius.Core.Cryptography.OmniSignature.Formatter.Serialize(ref w, value.Signature, rank + 1);
            }
            if (value.ShoutUpdatedTime != global::Omnius.Core.RocketPack.Timestamp64.Zero)
            {
                w.Write((uint)2);
                w.Write(value.ShoutUpdatedTime);
            }
            if (value.Value != global::Omnius.Axus.Interactors.Internal.Models.ProfileContent.Empty)
            {
                w.Write((uint)3);
                global::Omnius.Axus.Interactors.Internal.Models.ProfileContent.Formatter.Serialize(ref w, value.Value, rank + 1);
            }
            w.Write((uint)0);
        }
        public global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent Deserialize(ref global::Omnius.Core.RocketPack.RocketMessageReader r, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            global::Omnius.Core.Cryptography.OmniSignature p_signature = global::Omnius.Core.Cryptography.OmniSignature.Empty;
            global::Omnius.Core.RocketPack.Timestamp64 p_shoutUpdatedTime = global::Omnius.Core.RocketPack.Timestamp64.Zero;
            global::Omnius.Axus.Interactors.Internal.Models.ProfileContent p_value = global::Omnius.Axus.Interactors.Internal.Models.ProfileContent.Empty;

            for (; ; )
            {
                uint id = r.GetUInt32();
                if (id == 0) break;
                switch (id)
                {
                    case 1:
                        {
                            p_signature = global::Omnius.Core.Cryptography.OmniSignature.Formatter.Deserialize(ref r, rank + 1);
                            break;
                        }
                    case 2:
                        {
                            p_shoutUpdatedTime = r.GetTimestamp64();
                            break;
                        }
                    case 3:
                        {
                            p_value = global::Omnius.Axus.Interactors.Internal.Models.ProfileContent.Formatter.Deserialize(ref r, rank + 1);
                            break;
                        }
                }
            }

            return new global::Omnius.Axus.Interactors.Internal.Models.CachedProfileContent(p_signature, p_shoutUpdatedTime, p_value);
        }
    }
}
public sealed partial class ProfileContent : global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent>
{
    public static global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent> Formatter => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent>.Formatter;
    public static global::Omnius.Axus.Interactors.Internal.Models.ProfileContent Empty => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent>.Empty;

    static ProfileContent()
    {
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent>.Formatter = new ___CustomFormatter();
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent>.Empty = new global::Omnius.Axus.Interactors.Internal.Models.ProfileContent(global::System.Array.Empty<global::Omnius.Core.Cryptography.OmniSignature>(), global::System.Array.Empty<global::Omnius.Core.Cryptography.OmniSignature>());
    }

    private readonly global::System.Lazy<int> ___hashCode;

    public static readonly int MaxTrustedSignaturesCount = 1024;
    public static readonly int MaxBlockedSignaturesCount = 1024;

    public ProfileContent(global::Omnius.Core.Cryptography.OmniSignature[] trustedSignatures, global::Omnius.Core.Cryptography.OmniSignature[] blockedSignatures)
    {
        if (trustedSignatures is null) throw new global::System.ArgumentNullException("trustedSignatures");
        if (trustedSignatures.Length > 1024) throw new global::System.ArgumentOutOfRangeException("trustedSignatures");
        foreach (var n in trustedSignatures)
        {
            if (n is null) throw new global::System.ArgumentNullException("n");
        }
        if (blockedSignatures is null) throw new global::System.ArgumentNullException("blockedSignatures");
        if (blockedSignatures.Length > 1024) throw new global::System.ArgumentOutOfRangeException("blockedSignatures");
        foreach (var n in blockedSignatures)
        {
            if (n is null) throw new global::System.ArgumentNullException("n");
        }

        this.TrustedSignatures = new global::Omnius.Core.Collections.ReadOnlyListSlim<global::Omnius.Core.Cryptography.OmniSignature>(trustedSignatures);
        this.BlockedSignatures = new global::Omnius.Core.Collections.ReadOnlyListSlim<global::Omnius.Core.Cryptography.OmniSignature>(blockedSignatures);

        ___hashCode = new global::System.Lazy<int>(() =>
        {
            var ___h = new global::System.HashCode();
            foreach (var n in trustedSignatures)
            {
                if (n != default) ___h.Add(n.GetHashCode());
            }
            foreach (var n in blockedSignatures)
            {
                if (n != default) ___h.Add(n.GetHashCode());
            }
            return ___h.ToHashCode();
        });
    }

    public global::Omnius.Core.Collections.ReadOnlyListSlim<global::Omnius.Core.Cryptography.OmniSignature> TrustedSignatures { get; }
    public global::Omnius.Core.Collections.ReadOnlyListSlim<global::Omnius.Core.Cryptography.OmniSignature> BlockedSignatures { get; }

    public static global::Omnius.Axus.Interactors.Internal.Models.ProfileContent Import(global::System.Buffers.ReadOnlySequence<byte> sequence, global::Omnius.Core.IBytesPool bytesPool)
    {
        var reader = new global::Omnius.Core.RocketPack.RocketMessageReader(sequence, bytesPool);
        return Formatter.Deserialize(ref reader, 0);
    }
    public void Export(global::System.Buffers.IBufferWriter<byte> bufferWriter, global::Omnius.Core.IBytesPool bytesPool)
    {
        var writer = new global::Omnius.Core.RocketPack.RocketMessageWriter(bufferWriter, bytesPool);
        Formatter.Serialize(ref writer, this, 0);
    }

    public static bool operator ==(global::Omnius.Axus.Interactors.Internal.Models.ProfileContent? left, global::Omnius.Axus.Interactors.Internal.Models.ProfileContent? right)
    {
        return (right is null) ? (left is null) : right.Equals(left);
    }
    public static bool operator !=(global::Omnius.Axus.Interactors.Internal.Models.ProfileContent? left, global::Omnius.Axus.Interactors.Internal.Models.ProfileContent? right)
    {
        return !(left == right);
    }
    public override bool Equals(object? other)
    {
        if (other is not global::Omnius.Axus.Interactors.Internal.Models.ProfileContent) return false;
        return this.Equals((global::Omnius.Axus.Interactors.Internal.Models.ProfileContent)other);
    }
    public bool Equals(global::Omnius.Axus.Interactors.Internal.Models.ProfileContent? target)
    {
        if (target is null) return false;
        if (object.ReferenceEquals(this, target)) return true;
        if (!global::Omnius.Core.Helpers.CollectionHelper.Equals(this.TrustedSignatures, target.TrustedSignatures)) return false;
        if (!global::Omnius.Core.Helpers.CollectionHelper.Equals(this.BlockedSignatures, target.BlockedSignatures)) return false;

        return true;
    }
    public override int GetHashCode() => ___hashCode.Value;

    private sealed class ___CustomFormatter : global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.ProfileContent>
    {
        public void Serialize(ref global::Omnius.Core.RocketPack.RocketMessageWriter w, scoped in global::Omnius.Axus.Interactors.Internal.Models.ProfileContent value, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            if (value.TrustedSignatures.Count != 0)
            {
                w.Write((uint)1);
                w.Write((uint)value.TrustedSignatures.Count);
                foreach (var n in value.TrustedSignatures)
                {
                    global::Omnius.Core.Cryptography.OmniSignature.Formatter.Serialize(ref w, n, rank + 1);
                }
            }
            if (value.BlockedSignatures.Count != 0)
            {
                w.Write((uint)2);
                w.Write((uint)value.BlockedSignatures.Count);
                foreach (var n in value.BlockedSignatures)
                {
                    global::Omnius.Core.Cryptography.OmniSignature.Formatter.Serialize(ref w, n, rank + 1);
                }
            }
            w.Write((uint)0);
        }
        public global::Omnius.Axus.Interactors.Internal.Models.ProfileContent Deserialize(ref global::Omnius.Core.RocketPack.RocketMessageReader r, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            global::Omnius.Core.Cryptography.OmniSignature[] p_trustedSignatures = global::System.Array.Empty<global::Omnius.Core.Cryptography.OmniSignature>();
            global::Omnius.Core.Cryptography.OmniSignature[] p_blockedSignatures = global::System.Array.Empty<global::Omnius.Core.Cryptography.OmniSignature>();

            for (; ; )
            {
                uint id = r.GetUInt32();
                if (id == 0) break;
                switch (id)
                {
                    case 1:
                        {
                            var length = r.GetUInt32();
                            p_trustedSignatures = new global::Omnius.Core.Cryptography.OmniSignature[length];
                            for (int i = 0; i < p_trustedSignatures.Length; i++)
                            {
                                p_trustedSignatures[i] = global::Omnius.Core.Cryptography.OmniSignature.Formatter.Deserialize(ref r, rank + 1);
                            }
                            break;
                        }
                    case 2:
                        {
                            var length = r.GetUInt32();
                            p_blockedSignatures = new global::Omnius.Core.Cryptography.OmniSignature[length];
                            for (int i = 0; i < p_blockedSignatures.Length; i++)
                            {
                                p_blockedSignatures[i] = global::Omnius.Core.Cryptography.OmniSignature.Formatter.Deserialize(ref r, rank + 1);
                            }
                            break;
                        }
                }
            }

            return new global::Omnius.Axus.Interactors.Internal.Models.ProfileContent(p_trustedSignatures, p_blockedSignatures);
        }
    }
}
public sealed partial class CachedBarkContent : global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent>
{
    public static global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent> Formatter => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent>.Formatter;
    public static global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent Empty => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent>.Empty;

    static CachedBarkContent()
    {
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent>.Formatter = new ___CustomFormatter();
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent>.Empty = new global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent(global::Omnius.Core.Cryptography.OmniSignature.Empty, global::Omnius.Core.RocketPack.Timestamp64.Zero, global::Omnius.Axus.Interactors.Internal.Models.BarkContent.Empty);
    }

    private readonly global::System.Lazy<int> ___hashCode;

    public CachedBarkContent(global::Omnius.Core.Cryptography.OmniSignature signature, global::Omnius.Core.RocketPack.Timestamp64 shoutUpdatedTime, global::Omnius.Axus.Interactors.Internal.Models.BarkContent value)
    {
        if (signature is null) throw new global::System.ArgumentNullException("signature");
        if (value is null) throw new global::System.ArgumentNullException("value");

        this.Signature = signature;
        this.ShoutUpdatedTime = shoutUpdatedTime;
        this.Value = value;

        ___hashCode = new global::System.Lazy<int>(() =>
        {
            var ___h = new global::System.HashCode();
            if (signature != default) ___h.Add(signature.GetHashCode());
            if (shoutUpdatedTime != default) ___h.Add(shoutUpdatedTime.GetHashCode());
            if (value != default) ___h.Add(value.GetHashCode());
            return ___h.ToHashCode();
        });
    }

    public global::Omnius.Core.Cryptography.OmniSignature Signature { get; }
    public global::Omnius.Core.RocketPack.Timestamp64 ShoutUpdatedTime { get; }
    public global::Omnius.Axus.Interactors.Internal.Models.BarkContent Value { get; }

    public static global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent Import(global::System.Buffers.ReadOnlySequence<byte> sequence, global::Omnius.Core.IBytesPool bytesPool)
    {
        var reader = new global::Omnius.Core.RocketPack.RocketMessageReader(sequence, bytesPool);
        return Formatter.Deserialize(ref reader, 0);
    }
    public void Export(global::System.Buffers.IBufferWriter<byte> bufferWriter, global::Omnius.Core.IBytesPool bytesPool)
    {
        var writer = new global::Omnius.Core.RocketPack.RocketMessageWriter(bufferWriter, bytesPool);
        Formatter.Serialize(ref writer, this, 0);
    }

    public static bool operator ==(global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent? left, global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent? right)
    {
        return (right is null) ? (left is null) : right.Equals(left);
    }
    public static bool operator !=(global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent? left, global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent? right)
    {
        return !(left == right);
    }
    public override bool Equals(object? other)
    {
        if (other is not global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent) return false;
        return this.Equals((global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent)other);
    }
    public bool Equals(global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent? target)
    {
        if (target is null) return false;
        if (object.ReferenceEquals(this, target)) return true;
        if (this.Signature != target.Signature) return false;
        if (this.ShoutUpdatedTime != target.ShoutUpdatedTime) return false;
        if (this.Value != target.Value) return false;

        return true;
    }
    public override int GetHashCode() => ___hashCode.Value;

    private sealed class ___CustomFormatter : global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent>
    {
        public void Serialize(ref global::Omnius.Core.RocketPack.RocketMessageWriter w, scoped in global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent value, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            if (value.Signature != global::Omnius.Core.Cryptography.OmniSignature.Empty)
            {
                w.Write((uint)1);
                global::Omnius.Core.Cryptography.OmniSignature.Formatter.Serialize(ref w, value.Signature, rank + 1);
            }
            if (value.ShoutUpdatedTime != global::Omnius.Core.RocketPack.Timestamp64.Zero)
            {
                w.Write((uint)2);
                w.Write(value.ShoutUpdatedTime);
            }
            if (value.Value != global::Omnius.Axus.Interactors.Internal.Models.BarkContent.Empty)
            {
                w.Write((uint)3);
                global::Omnius.Axus.Interactors.Internal.Models.BarkContent.Formatter.Serialize(ref w, value.Value, rank + 1);
            }
            w.Write((uint)0);
        }
        public global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent Deserialize(ref global::Omnius.Core.RocketPack.RocketMessageReader r, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            global::Omnius.Core.Cryptography.OmniSignature p_signature = global::Omnius.Core.Cryptography.OmniSignature.Empty;
            global::Omnius.Core.RocketPack.Timestamp64 p_shoutUpdatedTime = global::Omnius.Core.RocketPack.Timestamp64.Zero;
            global::Omnius.Axus.Interactors.Internal.Models.BarkContent p_value = global::Omnius.Axus.Interactors.Internal.Models.BarkContent.Empty;

            for (; ; )
            {
                uint id = r.GetUInt32();
                if (id == 0) break;
                switch (id)
                {
                    case 1:
                        {
                            p_signature = global::Omnius.Core.Cryptography.OmniSignature.Formatter.Deserialize(ref r, rank + 1);
                            break;
                        }
                    case 2:
                        {
                            p_shoutUpdatedTime = r.GetTimestamp64();
                            break;
                        }
                    case 3:
                        {
                            p_value = global::Omnius.Axus.Interactors.Internal.Models.BarkContent.Formatter.Deserialize(ref r, rank + 1);
                            break;
                        }
                }
            }

            return new global::Omnius.Axus.Interactors.Internal.Models.CachedBarkContent(p_signature, p_shoutUpdatedTime, p_value);
        }
    }
}
public sealed partial class BarkContent : global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.BarkContent>
{
    public static global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.BarkContent> Formatter => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.BarkContent>.Formatter;
    public static global::Omnius.Axus.Interactors.Internal.Models.BarkContent Empty => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.BarkContent>.Empty;

    static BarkContent()
    {
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.BarkContent>.Formatter = new ___CustomFormatter();
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.BarkContent>.Empty = new global::Omnius.Axus.Interactors.Internal.Models.BarkContent(global::System.Array.Empty<global::Omnius.Axus.Interactors.Models.BarkMessage>());
    }

    private readonly global::System.Lazy<int> ___hashCode;

    public static readonly int MaxMessagesCount = 8192;

    public BarkContent(global::Omnius.Axus.Interactors.Models.BarkMessage[] messages)
    {
        if (messages is null) throw new global::System.ArgumentNullException("messages");
        if (messages.Length > 8192) throw new global::System.ArgumentOutOfRangeException("messages");
        foreach (var n in messages)
        {
            if (n is null) throw new global::System.ArgumentNullException("n");
        }

        this.Messages = new global::Omnius.Core.Collections.ReadOnlyListSlim<global::Omnius.Axus.Interactors.Models.BarkMessage>(messages);

        ___hashCode = new global::System.Lazy<int>(() =>
        {
            var ___h = new global::System.HashCode();
            foreach (var n in messages)
            {
                if (n != default) ___h.Add(n.GetHashCode());
            }
            return ___h.ToHashCode();
        });
    }

    public global::Omnius.Core.Collections.ReadOnlyListSlim<global::Omnius.Axus.Interactors.Models.BarkMessage> Messages { get; }

    public static global::Omnius.Axus.Interactors.Internal.Models.BarkContent Import(global::System.Buffers.ReadOnlySequence<byte> sequence, global::Omnius.Core.IBytesPool bytesPool)
    {
        var reader = new global::Omnius.Core.RocketPack.RocketMessageReader(sequence, bytesPool);
        return Formatter.Deserialize(ref reader, 0);
    }
    public void Export(global::System.Buffers.IBufferWriter<byte> bufferWriter, global::Omnius.Core.IBytesPool bytesPool)
    {
        var writer = new global::Omnius.Core.RocketPack.RocketMessageWriter(bufferWriter, bytesPool);
        Formatter.Serialize(ref writer, this, 0);
    }

    public static bool operator ==(global::Omnius.Axus.Interactors.Internal.Models.BarkContent? left, global::Omnius.Axus.Interactors.Internal.Models.BarkContent? right)
    {
        return (right is null) ? (left is null) : right.Equals(left);
    }
    public static bool operator !=(global::Omnius.Axus.Interactors.Internal.Models.BarkContent? left, global::Omnius.Axus.Interactors.Internal.Models.BarkContent? right)
    {
        return !(left == right);
    }
    public override bool Equals(object? other)
    {
        if (other is not global::Omnius.Axus.Interactors.Internal.Models.BarkContent) return false;
        return this.Equals((global::Omnius.Axus.Interactors.Internal.Models.BarkContent)other);
    }
    public bool Equals(global::Omnius.Axus.Interactors.Internal.Models.BarkContent? target)
    {
        if (target is null) return false;
        if (object.ReferenceEquals(this, target)) return true;
        if (!global::Omnius.Core.Helpers.CollectionHelper.Equals(this.Messages, target.Messages)) return false;

        return true;
    }
    public override int GetHashCode() => ___hashCode.Value;

    private sealed class ___CustomFormatter : global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.BarkContent>
    {
        public void Serialize(ref global::Omnius.Core.RocketPack.RocketMessageWriter w, scoped in global::Omnius.Axus.Interactors.Internal.Models.BarkContent value, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            if (value.Messages.Count != 0)
            {
                w.Write((uint)1);
                w.Write((uint)value.Messages.Count);
                foreach (var n in value.Messages)
                {
                    global::Omnius.Axus.Interactors.Models.BarkMessage.Formatter.Serialize(ref w, n, rank + 1);
                }
            }
            w.Write((uint)0);
        }
        public global::Omnius.Axus.Interactors.Internal.Models.BarkContent Deserialize(ref global::Omnius.Core.RocketPack.RocketMessageReader r, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            global::Omnius.Axus.Interactors.Models.BarkMessage[] p_messages = global::System.Array.Empty<global::Omnius.Axus.Interactors.Models.BarkMessage>();

            for (; ; )
            {
                uint id = r.GetUInt32();
                if (id == 0) break;
                switch (id)
                {
                    case 1:
                        {
                            var length = r.GetUInt32();
                            p_messages = new global::Omnius.Axus.Interactors.Models.BarkMessage[length];
                            for (int i = 0; i < p_messages.Length; i++)
                            {
                                p_messages[i] = global::Omnius.Axus.Interactors.Models.BarkMessage.Formatter.Deserialize(ref r, rank + 1);
                            }
                            break;
                        }
                }
            }

            return new global::Omnius.Axus.Interactors.Internal.Models.BarkContent(p_messages);
        }
    }
}
public sealed partial class CachedBarkMessage : global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage>
{
    public static global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage> Formatter => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage>.Formatter;
    public static global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage Empty => global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage>.Empty;

    static CachedBarkMessage()
    {
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage>.Formatter = new ___CustomFormatter();
        global::Omnius.Core.RocketPack.IRocketMessage<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage>.Empty = new global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage(global::Omnius.Core.Cryptography.OmniSignature.Empty, global::Omnius.Axus.Interactors.Models.BarkMessage.Empty);
    }

    private readonly global::System.Lazy<int> ___hashCode;

    public CachedBarkMessage(global::Omnius.Core.Cryptography.OmniSignature signature, global::Omnius.Axus.Interactors.Models.BarkMessage value)
    {
        if (signature is null) throw new global::System.ArgumentNullException("signature");
        if (value is null) throw new global::System.ArgumentNullException("value");

        this.Signature = signature;
        this.Value = value;

        ___hashCode = new global::System.Lazy<int>(() =>
        {
            var ___h = new global::System.HashCode();
            if (signature != default) ___h.Add(signature.GetHashCode());
            if (value != default) ___h.Add(value.GetHashCode());
            return ___h.ToHashCode();
        });
    }

    public global::Omnius.Core.Cryptography.OmniSignature Signature { get; }
    public global::Omnius.Axus.Interactors.Models.BarkMessage Value { get; }

    public static global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage Import(global::System.Buffers.ReadOnlySequence<byte> sequence, global::Omnius.Core.IBytesPool bytesPool)
    {
        var reader = new global::Omnius.Core.RocketPack.RocketMessageReader(sequence, bytesPool);
        return Formatter.Deserialize(ref reader, 0);
    }
    public void Export(global::System.Buffers.IBufferWriter<byte> bufferWriter, global::Omnius.Core.IBytesPool bytesPool)
    {
        var writer = new global::Omnius.Core.RocketPack.RocketMessageWriter(bufferWriter, bytesPool);
        Formatter.Serialize(ref writer, this, 0);
    }

    public static bool operator ==(global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage? left, global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage? right)
    {
        return (right is null) ? (left is null) : right.Equals(left);
    }
    public static bool operator !=(global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage? left, global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage? right)
    {
        return !(left == right);
    }
    public override bool Equals(object? other)
    {
        if (other is not global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage) return false;
        return this.Equals((global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage)other);
    }
    public bool Equals(global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage? target)
    {
        if (target is null) return false;
        if (object.ReferenceEquals(this, target)) return true;
        if (this.Signature != target.Signature) return false;
        if (this.Value != target.Value) return false;

        return true;
    }
    public override int GetHashCode() => ___hashCode.Value;

    private sealed class ___CustomFormatter : global::Omnius.Core.RocketPack.IRocketMessageFormatter<global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage>
    {
        public void Serialize(ref global::Omnius.Core.RocketPack.RocketMessageWriter w, scoped in global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage value, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            if (value.Signature != global::Omnius.Core.Cryptography.OmniSignature.Empty)
            {
                w.Write((uint)1);
                global::Omnius.Core.Cryptography.OmniSignature.Formatter.Serialize(ref w, value.Signature, rank + 1);
            }
            if (value.Value != global::Omnius.Axus.Interactors.Models.BarkMessage.Empty)
            {
                w.Write((uint)2);
                global::Omnius.Axus.Interactors.Models.BarkMessage.Formatter.Serialize(ref w, value.Value, rank + 1);
            }
            w.Write((uint)0);
        }
        public global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage Deserialize(ref global::Omnius.Core.RocketPack.RocketMessageReader r, scoped in int rank)
        {
            if (rank > 256) throw new global::System.FormatException();

            global::Omnius.Core.Cryptography.OmniSignature p_signature = global::Omnius.Core.Cryptography.OmniSignature.Empty;
            global::Omnius.Axus.Interactors.Models.BarkMessage p_value = global::Omnius.Axus.Interactors.Models.BarkMessage.Empty;

            for (; ; )
            {
                uint id = r.GetUInt32();
                if (id == 0) break;
                switch (id)
                {
                    case 1:
                        {
                            p_signature = global::Omnius.Core.Cryptography.OmniSignature.Formatter.Deserialize(ref r, rank + 1);
                            break;
                        }
                    case 2:
                        {
                            p_value = global::Omnius.Axus.Interactors.Models.BarkMessage.Formatter.Deserialize(ref r, rank + 1);
                            break;
                        }
                }
            }

            return new global::Omnius.Axus.Interactors.Internal.Models.CachedBarkMessage(p_signature, p_value);
        }
    }
}
