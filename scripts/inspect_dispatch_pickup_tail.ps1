param(
    [string]$BaseUrl = "http://127.0.0.1:18000",
    [string]$ComposeProject = "azums-proof",
    [string]$ComposeFile = $(Join-Path $PSScriptRoot "..\\deployments\\docker\\docker-compose.images.yml"),
    [string]$DbService = "postgres",
    [string]$DbUser = "app",
    [string]$DbName = "azums",
    [int]$RequestCount = 20,
    [int]$SubmitConcurrency = 8,
    [int]$TerminalTimeoutSec = 300,
    [int]$SampleCount = 8,
    [int]$SampleIntervalMs = 500,
    [string]$BenchmarkJsonPath = "",
    [switch]$RunBenchmark
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-ComposeServiceContainer {
    param([Parameter(Mandatory = $true)][string]$Service)

    $json = docker ps --filter "label=com.docker.compose.project=$ComposeProject" --filter "label=com.docker.compose.service=$Service" --format "{{json .}}" 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace(($json | Out-String))) {
        return $null
    }

    $first = @($json | Select-Object -First 1)
    if ($first.Count -eq 0 -or [string]::IsNullOrWhiteSpace([string]$first[0])) {
        return $null
    }

    $record = $first[0] | ConvertFrom-Json
    [pscustomobject]@{
        Id    = [string]$record.ID
        Names = [string]$record.Names
    }
}

function Invoke-ComposePsqlCsv {
    param([Parameter(Mandatory = $true)][string]$Sql)

    $dbContainer = Get-ComposeServiceContainer -Service $DbService
    if ($null -eq $dbContainer) {
        throw "compose postgres container for service `$DbService` not found"
    }

    $output = & docker exec $dbContainer.Id psql -U $DbUser -d $DbName -A -F "," -P footer=off -c $Sql 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "psql query failed: $output"
    }
    return $output.Trim()
}

function Convert-CsvTextToRows {
    param([string]$CsvText)

    if ([string]::IsNullOrWhiteSpace($CsvText)) {
        return @()
    }

    $lines = @($CsvText -split "`r?`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($lines.Count -le 1) {
        return @()
    }

    return @($lines | ConvertFrom-Csv)
}

function Get-BenchmarkOutputPath {
    if (-not [string]::IsNullOrWhiteSpace($BenchmarkJsonPath)) {
        return $BenchmarkJsonPath
    }
    return (Join-Path $env:TEMP "azums_dispatch_pickup_tail.json")
}

function Get-MaxNumericValue {
    param(
        [object[]]$Rows,
        [string]$Property
    )

    $values = @()
    foreach ($row in @($Rows)) {
        $value = $row.$Property
        if ($null -ne $value -and -not [string]::IsNullOrWhiteSpace([string]$value)) {
            $values += [double]$value
        }
    }
    if ($values.Count -eq 0) {
        return $null
    }
    return (($values | Measure-Object -Maximum).Maximum)
}

function Get-AvgNumericValue {
    param(
        [object[]]$Rows,
        [string]$Property
    )

    $values = @()
    foreach ($row in @($Rows)) {
        $value = $row.$Property
        if ($null -ne $value -and -not [string]::IsNullOrWhiteSpace([string]$value)) {
            $values += [double]$value
        }
    }
    if ($values.Count -eq 0) {
        return $null
    }
    return [Math]::Round((($values | Measure-Object -Average).Average), 2)
}

function Start-BenchmarkJob {
    param([Parameter(Mandatory = $true)][string]$OutputPath)

    Start-Job -ScriptBlock {
        param($RepoRoot, $OutputPath, $BaseUrl, $ComposeProject, $RequestCount, $SubmitConcurrency, $TerminalTimeoutSec)
        Set-Location $RepoRoot
        & pwsh -File scripts/benchmark_platform.ps1 `
            -Runtime compose `
            -ComposeProject $ComposeProject `
            -BaseUrl $BaseUrl `
            -Scenario synthetic_success `
            -RequestCount $RequestCount `
            -SubmitConcurrency $SubmitConcurrency `
            -TerminalTimeoutSec $TerminalTimeoutSec `
            -OutputJsonPath $OutputPath | Out-Null
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    } -ArgumentList @(
        (Get-Location).Path,
        $OutputPath,
        $BaseUrl,
        $ComposeProject,
        $RequestCount,
        $SubmitConcurrency,
        $TerminalTimeoutSec
    )
}

