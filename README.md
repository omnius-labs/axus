<p align="center">
<img width="128" src="https://github.com/omnius-labs/axus/blob/main/docs/logo.png?raw=true" alt="Axus logo">
</p>

<h1 align="center">Axus - Omnius File Exchanger (Work in Progress)</h1>

[![test-daemon-all](https://github.com/omnius-labs/axus/actions/workflows/test-daemon-all.yml/badge.svg)](https://github.com/omnius-labs/axus/actions/workflows/test-daemon-all.yml)
[![test-ui-desktop-all](https://github.com/omnius-labs/axus/actions/workflows/test-ui-desktop-all.yml/badge.svg)](https://github.com/omnius-labs/axus/actions/workflows/test-ui-desktop-all.yml)

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

## Links

- Official Documentation: https://docs.omnius-labs.com/

## License

This project is released under the MIT License. For more details, please refer to the [LICENSE](LICENSE.txt) file.

## Contribution

If you would like to contribute to this project, please contact us through [Issues](https://github.com/omnius-labs/axus/issues) or [Pull Requests](https://github.com/omnius-labs/axus/pulls) on GitHub.
