using Omnius.Core.RocketPack;

namespace Omnius.Axus.Service.Interface.Features.V1;

public class HealthResponse : RocketMessage<HealthResponse>
{
    public required string GitTag { get; init; }

    private int? _hashCode;

    public override int GetHashCode()
    {
        if (_hashCode is null)
        {
            var h = new HashCode();
            h.Add(this.GitTag);
            _hashCode = h.ToHashCode();
        }

        return _hashCode.Value;
    }

    public override bool Equals(HealthResponse? other)
    {
        if (other is null) return false;
        if (ReferenceEquals(this, other)) return true;
        return this.GitTag == other.GitTag;
    }

    static HealthResponse()
    {
        Formatter = new CustomSerializer();
        Empty = new HealthResponse() { GitTag = string.Empty };
    }

    private sealed class CustomSerializer : IRocketMessageSerializer<HealthResponse>
    {
        public void Serialize(ref RocketMessageWriter w, scoped in HealthResponse value, scoped in int depth)
        {
            w.Put(value.GitTag);
        }
        public HealthResponse Deserialize(ref RocketMessageReader r, scoped in int depth)
        {
            var gitTag = r.GetString(1024);

            return new HealthResponse()
            {
                GitTag = gitTag,
            };
        }
    }
}