$outputPath = Get-BenchmarkOutputPath
$samples = New-Object System.Collections.Generic.List[object]

if ($RunBenchmark) {
    $job = Start-BenchmarkJob -OutputPath $outputPath
    Start-Sleep -Seconds 2

    for ($i = 1; $i -le $SampleCount; $i++) {
        $waitSql = @"
COPY (
  SELECT
    coalesce(wait_event_type, 'cpu') AS wait_type,
    coalesce(wait_event, 'running') AS wait_event,
    count(*) AS backends
  FROM pg_stat_activity
  WHERE datname = '$DbName'
    AND pid <> pg_backend_pid()
    AND state <> 'idle'
  GROUP BY 1, 2
  ORDER BY 3 DESC
) TO STDOUT WITH CSV HEADER
"@
        $runningSql = @"
COPY (
  SELECT
    coalesce(locked_by, '<none>') AS worker_id,
    count(*) AS running_jobs
  FROM jobs
  WHERE queue = 'execution.dispatch'
    AND status = 'running'
  GROUP BY 1
  ORDER BY 2 DESC
) TO STDOUT WITH CSV HEADER
"@

        $waitRows = Convert-CsvTextToRows (Invoke-ComposePsqlCsv -Sql $waitSql)
        $runningRows = Convert-CsvTextToRows (Invoke-ComposePsqlCsv -Sql $runningSql)
        $samples.Add([pscustomobject]@{
            SampleIndex = $i
            WaitRows    = $waitRows
            RunningRows = $runningRows
        }) | Out-Null

        Start-Sleep -Milliseconds $SampleIntervalMs
    }

    Wait-Job -Job $job | Out-Null
    Receive-Job -Job $job | Out-Null
    Remove-Job -Job $job -Force | Out-Null
} elseif (-not (Test-Path $outputPath)) {
    throw "benchmark json not found at $outputPath; pass -RunBenchmark or provide -BenchmarkJsonPath"
}

$bench = Get-Content $outputPath -Raw | ConvertFrom-Json
$intentIds = @($bench.Requests | Where-Object { $_.SubmitAccepted } | Select-Object -ExpandProperty IntentId -Unique)
if ($intentIds.Count -eq 0) {
    throw "benchmark json contains no accepted intents"
}

$intentArray = ($intentIds | ForEach-Object { "'$_'" }) -join ","
$statsSql = @"
COPY (
  WITH selected_intents AS (
    SELECT unnest(ARRAY[$intentArray]) AS intent_id
  ),
  attempts AS (
    SELECT
      a.worker_id,
      a.started_at,
      a.finished_at,
      a.latency_ms,
      EXTRACT(EPOCH FROM (a.started_at - to_timestamp(i.received_at_ms / 1000.0))) * 1000 AS pickup_ms
    FROM selected_intents s
    JOIN execution_core_intents i
      ON i.intent_id = s.intent_id
    JOIN execution_core_jobs ej
      ON ej.intent_id = i.intent_id
     AND ej.tenant_id = i.tenant_id
    JOIN jobs q
      ON q.queue = 'execution.dispatch'
     AND q.payload_json->>'execution_job_id' = ej.job_id
    JOIN job_attempts a
      ON a.job_id = q.id
  ),
  sequenced AS (
    SELECT
      worker_id,
      latency_ms,
      pickup_ms,
      EXTRACT(EPOCH FROM (
        started_at - lag(finished_at) OVER (PARTITION BY worker_id ORDER BY started_at)
      )) * 1000 AS gap_after_prev_ms
    FROM attempts
  )
  SELECT
    worker_id,
    count(*) AS attempts,
    round(avg(pickup_ms)::numeric, 2) AS avg_pickup_ms,
    round((percentile_disc(0.95) WITHIN GROUP (ORDER BY pickup_ms))::numeric, 2) AS p95_pickup_ms,
    round(avg(latency_ms)::numeric, 2) AS avg_attempt_ms,
    round((percentile_disc(0.95) WITHIN GROUP (ORDER BY latency_ms))::numeric, 2) AS p95_attempt_ms,
    round(avg(coalesce(gap_after_prev_ms, 0))::numeric, 2) AS avg_gap_ms,
    round((percentile_disc(0.95) WITHIN GROUP (ORDER BY coalesce(gap_after_prev_ms, 0)))::numeric, 2) AS p95_gap_ms
  FROM sequenced
  GROUP BY worker_id
  ORDER BY worker_id
) TO STDOUT WITH CSV HEADER
"@
$perWorkerRows = Convert-CsvTextToRows (Invoke-ComposePsqlCsv -Sql $statsSql)

