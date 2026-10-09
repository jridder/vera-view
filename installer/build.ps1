# Builds Vera View in release mode and packages it as dist\VeraView-Setup-<version>.exe.
# Requires Inno Setup 6 (winget install JRSoftware.InnoSetup) and
# cargo-about (cargo install cargo-about --locked --features cli).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Push-Location $root
try {
    # Regenerate the open-source license notices so they match the current dependencies.
    # This fails if a dependency uses a license not listed in about.toml.
    if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) {
        throw "cargo-about not found. Install it with: cargo install cargo-about --locked --features cli"
    }
    cargo about generate installer\notices.hbs -o THIRD-PARTY-NOTICES.txt
    if ($LASTEXITCODE) { throw "cargo about failed (check for unaccepted licenses in about.toml)" }

    cargo build --release
    if ($LASTEXITCODE) { throw "cargo build failed" }

    $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

    $iscc = @(
        (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source,
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) { throw "Inno Setup 6 not found. Install it with: winget install JRSoftware.InnoSetup" }

    & $iscc /Q "/DAppVersion=$version" installer\vera-view.iss
    if ($LASTEXITCODE) { throw "Inno Setup compilation failed" }
    Get-Item "dist\VeraView-Setup-$version.exe"
}
finally {
    Pop-Location
}
