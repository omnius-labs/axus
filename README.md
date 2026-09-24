<p align="center">
<img width="128" src="https://github.com/omnius-labs/axus/blob/main/docs/logo.png?raw=true" alt="Axus logo">
</p>

<h1 align="center">Axus - Omnius File Exchanger (Work in Progress)</h1>

[![test-daemon-all](https://github.com/omnius-labs/axus/actions/workflows/test-daemon-all.yml/badge.svg)](https://github.com/omnius-labs/axus/actions/workflows/test-daemon-all.yml)

Axus is a peer-to-peer (P2P) file-sharing service.

## Features

- **File Upload and Download**: Users can easily upload and download files, making data sharing effortless.
- **File Search and Publishing**: Files can be searched and published with protection provided by a Web of Trust system, ensuring security and integrity.
- **Bulletin Board Functionality**: Provides a platform within the network for users to safely exchange and share information.

## Development

### Repository Setup

Please initialize the submodules right after cloning.

```sh
git submodule update --init
```

### Installing Required Dependencies

#### Debian and Ubuntu

```sh
sudo apt-get install -y libclang-dev
```
This package is required for RocksDB.

### Daemon Configuration

The daemon selects its configuration directory from `AXUS_DAEMON_CONFIG_DIR`, `~/.config/axus`, then `.config/axus`.
The selected directory must contain [axus.toml](./daemon/config/axus.toml).

Run the repository's development configuration with:

```sh
cargo run --manifest-path daemon/Cargo.toml -p omnius-axus-daemon -- start --config-dir daemon/config
```

The `[p2p]` section accepts the following keys.

| Key | Default | Meaning |
| --- | --- | --- |
| `listen_addr` | required | `host:port` that accepts P2P connections |
| `advertise_addrs` | `[]` | `host:port` list announced to other nodes. When empty, the daemon announces `listen_addr`, or the reachable IPs with its port when `listen_addr` is unspecified such as `0.0.0.0` |
| `use_upnp` | `false` | Opens the listen port on the router through UPnP when `listen_addr` is unspecified |
| `bootstrap_nodes` | `[]` | `axus:node/...` URIs of nodes to connect to first |

At startup the daemon logs its own URI as `node_profile` in the `node profile` message.
Add that URI to `bootstrap_nodes` of another daemon to connect the two.

## Links

- Official Documentation: https://docs.omnius-labs.com/

## License

This project is released under the MIT License. For more details, please refer to the [LICENSE](LICENSE.txt) file.

## Contribution

If you would like to contribute to this project, please contact us through [Issues](https://github.com/omnius-labs/axus/issues) or [Pull Requests](https://github.com/omnius-labs/axus/pulls) on GitHub.
