param(
    [ValidateSet("auto", "k8s", "compose")]
    [string]$Runtime = "auto",
    [string]$Namespace = "azums",
    [string]$ComposeProject = "azums-proof",
    [string]$DbPodLabel = "app=postgres",
    [string]$DbService = "postgres",
    [string]$DbUser = "app",
    [string]$DbName = "azums",
    [string]$TenantId = "",
    [int]$LookbackHours = 168,
    [switch]$Apply
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

    return ($first[0] | ConvertFrom-Json)
}

function Resolve-RuntimeTarget {
    if ($Runtime -ne "auto") {
        return $Runtime
    }

    $composeDb = Get-ComposeServiceContainer -Service $DbService
    if ($null -ne $composeDb -and [string]::IsNullOrWhiteSpace($Namespace)) {
        return "compose"
    }

    return "k8s"
}

function Invoke-K8sPsql([string]$Sql) {
    $pod = (& kubectl -n $Namespace get pod -l $DbPodLabel -o jsonpath='{.items[0].metadata.name}' 2>$null | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($pod)) {
        throw "No postgres pod found for label $DbPodLabel in namespace $Namespace"
    }

    $output = & kubectl -n $Namespace exec $pod -- psql -U $DbUser -d $DbName -v ON_ERROR_STOP=1 -P footer=off -A -F "," -c $Sql 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "k8s psql query failed: $output"
    }
    return $output.Trim()
}

function Invoke-ComposePsql([string]$Sql) {
    $container = Get-ComposeServiceContainer -Service $DbService
    if ($null -eq $container) {
        throw "No compose postgres container found for project $ComposeProject service $DbService"
    }

    $output = & docker exec $container.ID psql -U $DbUser -d $DbName -v ON_ERROR_STOP=1 -P footer=off -A -F "," -c $Sql 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "compose psql query failed: $output"
    }
    return $output.Trim()
}

function Invoke-DbQuery([string]$Sql) {
    if ((Resolve-RuntimeTarget) -eq "compose") {
        return Invoke-ComposePsql $Sql
    }
    return Invoke-K8sPsql $Sql
}

$lookbackHours = [Math]::Max(1, [Math]::Min(24 * 30, $LookbackHours))
$tenantClause = if ([string]::IsNullOrWhiteSpace($TenantId)) {
    ""
} else {
    "AND receipts.tenant_id = '$($TenantId.Replace("'", "''"))'"
}

$candidateSql = @"
WITH candidate_receipts AS (
    SELECT
        receipts.receipt_id,
        receipts.tenant_id,
        receipts.intent_id,
        receipts.job_id,
        COALESCE(jobs.job_json->>'adapter_id', NULL) AS adapter_id,
        COALESCE(NULLIF(receipts.receipt_json->>'recon_subject_id', ''), 'reconsub:' || receipts.job_id) AS recon_subject_id,
        COALESCE(NULLIF(receipts.receipt_json->>'state', ''), 'unknown') AS canonical_state,
        COALESCE(NULLIF(receipts.receipt_json->>'classification', ''), 'unknown') AS classification,
        NULLIF(receipts.receipt_json->>'execution_correlation_id', '') AS execution_correlation_id,
        NULLIF(receipts.receipt_json->>'adapter_execution_reference', '') AS adapter_execution_reference,
        NULLIF(receipts.receipt_json->>'external_observation_key', '') AS external_observation_key,
        CASE
            WHEN jsonb_typeof(receipts.receipt_json->'expected_fact_snapshot') = 'object'
                THEN receipts.receipt_json->'expected_fact_snapshot'
            ELSE NULL
        END AS expected_fact_snapshot_json,
        receipts.occurred_at_ms,
        CASE
            WHEN LOWER(COALESCE(receipts.receipt_json->>'state', '')) = 'succeeded' THEN 'finalized'
            WHEN LOWER(COALESCE(receipts.receipt_json->>'state', '')) IN ('failed_terminal', 'dead_lettered', 'rejected') THEN 'terminal_failure'
            WHEN NULLIF(receipts.receipt_json->>'adapter_execution_reference', '') IS NOT NULL
              OR NULLIF(receipts.receipt_json->>'external_observation_key', '') IS NOT NULL
              THEN 'submitted_with_reference'
            ELSE 'adapter_completed'
        END AS signal_kind
    FROM execution_core_receipts receipts
    LEFT JOIN execution_core_jobs jobs
        ON jobs.job_id = receipts.job_id
    WHERE receipts.occurred_at_ms >= ((EXTRACT(EPOCH FROM NOW()) * 1000)::bigint - ($lookbackHours::bigint * 60 * 60 * 1000))
      AND COALESCE((receipts.receipt_json->>'reconciliation_eligible')::boolean, FALSE)
      $tenantClause
)
SELECT
    signal_kind,
    COUNT(*) AS candidate_count
FROM candidate_receipts
GROUP BY signal_kind
ORDER BY signal_kind
"@

