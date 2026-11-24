# Spike S3-0: Do the Linux/macOS tweaks help Tibia?

**Status:** Waiting on measurements. This needs real Linux and macOS
machines; the development PC is Windows-only and has no WSL.

## Hypotheses

| OS | Tweak | Expected effect | Risk |
|----|-------|-----------------|------|
| Linux | `net.ipv4.tcp_moderate_rcvbuf=0` | No receive-buffer resizing, so less jitter on bursty game traffic | The buffer stays at `tcp_rmem` default (~128 KiB). That is fine for Tibia's low bandwidth but can slow big downloads on high-latency links. |
| Linux | `net.ipv4.tcp_autocorking=0` | Small writes go out immediately instead of being corked behind in-flight data | Slightly more packets and CPU, which is negligible for a game client. |
| Linux | `quickack 1` on default routes | ACKs go out immediately instead of after up to 40 ms, so the server's send window advances sooner | More ACK packets. The setting is lost when DHCP recreates the route, so a NetworkManager hook re-applies it. |
| macOS | `net.inet.tcp.doautorcvbuf=0` | Same as `tcp_moderate_rcvbuf` | Same |
| macOS | `net.inet.tcp.delayed_ack=0` | ACKs go out immediately (the default `3` delays them) | More ACK packets |

Neither OS has a system-wide equivalent of Windows `TCPNoDelay`. Nagle is
controlled per socket by the application, so it is out of scope unless the
Tibia client (or Wine) is shown to leave it on.

## Method

For each OS, run the native Tibia client, or Wine/CrossOver where there is no
native client, against Secura (Frankfurt) from the same network:

1. **Baseline:** run `tibia-net-optimizer revert`, then reboot.
2. Run `mtr -rwzc 300 <game server IP>` (or `ping -c 300`) during 5 minutes
   of normal play. Record avg, stddev (jitter) and loss.
3. Capture `tcpdump -i any host <game server IP> -w base.pcap` and measure the
   delay between each keypress packet and the server's ACK, and how much
   ACKs are delayed.
4. **Tuned:** run `sudo tibia-net-optimizer apply` and repeat steps 2–3.
5. Repeat each tweak on its own to see which ones matter.

## Results

| OS / client | Setting | Avg RTT | Jitter | Loss | ACK delay p95 | Feel |
|-------------|---------|--------:|-------:|-----:|--------------:|------|
| _tbd_ | baseline | | | | | |
| _tbd_ | all tweaks | | | | | |

## Decision

_To fill in: keep, or drop, each tweak based on the results above._
