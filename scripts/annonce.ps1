# Envoie un message a tous les utilisateurs de Plume, dans le MP officiel Plume
# (alerte, son et pastille comme pour une nouvelle version). Il arrive chez
# tout le monde en 10 a 15 minutes.
#   powershell -File scripts\annonce.ps1 -Titre "Coucou" -Message "Ligne 1\nLigne 2" [-Lien https://...]
#
# Le message est ajoute a announcements.json (les 20 derniers sont gardes),
# puis pousse sur GitHub : c'est ce fichier que lit Plumecord. Seul le
# proprietaire du depot peut y ecrire.
# Texte sans accents : Windows PowerShell 5 lit ce fichier en ANSI.

param(
    [Parameter(Mandatory)][string]$Titre,
    # Markdown de Discord accepte ; \n pour aller a la ligne.
    [Parameter(Mandatory)][string]$Message,
    # Bouton "Ouvrir le lien" sous le message (https seulement).
    [string]$Lien
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
$file = Join-Path $root "announcements.json"

if ($Lien -and $Lien -notmatch '^https://') { throw "Le lien doit commencer par https://" }

$list = @()
# foreach : PowerShell 5 rend le tableau JSON comme un seul objet, il faut le deplier.
if ((Test-Path $file) -and (Get-Item $file).Length -gt 0) { foreach ($x in (Get-Content $file -Raw -Encoding UTF8 | ConvertFrom-Json)) { $list += $x } }

$now = [DateTimeOffset]::UtcNow
$item = [ordered]@{
    id      = $now.ToUnixTimeMilliseconds()
    date    = $now.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    title   = $Titre
    message = $Message -replace '\\n', "`n"
}
if ($Lien) { $item.link = $Lien }
$list = @(@([pscustomobject]$item) + $list | Select-Object -First 20)

# UTF-8 sans BOM, et toujours un tableau (meme avec un seul message).
[IO.File]::WriteAllText($file, (ConvertTo-Json -InputObject $list -Depth 4), [Text.UTF8Encoding]::new($false))

# Chaque etape verifiee : on n'annonce "envoyee" que si le push a marche.
# --autostash : d'autres fichiers modifies dans le depot ne bloquent pas l'envoi.
git pull --rebase --autostash --quiet
if ($LASTEXITCODE) { throw "mise a jour depuis GitHub echouee (git pull)" }
git add announcements.json
git commit --quiet -m "Annonce : $Titre"
if ($LASTEXITCODE) { throw "commit echoue" }
git push --quiet
if ($LASTEXITCODE) { throw "envoi sur GitHub echoue (git push)" }
Write-Host "Annonce envoyee. Elle arrive chez tous les utilisateurs de Plume d'ici 10 a 15 minutes." -ForegroundColor Green
