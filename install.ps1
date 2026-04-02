# install.ps1 — CCX installer for Windows
$repo = "anton-abyzov/ccx-rs"
$artifact = "ccx-windows-x64.exe"
$defaultInstallDir = "$env:USERPROFILE\.ccx\bin"
$installDir = if ($env:CCX_INSTALL_DIR) { $env:CCX_INSTALL_DIR } else { $defaultInstallDir }

function Test-DirectoryWritable([string]$dir) {
    try {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $probe = Join-Path $dir ".ccx-write-test"
        Set-Content -Path $probe -Value "ok" -NoNewline
        Remove-Item $probe -Force
        return $true
    } catch {
        return $false
    }
}

if (-not $env:CCX_INSTALL_DIR) {
    $existing = Get-Command ccx -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($existing) {
        $existingDir = Split-Path $existing.Source -Parent
        if (Test-DirectoryWritable $existingDir) {
            $installDir = $existingDir
        }
    }
}

Write-Host "Installing CCX for Windows..."

# Create install directory
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

# Download latest release
$url = "https://github.com/$repo/releases/latest/download/$artifact"
Write-Host "Downloading from: $url"
Invoke-WebRequest -Uri $url -OutFile "$installDir\ccx.exe"

# Add to PATH (current session + user-level), prepending so new install wins.
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
$pathEntries = @()
if ($currentPath) {
    $pathEntries = $currentPath -split ';' | Where-Object { $_ -and $_ -ne $installDir }
}
$newUserPath = (($installDir) + ';' + ($pathEntries -join ';')).TrimEnd(';')
[Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")

$sessionEntries = $env:Path -split ';' | Where-Object { $_ -and $_ -ne $installDir }
$env:Path = (($installDir) + ';' + ($sessionEntries -join ';')).TrimEnd(';')

Write-Host ""
Write-Host "CCX installed to $installDir\ccx.exe"
Write-Host "Prepended $installDir to PATH"
Write-Host ""
Write-Host "Get started:"
Write-Host '  $env:OPENROUTER_API_KEY="your-key-from-openrouter.ai/keys"'
Write-Host '  ccx chat --provider openrouter --model "nvidia/nemotron-3-super-120b-a12b:free"'
