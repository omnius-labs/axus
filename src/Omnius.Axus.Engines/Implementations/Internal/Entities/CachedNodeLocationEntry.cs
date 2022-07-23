using Omnius.Axus.Engines.Internal.Models;
using Omnius.Axus.Models;

namespace Omnius.Axus.Engines.Internal.Entities;

internal record CachedNodeLocationEntity
{
    public NodeLocationEntity? Value { get; set; }

    public DateTime CreatedTime { get; set; }

    public DateTime LastConnectionTime { get; set; }

    public static CachedNodeLocationEntity Import(CachedNodeLocation item)
    {
        return new CachedNodeLocationEntity()
        {
            Value = NodeLocationEntity.Import(item.Value),
            CreatedTime = item.CreatedTime,
            LastConnectionTime = item.LastConnectionTime,
        };
    }

    public CachedNodeLocation Export()
    {
        return new CachedNodeLocation(this.Value?.Export() ?? NodeLocation.Empty, this.CreatedTime, this.LastConnectionTime);
    }
}