#!/bin/bash
set -euo pipefail

dotnet new sln --force -n axus
dotnet sln add ./refs/core-cs/src/**/*.csproj
dotnet sln add ./refs/core-cs/test/**/*.csproj
dotnet sln add ./src/**/*.csproj
dotnet sln add ./test/**/*.csproj
