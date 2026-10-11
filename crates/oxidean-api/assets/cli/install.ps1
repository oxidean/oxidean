# ox — Oxidean CLI installer (Windows). Served by the instance itself:
#   irm @OXIDEAN_ORIGIN@/cli/install.ps1 | iex
# ASCII only — Windows PowerShell 5.1 misreads BOM-less UTF-8.
#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$Base = '@OXIDEAN_ORIGIN@'
$Dest = if ($env:OXIDEAN_INSTALL_DIR) { $env:OXIDEAN_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\ox' }
# Binaries are served by the instance at $Base/cli/bin; OXIDEAN_BIN_BASE
# points at any flat mirror instead (e.g. a GitHub release download URL).
$BinBase = if ($env:OXIDEAN_BIN_BASE) { $env:OXIDEAN_BIN_BASE } else { "$Base/cli/bin" }

$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($arch) {
    'X64'   { $target = 'x86_64-pc-windows-msvc' }
    'Arm64' { $target = 'aarch64-pc-windows-msvc' }
    default {
        Write-Error ("unsupported architecture '{0}' — build from source: cargo install --path crates/oxidean-cli" -f $arch)
    }
}

Write-Host "installing ox for $target from $Base"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("ox-install-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $binUrl = "$BinBase/ox-$target"
    $oxPath = Join-Path $tmp 'ox.exe'
    Invoke-WebRequest -Uri $binUrl -OutFile $oxPath -UseBasicParsing

    # Verify against the published digest when the instance serves one.
    $sha = $null
    try { $sha = (Invoke-WebRequest -Uri "$binUrl.sha256" -UseBasicParsing).Content } catch { }
    if ($sha) {
        $expected = ($sha -split '\s+')[0]
        $actual = (Get-FileHash -Path $oxPath -Algorithm SHA256).Hash
        if ($expected -and ($actual -ine $expected)) {
            throw "checksum mismatch for ox-$target — aborting"
        }
    }

    New-Item -ItemType Directory -Force -Path $Dest | Out-Null
    $destBin = Join-Path $Dest 'ox.exe'
    Move-Item -Force $oxPath $destBin
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host "installed ox to $destBin"
& $destBin --version

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (($userPath -split ';') -contains $Dest)) {
    Write-Host "note: $Dest is not on your PATH — add it for your user with:"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `"`$env:Path;$Dest`", 'User')"
}
Write-Host "next: ox auth login --instance $Base --token <pat>"
