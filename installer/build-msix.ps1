# Builds the Microsoft Store package: dist\VeraView-<version>.msix (unsigned; the Store signs it).
#
#   -TestSign   also writes dist\VeraView-<version>-test.msix, signed with a self-signed
#               certificate whose subject matches the manifest's Publisher, plus
#               dist\VeraView-test.cer. Trust that certificate (Local Machine > Trusted People)
#               to install the test package on this PC. Don't upload the test package.
#
# Requires the Windows 10/11 SDK (MakeAppx, MakePri, SignTool) and cargo-about.
param([switch]$TestSign)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Push-Location $root
try {
    if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) {
        throw "cargo-about not found. Install it with: cargo install cargo-about --locked --features cli"
    }
    cargo about generate installer\notices.hbs -o THIRD-PARTY-NOTICES.txt
    if ($LASTEXITCODE) { throw "cargo about failed (check for unaccepted licenses in about.toml)" }
    cargo build --release
    if ($LASTEXITCODE) { throw "cargo build failed" }

    # The Store requires a four-part version whose last part is 0.
    $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
    $msixVersion = "$version.0"

    $sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1
    if (-not $sdk) { throw "Windows SDK not found (needs makeappx.exe). Install the Windows 10/11 SDK." }
    $bin = $sdk.DirectoryName

    # Package layout.
    $stage = "target\msix"
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory "$stage\Assets" | Out-Null
    # README.md is developer documentation, so it's left out of the Store package.
    Copy-Item target\release\vera-view.exe, THIRD-PARTY-NOTICES.txt $stage
    Copy-Item assets\msix\*.png "$stage\Assets"
    $manifest = (Get-Content installer\msix\AppxManifest.xml -Raw).Replace("__VERSION__", $msixVersion)
    [IO.File]::WriteAllText((Join-Path $root "$stage\AppxManifest.xml"), $manifest, (New-Object Text.UTF8Encoding($false)))

    # resources.pri tells Windows which logo file to use for each size and display scale.
    & "$bin\makepri.exe" createconfig /cf target\msix-priconfig.xml /dq en-US /pv 10.0.0 /o | Out-Null
    if ($LASTEXITCODE) { throw "makepri createconfig failed" }
    # The default config splits each display scale into a separate resource package, which
    # only works in an app bundle. Keep every variant in the single resources.pri instead.
    [xml]$priConfig = Get-Content target\msix-priconfig.xml
    $packaging = $priConfig.SelectSingleNode("//packaging")
    if ($packaging) { [void]$packaging.ParentNode.RemoveChild($packaging) }
    $priConfig.Save((Join-Path $root "target\msix-priconfig.xml"))
    & "$bin\makepri.exe" new /pr $stage /cf target\msix-priconfig.xml /mn "$stage\AppxManifest.xml" /of "$stage\resources.pri" /o | Out-Null
    if ($LASTEXITCODE) { throw "makepri new failed" }

    New-Item -ItemType Directory dist -Force | Out-Null
    $msix = "dist\VeraView-$version.msix"
    & "$bin\makeappx.exe" pack /d $stage /p $msix /o | Out-Null
    if ($LASTEXITCODE) { throw "makeappx pack failed" }
    Get-Item $msix

    if ($TestSign) {
        $publisher = ([xml]$manifest).Package.Identity.Publisher
        $cert = Get-ChildItem Cert:\CurrentUser\My |
            Where-Object { $_.Subject -eq $publisher -and $_.FriendlyName -eq "Vera View test signing" } |
            Select-Object -First 1
        if (-not $cert) {
            $cert = New-SelfSignedCertificate -Type Custom -Subject $publisher -KeyUsage DigitalSignature `
                -FriendlyName "Vera View test signing" -CertStoreLocation Cert:\CurrentUser\My `
                -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
        }
        $test = "dist\VeraView-$version-test.msix"
        Copy-Item $msix $test -Force
        & "$bin\signtool.exe" sign /fd SHA256 /sha1 $cert.Thumbprint $test | Out-Null
        if ($LASTEXITCODE) { throw "signtool failed" }
        Export-Certificate -Cert $cert -FilePath dist\VeraView-test.cer | Out-Null
        Get-Item $test, dist\VeraView-test.cer
    }
}
finally {
    Pop-Location
}