$maxGapP95 = Get-MaxNumericValue -Rows $perWorkerRows -Property "p95_gap_ms"
$avgGapMs = Get-AvgNumericValue -Rows $perWorkerRows -Property "avg_gap_ms"
$maxWaitBackends = 0
$dbWaitObserved = $false
foreach ($sample in $samples) {
    foreach ($row in @($sample.WaitRows)) {
        $backends = [int]$row.backends
        if ($backends -gt $maxWaitBackends) {
            $maxWaitBackends = $backends
        }
        if ([string]$row.wait_type -notin @("cpu", "Client")) {
            $dbWaitObserved = $true
        }
    }
}

$pickupP95 = [double]$bench.Summary.WorkerPickupDelayP95Ms
$enqueueP95 = [double]$bench.Summary.QueueEnqueueLatencyP95Ms
$attemptP95 = Get-MaxNumericValue -Rows $perWorkerRows -Property "p95_attempt_ms"

$diagnosis = "mixed"
if (-not $dbWaitObserved -and $maxGapP95 -lt 100 -and $enqueueP95 -le 5 -and $pickupP95 -gt 500) {
    $diagnosis = "worker_capacity_queueing"
} elseif ($maxGapP95 -ge 100 -and -not $dbWaitObserved) {
    $diagnosis = "worker_cadence"
} elseif ($dbWaitObserved) {
    $diagnosis = "db_contention"
} else {
    $diagnosis = "local_host_jitter_or_mixed"
}

Write-Host "=== Dispatch Pickup Tail Summary ==="
[pscustomobject]@{
    BenchmarkJsonPath          = $outputPath
    AcceptedCount              = $bench.Summary.AcceptedCount
    ThroughputRequestsPerSec   = $bench.Summary.ThroughputRequestsPerSecond
    QueueEnqueueP95Ms          = $bench.Summary.QueueEnqueueLatencyP95Ms
    WorkerPickupP95Ms          = $bench.Summary.WorkerPickupDelayP95Ms
    AcceptedToFinalP95Ms       = $bench.Summary.AcceptedToFinalP95Ms
    MaxObservedActiveBackends  = $maxWaitBackends
    DbContentionObserved       = $dbWaitObserved
    AvgWorkerGapMs             = $avgGapMs
    MaxWorkerGapP95Ms          = $maxGapP95
    MaxWorkerAttemptP95Ms      = $attemptP95
    Diagnosis                  = $diagnosis
} | Format-List

Write-Host ""
Write-Host "=== Per-Worker Attempts ==="
$perWorkerRows | Format-Table -AutoSize

if ($samples.Count -gt 0) {
    Write-Host ""
    Write-Host "=== Sampled Wait Events ==="
    foreach ($sample in $samples) {
        $summaryRows = @($sample.WaitRows | ForEach-Object {
            "{0}/{1}={2}" -f $_.wait_type, $_.wait_event, $_.backends
        })
        if ($summaryRows.Count -eq 0) {
            $summaryRows = @("<none>")
        }
        Write-Host ("sample {0}: {1}" -f $sample.SampleIndex, ($summaryRows -join ", "))
    }
}
