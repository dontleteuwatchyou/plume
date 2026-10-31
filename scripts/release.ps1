# Publie une version de Plume sur GitHub, prête pour la mise à jour automatique.
#   powershell -File scripts\release.ps1 -NotesFile notes\0.3.12.md
#   (ou -Notes "Ce qui change" pour une ligne)
#
# 1. compile et signe l'installateur avec la clé privée (hors du dépôt) ;
# 2. écrit latest.json, que les Plume installés consultent ;
# 3. pousse le code et crée la release vX.Y.Z avec les deux fichiers.
# La version vient de src-tauri\tauri.conf.json : la monter avant de publier.

param(
    [string]$Notes,
    # Notes de version en Markdown (notes\X.Y.Z.md) : telles quelles sur GitHub
    # et dans le MP Plume, sans la mise en forme dans la fenetre de mise a jour.
    [string]$NotesFile,
    [string]$Key = "$env:USERPROFILE\.tauri\plume-updater.key",
    [string]$Repo = "dontleteuwatchyou/plume"
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if ($NotesFile) { $NotesFile = (Resolve-Path $NotesFile).Path; $Notes = Get-Content $NotesFile -Raw -Encoding UTF8 }
if (-not $Notes) { throw "Donner -NotesFile ou -Notes" }
# Texte simple pour Plume (titres et gras Markdown retires).
$plain = (($Notes -split "`r?`n") | ForEach-Object { $_ -replace '^#+\s*', '' -replace '\*\*?', '' }) -join "`n"

if (-not (Test-Path $Key)) { throw "Clé de signature introuvable : $Key" }
$version = (Get-Content src-tauri\tauri.conf.json -Raw | ConvertFrom-Json).version
$tag = "v$version"
$gh = "$env:LOCALAPPDATA\Programs\gh\bin\gh.exe"
if (-not (Test-Path $gh)) { $gh = 'gh' }

Write-Host "Compilation et signature de Plume $version…"
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $Key -Raw
# La clé n'a pas de mot de passe, mais Tauri le demande si la variable
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD n'existe pas. Windows PowerShell supprime
# une variable qu'on vide et ne transmet pas les valeurs vides : c'est Node qui
# lance la compilation, avec la variable bien présente et vide.
node -e "process.exitCode = require('child_process').spawnSync('npx tauri build', { shell: true, stdio: 'inherit', env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' } }).status"
if ($LASTEXITCODE) { throw "échec de la compilation" }

$setup = "src-tauri\target\release\bundle\nsis\Plume_${version}_x64-setup.exe"
$sig = "$setup.sig"
if (-not (Test-Path $sig)) { throw "Signature absente : $sig" }

$latest = [ordered]@{
    version   = $version
    notes     = $plain.Trim()
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
if ($NotesFile) {
    & $gh release create $tag $setup $latestPath --repo $Repo --title "Plume $version" --notes-file $NotesFile
} else {
    & $gh release create $tag $setup $latestPath --repo $Repo --title "Plume $version" --notes $Notes
}
if ($LASTEXITCODE) { throw "échec de la release" }
Write-Host "Plume $version publié. Les Plume installés le proposeront au prochain démarrage."
