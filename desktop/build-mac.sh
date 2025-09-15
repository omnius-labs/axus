#!/bin/bash
set -euo pipefail

rm -rf ./pub/osx-arm64/*

export NativeDepsPlatform=OSX
export PlatformTarget=ARM64

dotnet publish ./src/Axus.Ui.Desktop -p:PublishSingleFile=true --runtime osx-arm64 --configuration Release --self-contained true --output ./pub/osx-arm64/bin/
dotnet publish ./src/Axus.Updater -p:PublishSingleFile=true --runtime osx-arm64 --configuration Release --self-contained true --output ./pub/osx-arm64/bin/updater/
