# Compare la RAM de Plume et de Discord, tous processus compris
# (pour Plume : plume.exe + les processus WebView2 qu'il a lancés).
#   powershell -File scripts\ram.ps1            une mesure
#   powershell -File scripts\ram.ps1 -Samples 10 -Interval 3   moyenne sur 30 s

param([int]$Samples = 1, [int]$Interval = 3)

function Get-Tree([string]$RootName) {
    $all = Get-CimInstance Win32_Process
    $ids = @($all | Where-Object { $_.Name -eq $RootName } | ForEach-Object ProcessId)
    do {
        $more = @($all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId } | ForEach-Object ProcessId)
        $ids += $more
    } while ($more.Count)
    $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
}

$rows = @{}
for ($i = 0; $i -lt $Samples; $i++) {
    foreach ($app in @(@{ Nom = 'Discord'; Exe = 'Discord.exe' }, @{ Nom = 'Plume'; Exe = 'plume.exe' })) {
        $p = @(Get-Tree $app.Exe)
        if (-not $p.Count) { continue }
        if (-not $rows[$app.Nom]) { $rows[$app.Nom] = @{ WS = 0; Priv = 0; N = 0; Proc = 0 } }
        $r = $rows[$app.Nom]
        $r.WS += ($p | Measure-Object WorkingSet64 -Sum).Sum
        $r.Priv += ($p | Measure-Object PrivateMemorySize64 -Sum).Sum
        $r.Proc = $p.Count
        $r.N++
    }
    if ($i -lt $Samples - 1) { Start-Sleep $Interval }
}

$rows.GetEnumerator() | Sort-Object Name | ForEach-Object {
    [pscustomobject]@{
        App            = $_.Name
        Processus      = $_.Value.Proc
        'RAM (Mo)'     = [math]::Round($_.Value.WS / $_.Value.N / 1MB)
        'Privée (Mo)'  = [math]::Round($_.Value.Priv / $_.Value.N / 1MB)
    }
} | Format-Table -AutoSize
