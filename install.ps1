# install.ps1 — CCX installer for Windows
$repo = "anton-abyzov/ccx-rs"
$artifact = "ccx-windows-x64.exe"
$installDir = "$env:USERPROFILE\.ccx\bin"

Write-Host "Installing CCX for Windows..."

# Create install directory
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

# Download latest release
$url = "https://github.com/$repo/releases/latest/download/$artifact"
Write-Host "Downloading from: $url"
Invoke-WebRequest -Uri $url -OutFile "$installDir\ccx.exe"

# Add to PATH (user-level)
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$installDir", "User")
    Write-Host "Added $installDir to PATH"
}

Write-Host ""
Write-Host "CCX installed to $installDir\ccx.exe"
Write-Host ""
Write-Host "Get started:"
Write-Host '  $env:OPENROUTER_API_KEY="your-key-from-openrouter.ai/keys"'
Write-Host '  ccx chat --provider openrouter --model "nvidia/nemotron-3-super-120b-a12b:free"'
