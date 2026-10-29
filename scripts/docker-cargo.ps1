param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CargoArgs = @("build")
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$volumePath = $root -replace '\\', '/'
$image = "logcli-rust:dev"

docker build -q -f "$PSScriptRoot/Dockerfile.dev" -t $image $root | Out-Null
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

docker run --rm `
    -v "${volumePath}:/work" `
    -v logcli-target:/work/target `
    -v logcli-registry:/usr/local/cargo/registry `
    -w /work `
    $image `
    cargo @CargoArgs

exit $LASTEXITCODE
