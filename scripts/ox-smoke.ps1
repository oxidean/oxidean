# ox binary smoke (Windows legs) — same assertions as ox-smoke.sh plus the
# %APPDATA% config-dir fallback the Windows build relies on.
param([Parameter(Mandatory = $true)][string]$Ox)
$ErrorActionPreference = 'Stop'

$out = & $Ox --version
if ($out -notmatch '^ox \d+\.\d+\.\d+') { throw "bad --version: $out" }

$help = & $Ox --help | Out-String
if (-not $help.Contains('ox <command>')) { throw '--help missing usage line' }
$bare = & $Ox | Out-String
if (-not $bare.Contains('ox <command>')) { throw 'bare help missing usage line' }

& $Ox frobnicate 2>$null | Out-Null
if ($LASTEXITCODE -eq 0) { throw 'unknown command exited 0' }

# Scrubbed env (no HOME/XDG/OXIDEAN_*): config must resolve via %APPDATA% and
# the command must reach the "no instance configured" usage error (exit 2)
# without touching the network.
$env:HOME = $null
$env:XDG_CONFIG_HOME = $null
$env:OXIDEAN_CONFIG_DIR = $null
$env:OXIDEAN_INSTANCE = $null
$env:OXIDEAN_TOKEN = $null
$err = & $Ox auth status 2>&1 | Out-String
if ($LASTEXITCODE -ne 2) { throw "auth status: expected exit 2, got $LASTEXITCODE — $err" }
if ($err -notmatch 'instance') { throw "auth status: unexpected message — $err" }

Write-Host "smoke OK: $out"
# $LASTEXITCODE still holds `ox auth status`'s expected 2 — without an
# explicit exit, pwsh propagates it and the step reads as failed.
exit 0
