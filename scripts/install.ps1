# Multifrost Router — Windows installer (PowerShell)
#
# Usage:
#   irm https://raw.githubusercontent.com/lirrensi/Multifrost/main/scripts/install.ps1 | iex
#   .\install.ps1 -Version v5.0.0
#   .\install.ps1 -Help

param(
    [string]$Version = "",
    [string]$Dir = "",
    [switch]$Help = $false
)

$ErrorActionPreference = "Stop"

$GithubRepo = if ($env:MULTIFROST_INSTALL_REPO) { $env:MULTIFROST_INSTALL_REPO } else { "lirrensi/Multifrost" }
$BinName = "multifrost-router.exe"

# ── helpers ──────────────────────────────────────────────────────────────────

function Write-Err {
    Write-Host "Error: $args" -ForegroundColor Red
    exit 1
}

# ── help ─────────────────────────────────────────────────────────────────────

if ($Help) {
    @"
Usage: .\install.ps1 [-Version vX.Y.Z] [-Dir C:\path] [-Help]

One-line installer for the Multifrost router binary.

Options:
  -Version vX.Y.Z  Install a specific version (default: latest release)
  -Dir path         Install directory (default: %USERPROFILE%\.local\bin)

The script detects your OS and architecture automatically.
"@
    exit 0
}

# ── version ──────────────────────────────────────────────────────────────────

if (-not $Version) {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$GithubRepo/releases/latest"
        $Version = $release.tag_name
    } catch {
        Write-Err "Could not determine latest version from GitHub. Pass -Version manually."
    }
}

# ── platform ─────────────────────────────────────────────────────────────────

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "AMD64") { $arch = "x86_64" }
elseif ($arch -eq "ARM64") { $arch = "aarch64" }
else { Write-Err "Unsupported architecture: $arch" }

$platform = "windows-$arch"

# The release asset name: multifrost-router-windows-x86_64.exe
$asset = "multifrost-router-$platform.exe"

$downloadUrl = "https://github.com/$GithubRepo/releases/download/$Version/$asset"

# ── install dir ──────────────────────────────────────────────────────────────

if (-not $Dir) {
    $Dir = Join-Path $env:USERPROFILE ".local\bin"
}

if (-not (Test-Path $Dir)) {
    New-Item -ItemType Directory -Path $Dir -Force | Out-Null
}

$dest = Join-Path $Dir $BinName

# ── download and install ─────────────────────────────────────────────────────

Write-Host "Installing Multifrost Router $Version for $platform..."
Write-Host "  Downloading: $downloadUrl"

$tmp = Join-Path $env:TEMP "multifrost-router-$((Get-Date).Ticks).exe"

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $tmp -ErrorAction Stop
} catch {
    $status = $_.Exception.Response.StatusCode.value__
    Write-Err "Download failed (HTTP $status). Is version '$Version' correct?"
}

Copy-Item $tmp $dest -Force
Remove-Item $tmp -Force

Write-Host "  Installed: $dest"

# ── PATH check ───────────────────────────────────────────────────────────────

$pathDirs = $env:PATH -split ';'
if ($Dir -notin $pathDirs) {
    Write-Host ""
    Write-Host "Note: $Dir is not on your PATH."
    Write-Host "Add it via:"
    Write-Host '  [Environment]::SetEnvironmentVariable("PATH", $env:PATH + ";' + $Dir + '", "User")'
    Write-Host "Then restart your terminal."
}

Write-Host ""
Write-Host "Multifrost Router installed. Run: multifrost-router"
