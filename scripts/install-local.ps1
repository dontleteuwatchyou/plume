# Compile Plume et l'installe sur ce PC pour le tester, sans rien publier.
#   double-clic sur "Tester Plume.bat" a la racine du depot
# (texte sans accents : Windows PowerShell 5 lit ce fichier en ANSI)

param([string]$Key = "$env:USERPROFILE\.tauri\plume-updater.key")
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$version = (Get-Content src-tauri\tauri.conf.json -Raw -Encoding UTF8 | ConvertFrom-Json).version
Write-Host "== Plume $version : compilation (quelques minutes la premiere fois)" -ForegroundColor Magenta

if (-not (Test-Path node_modules)) { npm install; if ($LASTEXITCODE) { throw "npm install a echoue" } }

# Meme signature que release.ps1, sinon tauri refuse de finir (createUpdaterArtifacts).
if (Test-Path $Key) { $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $Key -Raw }
node -e "process.exitCode = require('child_process').spawnSync('npx tauri build', { shell: true, stdio: 'inherit', env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' } }).status"
if ($LASTEXITCODE) { throw "la compilation a echoue (voir les erreurs au-dessus)" }

$setup = "src-tauri\target\release\bundle\nsis\Plume_${version}_x64-setup.exe"
if (-not (Test-Path $setup)) { throw "installateur introuvable : $setup" }

Write-Host "== Fermeture de Plume et installation" -ForegroundColor Magenta
Get-Process plume -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
Start-Process $setup -ArgumentList '/S' -Wait

$exe = "$env:LOCALAPPDATA\Plume\plume.exe"
if (Test-Path $exe) { Start-Process $exe }
Write-Host "== Plume $version installe et relance." -ForegroundColor Green