$insertSql = @"
WITH candidate_receipts AS (
    SELECT
        receipts.receipt_id,
        receipts.tenant_id,
        receipts.intent_id,
        receipts.job_id,
        COALESCE(jobs.job_json->>'adapter_id', NULL) AS adapter_id,
        COALESCE(NULLIF(receipts.receipt_json->>'recon_subject_id', ''), 'reconsub:' || receipts.job_id) AS recon_subject_id,
        COALESCE(NULLIF(receipts.receipt_json->>'state', ''), 'unknown') AS canonical_state,
        COALESCE(NULLIF(receipts.receipt_json->>'classification', ''), 'unknown') AS classification,
        NULLIF(receipts.receipt_json->>'execution_correlation_id', '') AS execution_correlation_id,
        NULLIF(receipts.receipt_json->>'adapter_execution_reference', '') AS adapter_execution_reference,
        NULLIF(receipts.receipt_json->>'external_observation_key', '') AS external_observation_key,
        CASE
            WHEN jsonb_typeof(receipts.receipt_json->'expected_fact_snapshot') = 'object'
                THEN receipts.receipt_json->'expected_fact_snapshot'
            ELSE NULL
        END AS expected_fact_snapshot_json,
        receipts.occurred_at_ms,
        CASE
            WHEN LOWER(COALESCE(receipts.receipt_json->>'state', '')) = 'succeeded' THEN 'finalized'
            WHEN LOWER(COALESCE(receipts.receipt_json->>'state', '')) IN ('failed_terminal', 'dead_lettered', 'rejected') THEN 'terminal_failure'
            WHEN NULLIF(receipts.receipt_json->>'adapter_execution_reference', '') IS NOT NULL
              OR NULLIF(receipts.receipt_json->>'external_observation_key', '') IS NOT NULL
              THEN 'submitted_with_reference'
            ELSE 'adapter_completed'
        END AS signal_kind
    FROM execution_core_receipts receipts
    LEFT JOIN execution_core_jobs jobs
        ON jobs.job_id = receipts.job_id
    WHERE receipts.occurred_at_ms >= ((EXTRACT(EPOCH FROM NOW()) * 1000)::bigint - ($lookbackHours::bigint * 60 * 60 * 1000))
      AND COALESCE((receipts.receipt_json->>'reconciliation_eligible')::boolean, FALSE)
      $tenantClause
),
inserted AS (
    INSERT INTO platform_recon_intake_signals (
        signal_id,
        source_system,
        signal_kind,
        tenant_id,
        intent_id,
        job_id,
        adapter_id,
        receipt_id,
        transition_id,
        callback_id,
        recon_subject_id,
        canonical_state,
        classification,
        execution_correlation_id,
        adapter_execution_reference,
        external_observation_key,
        expected_fact_snapshot_json,
        payload_json,
        occurred_at_ms
    )
    SELECT
        'backfill:' || receipt_id || ':' || signal_kind,
        'receipt_backfill',
        signal_kind,
        tenant_id,
        intent_id,
        job_id,
        adapter_id,
        receipt_id,
        NULL,
        NULL,
        recon_subject_id,
        canonical_state,
        classification,
        execution_correlation_id,
        adapter_execution_reference,
        external_observation_key,
        expected_fact_snapshot_json,
        jsonb_build_object(
            'source', 'backfill_reconciliation_intake',
            'receipt_id', receipt_id,
            'signal_kind', signal_kind,
            'lookback_hours', $lookbackHours
        ),
        occurred_at_ms
    FROM candidate_receipts
    ON CONFLICT (signal_id) DO NOTHING
    RETURNING signal_kind
)
SELECT
    signal_kind,
    COUNT(*) AS inserted_count
FROM inserted
GROUP BY signal_kind
ORDER BY signal_kind
"@

Write-Host "Runtime target : $(Resolve-RuntimeTarget)"
Write-Host "Lookback hours : $lookbackHours"
Write-Host "Tenant filter  : $(if ([string]::IsNullOrWhiteSpace($TenantId)) { '<all>' } else { $TenantId })"
Write-Host ""
Write-Host "Candidate receipts:"
$candidateOutput = Invoke-DbQuery $candidateSql
if ([string]::IsNullOrWhiteSpace($candidateOutput)) {
    Write-Host "  none"
} else {
    $candidateOutput.Split([Environment]::NewLine, [System.StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object {
        Write-Host "  $_"
    }
}

if (-not $Apply) {
    Write-Host ""
    Write-Host "Dry-run only. Re-run with -Apply to insert deterministic recon intake signals."
    exit 0
}

Write-Host ""
Write-Host "Applying backfill..."
$insertOutput = Invoke-DbQuery $insertSql
if ([string]::IsNullOrWhiteSpace($insertOutput)) {
    Write-Host "No new signals inserted."
} else {
    $insertOutput.Split([Environment]::NewLine, [System.StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object {
        Write-Host "  $_"
    }
}
