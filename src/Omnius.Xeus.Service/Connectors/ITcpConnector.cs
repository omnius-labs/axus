using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Omnius.Core;
using Omnius.Core.Network;
using Omnius.Core.Network.Caps;
using Omnius.Core.Network.Connections;
using Omnius.Core.Network.Proxies;
using Omnius.Core.Network.Upnp;

namespace Omnius.Xeus.Service.Connectors
{
    public interface ITcpConnectorFactory
    {
        public ValueTask<ITcpConnector> CreateAsync(TcpConnectorOptions options, ISocks5ProxyClientFactory socks5ProxyClientFactory, IHttpProxyClientFactory httpProxyClientFactory, IUpnpClientFactory upnpClientFactory, IBytesPool bytesPool);
    }

    public interface ITcpConnector : IConnector
    {
    }
}
