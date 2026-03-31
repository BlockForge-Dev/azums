param(
    [ValidateSet("auto", "k8s", "compose")]
    [string]$Runtime = "auto",
    [string]$BaseUrl = "",
    [string]$Namespace = "azums",
    [string]$ComposeProject = "azums-proof",
    [string]$DbPodLabel = "app=postgres",
    [string]$DbService = "postgres",
    [string]$DbUser = "app",
    [string]$DbName = "azums",
    [int]$DispatchReadyQueueWarn = 5000,
    [int]$DispatchQueueLagWarnSeconds = 120,
    [int]$DispatchExpiredLockWarn = 1,
    [int]$CallbackRetryWarn = 500,
    [int]$CallbackTerminalFailureWarn = 1
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-PortFromBaseUrl([string]$Url) {
    if ([string]::IsNullOrWhiteSpace($Url)) {
        return $null
    }
    $uri = [Uri]$Url
    if (-not $uri.IsDefaultPort) {
        return $uri.Port
    }
    if ($uri.Scheme -eq "https") {
        return 443
    }
    return 80
}

function Resolve-RuntimeTarget {
    if ($Runtime -ne "auto") {
        return $Runtime
    }

    $composeReverseProxy = Get-ComposeServiceContainer -Service "reverse_proxy"
    $basePort = Get-PortFromBaseUrl $BaseUrl
    if ($null -ne $composeReverseProxy -and $null -ne $basePort) {
        foreach ($publishedPort in @($composeReverseProxy.Ports)) {
            if ($publishedPort.PublishedPort -eq $basePort -and $publishedPort.TargetPort -eq 8000) {
                return "compose"
            }
        }
    }

    if ($null -ne $composeReverseProxy -and [string]::IsNullOrWhiteSpace($Namespace)) {
        return "compose"
    }

    return "k8s"
}

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
    $ports = @()
    $portsRaw = [string]$record.Ports
    foreach ($part in $portsRaw.Split(',')) {
        $trimmed = $part.Trim()
        if ($trimmed -match '(?<published>\d+)->(?<target>\d+)/tcp') {
            $ports += [pscustomobject]@{
                PublishedPort = [int]$Matches.published
                TargetPort    = [int]$Matches.target
            }
        }
    }

    [pscustomobject]@{
        Id     = [string]$record.ID
        Image  = [string]$record.Image
        Names  = [string]$record.Names
        Ports  = $ports
        Status = [string]$record.Status
    }
}

function Query-ScalarK8s([string]$Pod, [string]$Sql) {
    $out = & kubectl -n $Namespace exec $Pod -- psql -U $DbUser -d $DbName -t -A -v ON_ERROR_STOP=1 -c $Sql
    if ($LASTEXITCODE -ne 0 -or $null -eq $out) {
        throw "failed to query k8s postgres health SQL; ensure queue schema/migrations are present"
    }
    $last = $out | Select-Object -Last 1
    if ($null -eq $last) { return 0 }
    $line = ([string]$last).Trim()
    if ([string]::IsNullOrWhiteSpace($line)) { return 0 }
    return [double]$line
}

function Query-ScalarCompose([string]$ContainerId, [string]$Sql) {
    $out = & docker exec $ContainerId psql -U $DbUser -d $DbName -t -A -v ON_ERROR_STOP=1 -c $Sql
    if ($LASTEXITCODE -ne 0 -or $null -eq $out) {
        throw "failed to query compose postgres health SQL; if this is a fresh proof volume, recreate it so postgresflow migrations run"
    }
    $last = $out | Select-Object -Last 1
    if ($null -eq $last) { return 0 }
    $line = ([string]$last).Trim()
    if ([string]::IsNullOrWhiteSpace($line)) { return 0 }
    return [double]$line
}

function Test-K8sPodsHealthy {
    $problems = New-Object System.Collections.Generic.List[string]
    $podsJson = & kubectl -n $Namespace get pods -o json | ConvertFrom-Json
    $criticalPods = @("ingress-api", "status-api", "execution-worker", "execution-callback-worker", "reverse-proxy")
    foreach ($item in $podsJson.items) {
        $name = [string]$item.metadata.name
        if (-not ($criticalPods | Where-Object { $name.StartsWith($_) })) {
            continue
        }

        $phase = [string]$item.status.phase
        if ($phase -ne "Running") {
            $problems.Add("$name phase=$phase")
            continue
        }

        foreach ($cs in @($item.status.containerStatuses)) {
            if (-not $cs.ready) {
                $problems.Add("$name not Ready")
            }
            if ($cs.restartCount -gt 3) {
                $problems.Add("$name restartCount=$($cs.restartCount)")
            }

            $waiting = $null
            if ($null -ne $cs.state) {
                $waitingProp = $cs.state.PSObject.Properties["waiting"]
                if ($null -ne $waitingProp) {
                    $waiting = $waitingProp.Value
                }
            }
            if ($null -ne $waiting -and $waiting.reason -eq "CrashLoopBackOff") {
                $problems.Add("$name CrashLoopBackOff")
            }
        }
    }

    return $problems
}

function Test-ComposeServicesHealthy {
    $problems = New-Object System.Collections.Generic.List[string]
    $criticalServices = @(
        "ingress_api",
        "status_api",
        "execution_worker",
        "execution_worker_replica",
        "execution_callback_worker",
        "reverse_proxy"
    )
    foreach ($service in $criticalServices) {
        $container = Get-ComposeServiceContainer -Service $service
        if ($null -eq $container) {
            $problems.Add("$service container not found")
            continue
        }
        if ($container.Status -notmatch '^Up\b') {
            $problems.Add("$service status=$($container.Status)")
        }
    }
    return $problems
}

