#Requires -Version 5.1
<#
.SYNOPSIS
    Installs open-dataset-cleaner (odc) on Windows.

.EXAMPLE
    irm https://raw.githubusercontent.com/tonrakun/open-dataset-cleaner/main/scripts/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version v0.1.0 -InstallDir D:\tools\odc
#>
[CmdletBinding()]
param(
    [string]$Version = $(if ($env:ODC_VERSION) { $env:ODC_VERSION } else { "latest" }),
    [string]$InstallDir = $(if ($env:ODC_INSTALL_DIR) { $env:ODC_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "odc" }),
    [string]$AssetUrl = $env:ODC_ASSET_URL,
    [switch]$NoModifyPath
)

$ErrorActionPreference = "Stop"

$Repo = "tonrakun/open-dataset-cleaner"
$BinName = "odc.exe"
$Target = "x86_64-pc-windows-msvc"

function Write-Info($msg) {
    Write-Host "[odc-install] $msg"
}

function Resolve-AssetUrl {
    if ($AssetUrl) {
        $script:ResolvedVersion = "(custom url)"
        return $AssetUrl
    }

    if ($Version -eq "latest") {
        Write-Info "resolving latest release..."
        $release = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$Repo/releases/latest"
        $script:ResolvedVersion = $release.tag_name
        if (-not $script:ResolvedVersion) {
            throw "could not resolve latest release tag"
        }
    } else {
        $script:ResolvedVersion = $Version
    }

    return "https://github.com/$Repo/releases/download/$($script:ResolvedVersion)/odc-$Target.zip"
}

function Add-InstallDirToPath {
    param([string]$Dir)

    $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if (-not $userPath) { $userPath = "" }

    $entries = $userPath -split ";" | Where-Object { $_ -ne "" }
    if ($entries -contains $Dir) {
        Write-Info "PATH already contains $Dir"
        return
    }

    $newPath = if ($userPath) { "$userPath;$Dir" } else { $Dir }
    [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
    $env:PATH = "$env:PATH;$Dir"
    Write-Info "added $Dir to User PATH (open a new terminal to pick it up)"
}

function Main {
    $url = Resolve-AssetUrl
    Write-Info "version: $($script:ResolvedVersion)"
    Write-Info "target:  $Target"
    Write-Info "url:     $url"

    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("odc-install-" + [System.IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Path $tmpDir | Out-Null

    try {
        $archivePath = Join-Path $tmpDir "odc.zip"
        Write-Info "downloading..."
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archivePath

        Write-Info "extracting..."
        Expand-Archive -Path $archivePath -DestinationPath $tmpDir -Force

        $binPath = Get-ChildItem -Path $tmpDir -Recurse -Filter $BinName | Select-Object -First 1
        if (-not $binPath) {
            throw "binary '$BinName' not found in archive"
        }

        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        Copy-Item -Path $binPath.FullName -Destination (Join-Path $InstallDir $BinName) -Force
        Write-Info "installed to $(Join-Path $InstallDir $BinName)"

        if (-not $NoModifyPath) {
            Add-InstallDirToPath -Dir $InstallDir
        } else {
            Write-Info "skipping PATH update (-NoModifyPath)"
        }

        Write-Info "done. run 'odc --help' to get started (open a new terminal if PATH was just updated)."
    }
    finally {
        Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Main
