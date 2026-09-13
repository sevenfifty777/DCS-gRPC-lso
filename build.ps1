param(
    [ValidateSet("release", "debug")]
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"

$repoRoot = $PSScriptRoot
Set-Location $repoRoot

$cargoArguments = @("build", "--locked")
if ($Configuration -eq "release") {
    $cargoArguments += "--release"
}

& cargo @cargoArguments
$exitCode = $LASTEXITCODE

if ($exitCode -eq 0) {
    $binaryPath = Join-Path $repoRoot "target\$Configuration\lso.exe"
    Write-Host "Build succeeded: $binaryPath"
}

exit $exitCode
