$env:DOTNET_CLI_TELEMETRY_OPTOUT = 1

# *.rpf
$RPFC_PATH = Join-Path $PWD "/refs/core-cs/src/Core.RocketPack.DefinitionCompiler"
dotnet run --project $RPFC_PATH -- -c (Join-Path $PWD "rpfs/config.yml")
