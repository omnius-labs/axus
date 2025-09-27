#!/bin/bash
set -euo pipefail

rm -rf ./pub/linux-x64/*

export NativeDepsPlatform=Linux
export PlatformTarget=x64

dotnet publish ./src/Axus.Ui.Desktop -p:PublishSingleFile=true --runtime linux-x64 --configuration Release --self-contained true --output ./pub/linux-x64/bin/
dotnet publish ./src/Axus.Updater -p:PublishSingleFile=true --runtime linux-x64 --configuration Release --self-contained true --output ./pub/linux-x64/bin/updater/
