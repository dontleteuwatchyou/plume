# Publie une version de Plume sur GitHub, prête pour la mise à jour automatique.
#   powershell -File scripts\release.ps1 -Notes "Ce qui change"
#
# 1. compile et signe l'installateur avec la clé privée (hors du dépôt) ;
# 2. écrit latest.json, que les Plume installés consultent ;
# 3. pousse le code et crée la release vX.Y.Z avec les deux fichiers.
# La version vient de src-tauri\tauri.conf.json : la monter avant de publier.

param(
    [Parameter(Mandatory)][string]$Notes,
    [string]$Key = "$env:USERPROFILE\.tauri\plume-updater.key",
    [string]$Repo = "dontleteuwatchyou/plume"
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if (-not (Test-Path $Key)) { throw "Clé de signature introuvable : $Key" }
$version = (Get-Content src-tauri\tauri.conf.json -Raw | ConvertFrom-Json).version
$tag = "v$version"
$gh = "$env:LOCALAPPDATA\Programs\gh\bin\gh.exe"
if (-not (Test-Path $gh)) { $gh = 'gh' }

Write-Host "Compilation et signature de Plume $version…"
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $Key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
npx tauri build
if ($LASTEXITCODE) { throw "échec de la compilation" }

$setup = "src-tauri\target\release\bundle\nsis\Plume_${version}_x64-setup.exe"
$sig = "$setup.sig"
if (-not (Test-Path $sig)) { throw "Signature absente : $sig" }

$latest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = (Get-Content $sig -Raw).Trim()
            url       = "https://github.com/$Repo/releases/download/$tag/Plume_${version}_x64-setup.exe"
        }
    }
}
$latestPath = "src-tauri\target\release\bundle\nsis\latest.json"
# UTF-8 sans BOM : le module de mise à jour refuse un JSON précédé d'un BOM.
[IO.File]::WriteAllText((Join-Path $root $latestPath), ($latest | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))

Write-Host "Publication de $tag…"
git push
& $gh release create $tag $setup $latestPath --repo $Repo --title "Plume $version" --notes $Notes
if ($LASTEXITCODE) { throw "échec de la release" }
Write-Host "Plume $version publié. Les Plume installés le proposeront au prochain démarrage."
