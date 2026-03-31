param(
    [string]$Namespace = "azums",
    [string]$DbPodLabel = "app=postgres",
    [string]$DbUser = "app",
    [string]$DbName = "azums",
    [string]$BackupDir = ".live-run-logs/db-backups",
    [switch]$SkipRestoreDrill
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExit([string]$Context) {
    if ($LASTEXITCODE -ne 0) {
        throw "$Context (exit code $LASTEXITCODE)"
    }
}

New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null

$pod = (& kubectl -n $Namespace get pod -l $DbPodLabel -o jsonpath='{.items[0].metadata.name}').Trim()
if ([string]::IsNullOrWhiteSpace($pod)) {
    throw "No postgres pod found for label $DbPodLabel in namespace $Namespace"
}

$ts = Get-Date -Format "yyyyMMdd-HHmmss"
$backupFile = Join-Path $BackupDir "azums-$ts.dump"
$remoteDumpPath = "/tmp/azums-$ts.dump"

Write-Host "Using pod      : $pod"
Write-Host "Backup file    : $backupFile"
Write-Host "Creating backup..."
& kubectl -n $Namespace exec $pod -- pg_dump -U $DbUser -d $DbName -Fc > $backupFile
Assert-LastExit "pg_dump backup failed"

if (-not (Test-Path $backupFile)) {
    throw "Backup file was not created."
}

$fileSize = (Get-Item $backupFile).Length
if ($fileSize -le 0) {
    throw "Backup file is empty."
}
Write-Host "Backup complete. Size: $fileSize bytes"

if ($SkipRestoreDrill) {
    Write-Host "Skipping restore drill by request."
    return
}

$restoreDb = ("azums_restore_drill_{0}" -f $ts.Replace("-", "_"))
Write-Host "Running restore drill into database: $restoreDb"

& kubectl -n $Namespace cp $backupFile "${pod}:$remoteDumpPath"
Assert-LastExit "kubectl cp backup into postgres pod failed"
$restoreCreated = $false
try {
    & kubectl -n $Namespace exec $pod -- psql -U $DbUser -d postgres -v ON_ERROR_STOP=1 -c "CREATE DATABASE $restoreDb;"
    Assert-LastExit "create restore drill database failed"
    $restoreCreated = $true

    & kubectl -n $Namespace exec $pod -- pg_restore -U $DbUser -d $restoreDb --no-owner --no-privileges $remoteDumpPath
    Assert-LastExit "pg_restore failed"

    $checks = @(
        "SELECT COUNT(*) AS jobs_total FROM jobs;",
        "SELECT COUNT(*) AS callback_deliveries_total FROM callback_core_deliveries;",
        "SELECT COUNT(*) AS receipts_total FROM execution_core_receipts;"
    )

    foreach ($sql in $checks) {
        & kubectl -n $Namespace exec $pod -- psql -U $DbUser -d $restoreDb -v ON_ERROR_STOP=1 -c $sql
        Assert-LastExit "restore verification query failed"
    }
}
finally {
    Write-Host "Cleaning up restore drill database..."
    if ($restoreCreated) {
        & kubectl -n $Namespace exec $pod -- psql -U $DbUser -d postgres -v ON_ERROR_STOP=1 -c "DROP DATABASE IF EXISTS $restoreDb;"
    }
    & kubectl -n $Namespace exec $pod -- rm -f $remoteDumpPath
}

Write-Host "Backup + restore drill completed successfully."
