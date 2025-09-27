using System.Net;
using System.Net.Sockets;
using Omnius.Core.Base;
using Omnius.Core.Omnikit.Remoting;
using Omnius.Core.RocketPack;
using V1 = Omnius.Axus.Service.Interface.Features.V1;

namespace Omnius.Axus.Service.Interface;

public class AxusApiClient
{
    private readonly IPAddress _address;
    private readonly int _port;

    public AxusApiClient(IPAddress address, int port)
    {
        _address = address;
        _port = port;
    }

    private async ValueTask<OmniRemotingCaller<OmniRemotingDefaultErrorMessage>> ConnectAsync(uint functionId, CancellationToken cancellationToken = default)
    {
        var client = new TcpClient();
        await client.ConnectAsync(_address, _port, cancellationToken);
        var stream = client.GetStream();

        var remotingCaller = new OmniRemotingCaller<OmniRemotingDefaultErrorMessage>(stream, functionId, 1024 * 1024, BytesPool.Shared);
        await remotingCaller.HandshakeAsync(cancellationToken);
        return remotingCaller;
    }

    public async ValueTask<V1.HealthResponse> GetHealthAsync(CancellationToken cancellationToken = default)
    {
        await using var remotingCaller = await this.ConnectAsync(0, cancellationToken);
        var result = await remotingCaller.CallAsync<EmptyRocketMessage, V1.HealthResponse>(EmptyRocketMessage.Empty, cancellationToken);
        return result;
    }
}