$resolvedRuntime = Resolve-RuntimeTarget
$problems = New-Object System.Collections.Generic.List[string]

if ($resolvedRuntime -eq "k8s") {
    Write-Host "Checking pod health..."
    foreach ($problem in @(Test-K8sPodsHealthy)) {
        $problems.Add($problem)
    }
    $dbTarget = (& kubectl -n $Namespace get pod -l $DbPodLabel -o jsonpath='{.items[0].metadata.name}').Trim()
    if ([string]::IsNullOrWhiteSpace($dbTarget)) {
        throw "No postgres pod found for label $DbPodLabel in namespace $Namespace"
    }
    $queryScalar = { param($sql) Query-ScalarK8s $dbTarget $sql }
    $targetLabel = "pod/$dbTarget"
} else {
    Write-Host "Checking compose service health..."
    foreach ($problem in @(Test-ComposeServicesHealthy)) {
        $problems.Add($problem)
    }
    $dbContainer = Get-ComposeServiceContainer -Service $DbService
    if ($null -eq $dbContainer) {
        throw "No compose postgres container found for project $ComposeProject service $DbService"
    }
    $queryScalar = { param($sql) Query-ScalarCompose $dbContainer.Id $sql }
    $targetLabel = "container/$($dbContainer.Names)"
}

Write-Host "Checking queue/callback DB health..."
$dispatchReady = [int](& $queryScalar "SELECT COALESCE(COUNT(*),0) FROM jobs WHERE queue='execution.dispatch' AND status='queued' AND run_at <= NOW();")
$dispatchLag = [int](& $queryScalar "SELECT COALESCE(EXTRACT(EPOCH FROM (NOW() - MIN(run_at))),0) FROM jobs WHERE queue='execution.dispatch' AND status='queued' AND run_at <= NOW();")
$dispatchQueuedFuture = [int](& $queryScalar "SELECT COALESCE(COUNT(*),0) FROM jobs WHERE queue='execution.dispatch' AND status='queued' AND run_at > NOW();")
$dispatchNextDelay = [int](& $queryScalar "SELECT COALESCE(EXTRACT(EPOCH FROM (MIN(run_at) - NOW())),0) FROM jobs WHERE queue='execution.dispatch' AND status='queued' AND run_at > NOW();")
$dispatchRunning = [int](& $queryScalar "SELECT COALESCE(COUNT(*),0) FROM jobs WHERE queue='execution.dispatch' AND status='running';")
$dispatchExpiredLocks = [int](& $queryScalar "SELECT COALESCE(COUNT(*),0) FROM jobs WHERE queue='execution.dispatch' AND status='running' AND lock_expires_at IS NOT NULL AND lock_expires_at < NOW();")
$dispatchDlq = [int](& $queryScalar "SELECT COALESCE(COUNT(*),0) FROM jobs WHERE queue='execution.dispatch' AND status='dlq';")
$callbackRetry = [int](& $queryScalar "SELECT CASE WHEN to_regclass('public.callback_core_deliveries') IS NULL THEN 0 ELSE (SELECT COALESCE(COUNT(*),0) FROM callback_core_deliveries WHERE state='retry_scheduled') END;")
$callbackTerminal = [int](& $queryScalar "SELECT CASE WHEN to_regclass('public.callback_core_deliveries') IS NULL THEN 0 ELSE (SELECT COALESCE(COUNT(*),0) FROM callback_core_deliveries WHERE state='terminal_failure') END;")

if ($dispatchReady -gt $DispatchReadyQueueWarn) {
    $problems.Add("dispatch ready queue too high: $dispatchReady > $DispatchReadyQueueWarn")
}
if ($dispatchLag -gt $DispatchQueueLagWarnSeconds) {
    $problems.Add("dispatch lag too high: ${dispatchLag}s > ${DispatchQueueLagWarnSeconds}s")
}
if ($dispatchExpiredLocks -ge $DispatchExpiredLockWarn -and $DispatchExpiredLockWarn -gt 0) {
    $problems.Add("dispatch expired running locks detected: $dispatchExpiredLocks")
}
if ($callbackRetry -gt $CallbackRetryWarn) {
    $problems.Add("callback retry backlog too high: $callbackRetry > $CallbackRetryWarn")
}
if ($callbackTerminal -gt $CallbackTerminalFailureWarn) {
    $problems.Add("callback terminal failures above threshold: $callbackTerminal > $CallbackTerminalFailureWarn")
}

Write-Host ""
Write-Host "=== Platform Health Summary ==="
[PSCustomObject]@{
    Runtime                   = $resolvedRuntime
    Target                    = $targetLabel
    DispatchReadyQueue        = $dispatchReady
    DispatchQueueLagSeconds   = $dispatchLag
    DispatchQueuedFuture      = $dispatchQueuedFuture
    DispatchNextRunDelaySecs  = [Math]::Max($dispatchNextDelay, 0)
    DispatchRunning           = $dispatchRunning
    DispatchExpiredLocks      = $dispatchExpiredLocks
    DispatchDeadLettered      = $dispatchDlq
    CallbackRetryScheduled    = $callbackRetry
    CallbackTerminalFailures  = $callbackTerminal
} | Format-List

if ($problems.Count -gt 0) {
    Write-Host ""
    Write-Host "Health check failed:"
    foreach ($problem in $problems) {
        Write-Host " - $problem"
    }
    exit 2
}

Write-Host "Health check passed."
