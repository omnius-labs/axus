using Omnius.Axis.Models;
using Omnius.Core.Net;

namespace Omnius.Axis.Engines.Internal.Entities;

internal record NodeLocationEntity
{
    public string[]? Addresses { get; set; }

    public static NodeLocationEntity Import(NodeLocation value)
    {
        return new NodeLocationEntity()
        {
            Addresses = value.Addresses.Select(n => n.ToString()).ToArray()
        };
    }

    public NodeLocation Export()
    {
        return new NodeLocation(this.Addresses?.Select(n => new OmniAddress(n))?.ToArray() ?? Array.Empty<OmniAddress>());
    }
}