using OpenAirBookLink.Core.Localization;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Text;
using OpenAirBookLink.Core.Protocol;

namespace OpenAirBookLink.Core.Discovery;

// Co 2 s rozgłasza `announce` na UDP 47811, żeby telefon sam znalazł komputer.
public sealed class DiscoveryBroadcaster : IDisposable
{
    public static readonly TimeSpan Interval = TimeSpan.FromSeconds(2);

    private readonly Func<AnnounceMessage> _message;
    private readonly int _port;
    private readonly CancellationTokenSource _cts = new();

    public DiscoveryBroadcaster(Func<AnnounceMessage> message, int port = LinkProtocol.DiscoveryPort)
    {
        _message = message;
        _port = port;
    }

    public event Action<string>? Log;

    public void Start() => _ = LoopAsync(_cts.Token);

    private async Task LoopAsync(CancellationToken ct)
    {
        using var udp = new UdpClient(AddressFamily.InterNetwork) { EnableBroadcast = true };
        var warned = false;
        while (!ct.IsCancellationRequested)
        {
            var payload = Encoding.UTF8.GetBytes(LinkJson.Serialize(_message()));
            foreach (var target in BroadcastAddresses())
            {
                try
                {
                    await udp.SendAsync(payload, new IPEndPoint(target, _port), ct);
                }
                catch (SocketException e) when (!warned)
                {
                    warned = true;
                    Log?.Invoke(L.T("discovery.failed", e.Message));
                }
                catch (SocketException)
                {
                }
            }

            try { await Task.Delay(Interval, ct); }
            catch (OperationCanceledException) { return; }
        }
    }

    private static IEnumerable<IPAddress> BroadcastAddresses()
    {
        var result = new HashSet<IPAddress> { IPAddress.Broadcast };
        foreach (var (address, mask) in LocalIPv4())
        {
            var a = address.GetAddressBytes();
            var m = mask.GetAddressBytes();
            var b = new byte[4];
            for (var i = 0; i < 4; i++) b[i] = (byte)(a[i] | ~m[i]);
            result.Add(new IPAddress(b));
        }
        return result;
    }

    // Adresy IPv4 aktywnych kart sieciowych (bez loopbacka i APIPA), z maskami.
    public static IEnumerable<(IPAddress Address, IPAddress Mask)> LocalIPv4()
    {
        foreach (var nic in NetworkInterface.GetAllNetworkInterfaces())
        {
            if (nic.OperationalStatus != OperationalStatus.Up || nic.NetworkInterfaceType == NetworkInterfaceType.Loopback)
                continue;
            foreach (var ua in nic.GetIPProperties().UnicastAddresses)
            {
                if (ua.Address.AddressFamily != AddressFamily.InterNetwork) continue;
                var bytes = ua.Address.GetAddressBytes();
                if (bytes[0] == 169 && bytes[1] == 254) continue;
                yield return (ua.Address, ua.IPv4Mask);
            }
        }
    }

    public void Dispose() => _cts.Cancel();
}
