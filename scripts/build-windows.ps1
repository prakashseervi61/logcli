param(
    [switch] $Install
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$volumePath = $root -replace '\\', '/'
$image = "logcli-rust:windows"
$dist = Join-Path $root "dist"

New-Item -ItemType Directory -Force -Path $dist | Out-Null

docker build -q -f "$PSScriptRoot/Dockerfile.windows" -t $image $root | Out-Null
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

docker run --rm `
    -v "${volumePath}:/work" `
    -v "${volumePath}/dist:/out" `
    -v logcli-target:/work/target `
    -v logcli-registry:/usr/local/cargo/registry `
    -w /work `
    $image `
    sh -c 'cargo build --release --target x86_64-pc-windows-gnu && cp target/x86_64-pc-windows-gnu/release/logcli.exe /out/'

if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $dist "logcli.exe"
Write-Host "Built $exe"

if ($Install) {
    $bin = Join-Path $env:USERPROFILE "bin"
    New-Item -ItemType Directory -Force -Path $bin | Out-Null
    Copy-Item -Force $exe (Join-Path $bin "logcli.exe")

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ([string]::IsNullOrEmpty($userPath)) {
        $userPath = ""
    }
    $entries = $userPath -split ';' | Where-Object { $_ -ne "" }
    if ($entries -notcontains $bin) {
        $updated = (@($entries) + $bin) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $updated, "User")
        Write-Host "Installed to $bin and added it to the user PATH."
        Write-Host "Open a new terminal, then run: logcli --version"
    } else {
        Write-Host "Installed to $bin (already on the user PATH)."
    }
}
