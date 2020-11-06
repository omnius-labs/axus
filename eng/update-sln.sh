#!/bin/bash

dotnet new sln --force
dotnet sln xeus.sln add ./src/**/*.csproj
dotnet sln xeus.sln add ./test/**/*.csproj
dotnet sln xeus.sln add ./refs/core/src/**/*.csproj
dotnet sln xeus.sln add ./refs/core/test/**/*.csproj
