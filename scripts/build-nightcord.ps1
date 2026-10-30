# Recompile la version web de Nightcord avec les plugins que l'officielle
# retire (nightcord\plume.patch) et la prépare pour Plume.
#   powershell -File scripts\build-nightcord.ps1 -Install    # l'essayer dans ce Plume
#   powershell -File scripts\build-nightcord.ps1 -Publish    # la proposer à tous les Plume
#
# Publier = la release « nightcord » du dépôt (prérelease, pour ne jamais passer
# devant les versions de Plume que la mise à jour automatique consulte).
# Si le correctif ne s'applique plus sur une nouvelle version de Nightcord, le
# script s'arrête : il faut alors refaire nightcord\plume.patch.

param(
    [string]$Tag,
    [switch]$Install,
    [switch]$Publish,
    [string]$Repo = "dontleteuwatchyou/plume"
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root "target-nightcord"
$src = Join-Path $work "src"
$zip = Join-Path $work "nightcord-plume.zip"

if (-not $Tag) {
    $Tag = (Invoke-RestMethod "https://source.nightcord.st/api/v1/repos/nightcord/nightcord/releases/latest").tag_name
}
Write-Host "Nightcord $Tag…"

if (Test-Path $src) { Remove-Item $src -Recurse -Force }
New-Item -ItemType Directory -Force $work | Out-Null
git clone -q --depth 1 --branch $Tag https://source.nightcord.st/nightcord/nightcord.git $src
if ($LASTEXITCODE) { throw "échec du clonage" }

Push-Location $src
try {
    git apply --whitespace=nowarn (Join-Path $root "nightcord\plume.patch")
    if ($LASTEXITCODE) { throw "nightcord\plume.patch ne s'applique plus sur $Tag" }
    corepack pnpm install --frozen-lockfile
    if ($LASTEXITCODE) { throw "échec de l'installation des dépendances" }
    corepack pnpm buildWeb
    if ($LASTEXITCODE) { throw "échec de la compilation" }
    Copy-Item dist\extension-chrome.zip $zip -Force
} finally {
    Pop-Location
}
Write-Host "Extension prête : $zip"

if ($Install) {
    # Nouveau dossier de version, comme le fait Plume (mods.rs) : il sera pris
    # au prochain démarrage et l'ancien supprimé.
    $dest = Join-Path $env:LOCALAPPDATA "app.plume.desktop\mods\nightcord\v$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())"
    Expand-Archive $zip $dest
    Write-Host "Installée dans $dest : relance Plume pour l'utiliser."
}

if ($Publish) {
    $gh = "$env:LOCALAPPDATA\Programs\gh\bin\gh.exe"
    if (-not (Test-Path $gh)) { $gh = 'gh' }
    & $gh release view nightcord --repo $Repo *> $null
    if ($LASTEXITCODE) {
        & $gh release create nightcord --repo $Repo --prerelease --title "Nightcord pour Plume" --notes "Version web de Nightcord recompilée pour Plume. Mise à jour par scripts/build-nightcord.ps1."
        if ($LASTEXITCODE) { throw "échec de la création de la release" }
    }
    & $gh release upload nightcord $zip --repo $Repo --clobber
    if ($LASTEXITCODE) { throw "échec de l'envoi" }
    Write-Host "Publiée : les Plume la prendront à leur prochaine vérification (6 h max)."
}
