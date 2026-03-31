param(
    [string]$BaseUrl = $(if ($env:BASE_URL) { $env:BASE_URL } else { "http://127.0.0.2:8000" }),
    [string]$TenantId = $(if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }),
    [string]$IngressToken = $(if ($env:INGRESS_TOKEN) { $env:INGRESS_TOKEN } else { "dev-ingress-token" }),
    [string]$StatusToken = $(if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }),
    [string]$IngressPrincipalId = $(if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }),
    [string]$StatusPrincipalId = $(if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }),
    [string]$StatusPrincipalRole = $(if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }),
    [string]$ToAddr = $(if ($env:TO_WALLET) { $env:TO_WALLET } else { "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC" }),
    [ValidateSet("synthetic_success", "retry_then_success", "terminal_failure", "rpc_timeout")]
    [string]$Scenario = "synthetic_success",
    [int]$RequestCount = 10,
    [int]$SubmitConcurrency = 4,
    [int]$StatusPollIntervalMs = 1000,
    [int]$TerminalTimeoutSec = 0,
    [string]$CallbackDeliveryUrl = "",
    [switch]$ConfigureCallbackDestination,
    [bool]$AllowPrivateCallbackDestinations = $true,
    [int]$DuplicateGroupSize = 1,
    [string]$OutputJsonPath = "",
    [string]$Namespace = "",
    [ValidateSet("auto", "k8s", "compose")]
    [string]$Runtime = "auto",
    [string]$ComposeProject = "azums-proof",
    [string]$ComposeFile = $(Join-Path $PSScriptRoot "..\\deployments\\docker\\docker-compose.images.yml"),
    [switch]$UseFastFailureProfile,
    [switch]$SkipClusterHealth
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$TransientSubmitRetryAttempts = 6
$TransientSubmitRetryDelayMs = 250
$FastFailureProfile = [ordered]@{
    EXECUTION_RETRY_MAX_ATTEMPTS          = "3"
    EXECUTION_RETRY_BASE_DELAY_MS         = "250"
    EXECUTION_RETRY_MAX_DELAY_MS          = "1500"
    EXECUTION_RETRY_JITTER_PERCENT        = "0"
    EXECUTION_QUEUE_JOB_MAX_ATTEMPTS      = "4"
    EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS = "1"
    EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS  = "3"
}
$ComposeManagedServices = @(
    "ingress_api",
    "status_api",
    "execution_worker",
    "execution_worker_replica",
    "execution_callback_worker",
    "reverse_proxy"
)
$SavedFastFailureEnv = @{}

function Parse-Body([string]$Raw) {
    if ([string]::IsNullOrWhiteSpace($Raw)) {
        return $null
    }
    try {
        return $Raw | ConvertFrom-Json
    }
    catch {
        return $Raw
    }
}

function Get-PortFromBaseUrl([string]$Url) {
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
    if ($null -ne $composeReverseProxy) {
        foreach ($publishedPort in @($composeReverseProxy.Ports)) {
            if ($publishedPort.PublishedPort -eq $basePort -and $publishedPort.TargetPort -eq 8000) {
                return "compose"
            }
        }
    }

    return "k8s"
}

function Get-K8sSecretValue([string]$SecretName, [string]$KeyName) {
    $jsonPath = "jsonpath={.data.$KeyName}"
    $encoded = (& kubectl -n $Namespace get secret $SecretName -o $jsonPath 2>$null | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($encoded)) {
        return $null
    }
    $bytes = [Convert]::FromBase64String($encoded)
    return [System.Text.Encoding]::UTF8.GetString($bytes)
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

function Get-ComposeServicePort {
    param(
        [Parameter(Mandatory = $true)][string]$Service,
        [Parameter(Mandatory = $true)][int]$TargetPort
    )

    $container = Get-ComposeServiceContainer -Service $Service
    if ($null -eq $container) {
        return $null
    }

    $match = @($container.Ports | Where-Object { $_.TargetPort -eq $TargetPort } | Select-Object -First 1)
    if ($match.Count -eq 0) {
        return $null
    }

    return [int]$match[0].PublishedPort
}

function Invoke-JsonRequest {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [hashtable]$Headers,
        [object]$Body
    )

    $invokeArgs = @{
        Method             = $Method
        Uri                = $Url
        ErrorAction        = "Stop"
        SkipHttpErrorCheck = $true
    }
    if ($null -ne $Headers) {
        $invokeArgs.Headers = $Headers
    }
    if ($null -ne $Body) {
        $invokeArgs.ContentType = "application/json"
        $invokeArgs.Body = ($Body | ConvertTo-Json -Depth 16)
    }

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $resp = Invoke-WebRequest @invokeArgs
    $stopwatch.Stop()

    [pscustomobject]@{
        Status    = [int]$resp.StatusCode
        Body      = Parse-Body $resp.Content
        Raw       = $resp.Content
        LatencyMs = [Math]::Round($stopwatch.Elapsed.TotalMilliseconds, 2)
    }
}

function Wait-ForHttpEndpoint {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [int]$TimeoutSec = 60,
        [int]$SleepMs = 500
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $lastError = $null
    while ((Get-Date) -lt $deadline) {
        try {
            $resp = Invoke-WebRequest -Method "GET" -Uri $Url -SkipHttpErrorCheck -ErrorAction Stop
            if ($null -ne $resp -and [int]$resp.StatusCode -ge 200 -and [int]$resp.StatusCode -lt 500) {
                return
            }
            $lastError = "status=$([int]$resp.StatusCode)"
        }
        catch {
            $lastError = [string]$_.Exception.Message
        }
        Start-Sleep -Milliseconds $SleepMs
    }

    throw "timed out waiting for HTTP endpoint $Url ($lastError)"
}

function Wait-ForComposePlatformReady {
    param(
        [Parameter(Mandatory = $true)][string]$HealthBaseUrl,
        [Parameter(Mandatory = $true)][string]$ComposeProjectName,
        [int]$TimeoutSec = 120
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $lastOutput = $null
    while ((Get-Date) -lt $deadline) {
        $args = @(
            "-File", (Join-Path $PSScriptRoot "check_platform_health.ps1"),
            "-Runtime", "compose",
            "-BaseUrl", $HealthBaseUrl,
            "-ComposeProject", $ComposeProjectName
        )
        $output = & pwsh @args 2>&1 | Out-String
        if ($LASTEXITCODE -eq 0) {
            return
        }
        $lastOutput = $output.Trim()
        Start-Sleep -Seconds 2
    }

    throw "timed out waiting for compose platform readiness: $lastOutput"
}

function Invoke-ComposeUp {
    param([string[]]$Services)

    $args = @("-p", $ComposeProject, "-f", $ComposeFile, "up", "-d", "--force-recreate")
    $args += $Services

    $output = & docker compose @args 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        if (-not [string]::IsNullOrWhiteSpace($output)) {
            Write-Host $output.Trim()
        }
        throw "docker compose up failed"
    }
}

function Wait-ForComposeService {
    param(
        [Parameter(Mandatory = $true)][string]$Service,
        [int]$TimeoutSec = 180
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $container = Get-ComposeServiceContainer -Service $Service
        if ($null -ne $container -and $container.Status -match '^Up\b') {
            return
        }
        Start-Sleep -Seconds 2
    }

    throw "timed out waiting for compose service $Service"
}

function Apply-FastFailureProfile {
    if ($resolvedRuntime -eq "compose") {
        foreach ($entry in $FastFailureProfile.GetEnumerator()) {
            $existing = Get-Item -Path "Env:$($entry.Key)" -ErrorAction SilentlyContinue
            $SavedFastFailureEnv[$entry.Key] = if ($null -ne $existing) { [string]$existing.Value } else { $null }
            Set-Item -Path "Env:$($entry.Key)" -Value $entry.Value
        }
        Invoke-ComposeUp -Services $ComposeManagedServices
        foreach ($service in $ComposeManagedServices) {
            Wait-ForComposeService -Service $service
        }
        return
    }

    $retryVars = @(
        "EXECUTION_RETRY_MAX_ATTEMPTS=$($FastFailureProfile.EXECUTION_RETRY_MAX_ATTEMPTS)",
        "EXECUTION_RETRY_BASE_DELAY_MS=$($FastFailureProfile.EXECUTION_RETRY_BASE_DELAY_MS)",
        "EXECUTION_RETRY_MAX_DELAY_MS=$($FastFailureProfile.EXECUTION_RETRY_MAX_DELAY_MS)",
        "EXECUTION_RETRY_JITTER_PERCENT=$($FastFailureProfile.EXECUTION_RETRY_JITTER_PERCENT)"
    )
    $queueVars = @(
        "EXECUTION_QUEUE_JOB_MAX_ATTEMPTS=$($FastFailureProfile.EXECUTION_QUEUE_JOB_MAX_ATTEMPTS)",
        "EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS=$($FastFailureProfile.EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS)",
        "EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS=$($FastFailureProfile.EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS)"
    )

    & kubectl -n $Namespace set env deploy/ingress-api @retryVars
    & kubectl -n $Namespace set env deploy/status-api @retryVars
    & kubectl -n $Namespace set env deploy/execution-worker @retryVars @queueVars
    & kubectl -n $Namespace set env deploy/execution-callback-worker @queueVars
    if ($LASTEXITCODE -ne 0) {
        throw "failed to apply fast-failure profile to k8s"
    }
    & kubectl -n $Namespace rollout status deploy/ingress-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/status-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/execution-worker --timeout=300s
    & kubectl -n $Namespace rollout status deploy/execution-callback-worker --timeout=300s
}

function Restore-FastFailureProfile {
    if ($resolvedRuntime -eq "compose") {
        foreach ($entry in $FastFailureProfile.GetEnumerator()) {
            $name = $entry.Key
            $savedValue = $SavedFastFailureEnv[$name]
            if ($null -eq $savedValue) {
                Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue
            } else {
                Set-Item -Path "Env:$name" -Value $savedValue
            }
        }
        Invoke-ComposeUp -Services $ComposeManagedServices
        return
    }

    & kubectl -n $Namespace set env deploy/ingress-api EXECUTION_RETRY_MAX_ATTEMPTS- EXECUTION_RETRY_BASE_DELAY_MS- EXECUTION_RETRY_MAX_DELAY_MS- EXECUTION_RETRY_JITTER_PERCENT-
    & kubectl -n $Namespace set env deploy/status-api EXECUTION_RETRY_MAX_ATTEMPTS- EXECUTION_RETRY_BASE_DELAY_MS- EXECUTION_RETRY_MAX_DELAY_MS- EXECUTION_RETRY_JITTER_PERCENT-
    & kubectl -n $Namespace set env deploy/execution-worker EXECUTION_RETRY_MAX_ATTEMPTS- EXECUTION_RETRY_BASE_DELAY_MS- EXECUTION_RETRY_MAX_DELAY_MS- EXECUTION_RETRY_JITTER_PERCENT- EXECUTION_QUEUE_JOB_MAX_ATTEMPTS- EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS- EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS-
    & kubectl -n $Namespace set env deploy/execution-callback-worker EXECUTION_QUEUE_JOB_MAX_ATTEMPTS- EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS- EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS-
    & kubectl -n $Namespace rollout status deploy/ingress-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/status-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/execution-worker --timeout=300s
    & kubectl -n $Namespace rollout status deploy/execution-callback-worker --timeout=300s
}

function Test-IsTransientSubmitException {
    param([System.Exception]$Exception)

    $cursor = $Exception
    while ($null -ne $cursor) {
        $message = [string]$cursor.Message
        if (
            $message -match 'ResponseEnded' -or
            $message -match 'response ended prematurely' -or
            $message -match 'An error occurred while sending the request' -or
            $message -match 'actively refused' -or
            $message -match 'No connection could be made' -or
            $message -match 'Unable to connect' -or
            $message -match 'forcibly closed' -or
            $message -match 'connection was closed' -or
            $message -match 'connection reset' -or
            $message -match 'unexpected EOF'
        ) {
            return $true
        }
        $cursor = $cursor.InnerException
    }

    return $false
}

function Invoke-SubmitRequest {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][hashtable]$Headers,
        [Parameter(Mandatory = $true)][string]$BodyJson
    )

    for ($attempt = 1; $attempt -le $TransientSubmitRetryAttempts; $attempt++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $resp = Invoke-WebRequest -Method "POST" -Uri $Url -Headers $Headers -ContentType "application/json" -Body $BodyJson -SkipHttpErrorCheck -ErrorAction Stop
            $sw.Stop()
            return [pscustomobject]@{
                StatusCode = [int]$resp.StatusCode
                Content    = [string]$resp.Content
                LatencyMs  = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
            }
        }
        catch {
            $sw.Stop()
            if ($attempt -ge $TransientSubmitRetryAttempts -or -not (Test-IsTransientSubmitException -Exception $_.Exception)) {
                throw
            }
            Start-Sleep -Milliseconds ($TransientSubmitRetryDelayMs * $attempt)
        }
    }

    throw "submit request retry loop exited unexpectedly"
}

function Get-TerminalState([string]$State) {
    return $State -in @("succeeded", "failed_terminal", "dead_lettered", "finalized")
}

function Get-FirstEntryTime($Entries, [string]$State) {
    $match = @($Entries | Where-Object { $_.state -eq $State } | Sort-Object occurred_at_ms | Select-Object -First 1)
    if ($match.Count -eq 0) { return $null }
    return [double]$match[0].occurred_at_ms
}

function Get-LastEntryTime($Entries, [string]$State) {
    $match = @($Entries | Where-Object { $_.state -eq $State } | Sort-Object occurred_at_ms -Descending | Select-Object -First 1)
    if ($match.Count -eq 0) { return $null }
    return [double]$match[0].occurred_at_ms
}

function Get-FinalEntry($Entries) {
    $match = @($Entries | Sort-Object occurred_at_ms -Descending | Select-Object -First 1)
    if ($match.Count -eq 0) { return $null }
    return $match[0]
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    $filtered = @($Values | Where-Object { $null -ne $_ } | Sort-Object)
    if ($filtered.Count -eq 0) {
        return $null
    }
    $rank = [Math]::Ceiling(($Percentile / 100.0) * $filtered.Count) - 1
    if ($rank -lt 0) { $rank = 0 }
    if ($rank -ge $filtered.Count) { $rank = $filtered.Count - 1 }
    return [Math]::Round([double]$filtered[$rank], 2)
}

function Get-Average([double[]]$Values) {
    $filtered = @($Values | Where-Object { $null -ne $_ })
    if ($filtered.Count -eq 0) {
        return $null
    }
    return [Math]::Round((($filtered | Measure-Object -Average).Average), 2)
}

function Get-ObjectPropertyValue($Object, [string]$PropertyName) {
    if ($null -eq $Object) {
        return $null
    }
    $property = $Object.PSObject.Properties[$PropertyName]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Get-PropertyValues($Items, [string]$PropertyName) {
    $values = @()
    foreach ($item in @($Items)) {
        if ($null -eq $item) {
            continue
        }
        $prop = $item.PSObject.Properties[$PropertyName]
        if ($null -eq $prop) {
            continue
        }
        $values += $prop.Value
    }
    return @($values)
}

function Get-PropertyValuesAsStrings($Items, [string]$PropertyName) {
    $values = @()
    foreach ($item in @($Items)) {
        $value = Get-ObjectPropertyValue $item $PropertyName
        if ($null -eq $value) {
            continue
        }
        $text = [string]$value
        if ([string]::IsNullOrWhiteSpace($text)) {
            continue
        }
        $values += $text
    }
    return $values
}

function Get-StateCountTable($Items, [string]$PropertyName) {
    return @($Items |
        Group-Object -Property $PropertyName |
        Sort-Object Name |
        ForEach-Object {
            [pscustomobject]@{
                Name  = if ([string]::IsNullOrWhiteSpace([string]$_.Name)) { "<empty>" } else { [string]$_.Name }
                Count = $_.Count
            }
        })
}

function Get-ScenarioTerminalTimeoutSec([string]$SelectedScenario) {
    switch ($SelectedScenario) {
        "synthetic_success" { return 300 }
        "retry_then_success" { return 300 }
        "terminal_failure" { return 180 }
        "rpc_timeout" { return 420 }
        default { return 300 }
    }
}

function New-BenchmarkIntent([int]$Index, [string]$RunTag) {
    $intentId = "intent_bench_${Scenario}_${RunTag}_{0}" -f $Index.ToString("0000")
    $payload = [ordered]@{
        intent_id = $intentId
        type      = "transfer"
        to_addr   = $ToAddr
        amount    = 1
    }
    $metadata = [ordered]@{}

    switch ($Scenario) {
        "synthetic_success" {
            $metadata["metering.scope"] = "playground"
            $metadata["ui.surface"] = "playground"
            $metadata["playground.demo_scenario"] = "success"
        }
        "retry_then_success" {
            $metadata["metering.scope"] = "playground"
            $metadata["ui.surface"] = "playground"
            $metadata["playground.demo_scenario"] = "retry_then_success"
        }
        "terminal_failure" {
            $metadata["metering.scope"] = "playground"
            $metadata["ui.surface"] = "playground"
            $metadata["playground.demo_scenario"] = "terminal_failure"
        }
        "rpc_timeout" {
            $payload["rpc_url"] = "https://127.0.0.1:1"
            $metadata["metering.scope"] = "playground"
            $metadata["ui.surface"] = "playground"
        }
    }

    [pscustomobject]@{
        IntentId = $intentId
        Payload  = $payload
        Metadata = $metadata
    }
}

function Get-IdempotencyKey([int]$Index, [string]$RunTag) {
    if ($DuplicateGroupSize -le 1) {
        return $null
    }
    $group = [Math]::Floor(($Index - 1) / $DuplicateGroupSize)
    return "idem-bench-${Scenario}-${RunTag}-${group}"
}

function Invoke-SubmitSequential {
    param(
        [object[]]$Requests,
        [hashtable]$Headers,
        [string]$Base
    )

    $results = @()
    foreach ($request in $Requests) {
        $submitHeaders = @{}
        foreach ($pair in $Headers.GetEnumerator()) {
            $submitHeaders[$pair.Key] = $pair.Value
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$request.IdempotencyKey)) {
            $submitHeaders["x-idempotency-key"] = [string]$request.IdempotencyKey
        }

        $body = @{
            intent_kind = "solana.transfer.v1"
            payload     = $request.Payload
        }
        if ($request.Metadata.Count -gt 0) {
            $body.metadata = $request.Metadata
        }

        $bodyJson = $body | ConvertTo-Json -Depth 16
        $resp = $null
        $parsed = $null
        $responseRaw = ""
        $submitStatus = 599
        $submitLatencyMs = 0
        try {
            $resp = Invoke-SubmitRequest -Url "$Base/api/requests" -Headers $submitHeaders -BodyJson $bodyJson
            $parsed = Parse-Body $resp.Content
            $responseRaw = $resp.Content
            $submitStatus = $resp.StatusCode
            $submitLatencyMs = $resp.LatencyMs
        }
        catch {
            $responseRaw = [string]$_.Exception.Message
        }
        $results += [pscustomobject]@{
            Index           = $request.Index
            DuplicateGroup  = $request.DuplicateGroup
            IntentId        = $request.IntentId
            IdempotencyKey  = $request.IdempotencyKey
            SubmitStatus    = $submitStatus
            SubmitLatencyMs = $submitLatencyMs
            ResponseBody    = $parsed
            ResponseRaw     = $responseRaw
            Accepted        = ($submitStatus -ge 200 -and $submitStatus -lt 300)
            AcceptedIntentId = if ($null -ne (Get-ObjectPropertyValue $parsed "intent_id")) { [string](Get-ObjectPropertyValue $parsed "intent_id") } else { $request.IntentId }
            JobId           = if ($null -ne (Get-ObjectPropertyValue $parsed "job_id")) { [string](Get-ObjectPropertyValue $parsed "job_id") } else { "" }
        }
    }
    return $results
}

function Invoke-SubmitConcurrent {
    param(
        [object[]]$Requests,
        [hashtable]$Headers,
        [string]$Base
    )

    $threadJob = Get-Command Start-ThreadJob -ErrorAction SilentlyContinue
    if ($resolvedRuntime -eq "compose" -or $null -eq $threadJob -or $SubmitConcurrency -le 1) {
        return Invoke-SubmitSequential -Requests $Requests -Headers $Headers -Base $Base
    }

    $jobs = @()
    foreach ($request in $Requests) {
        $submitHeaders = @{}
        foreach ($pair in $Headers.GetEnumerator()) {
            $submitHeaders[$pair.Key] = $pair.Value
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$request.IdempotencyKey)) {
            $submitHeaders["x-idempotency-key"] = [string]$request.IdempotencyKey
        }
        $body = @{
            intent_kind = "solana.transfer.v1"
            payload     = $request.Payload
        }
        if ($request.Metadata.Count -gt 0) {
            $body.metadata = $request.Metadata
        }
        $bodyJson = $body | ConvertTo-Json -Depth 16

        $jobs += Start-ThreadJob -ThrottleLimit $SubmitConcurrency -ScriptBlock {
            param($BaseUrl, $Headers, $BodyJson, $Index, $DuplicateGroup, $IntentId, $IdempotencyKey)

            $transientSubmitRetryAttempts = 6
            $transientSubmitRetryDelayMs = 250

            function Test-IsTransientSubmitExceptionLocal {
                param([System.Exception]$Exception)

                $cursor = $Exception
                while ($null -ne $cursor) {
                    $message = [string]$cursor.Message
                    if (
                        $message -match 'ResponseEnded' -or
                        $message -match 'response ended prematurely' -or
                        $message -match 'An error occurred while sending the request' -or
                        $message -match 'actively refused' -or
                        $message -match 'No connection could be made' -or
                        $message -match 'Unable to connect' -or
                        $message -match 'forcibly closed' -or
                        $message -match 'connection was closed' -or
                        $message -match 'connection reset' -or
                        $message -match 'unexpected EOF'
                    ) {
                        return $true
                    }
                    $cursor = $cursor.InnerException
                }

                return $false
            }

            $result = [ordered]@{
                Index            = $Index
                DuplicateGroup   = $DuplicateGroup
                IntentId         = $IntentId
                IdempotencyKey   = $IdempotencyKey
                SubmitStatus     = 0
                SubmitLatencyMs  = 0
                ResponseBody     = $null
                ResponseRaw      = ""
                Accepted         = $false
                AcceptedIntentId = $IntentId
                JobId            = ""
            }

            try {
                $resp = $null
                $latencyMs = 0
                for ($attempt = 1; $attempt -le $transientSubmitRetryAttempts; $attempt++) {
                    $sw = [System.Diagnostics.Stopwatch]::StartNew()
                    try {
                        $resp = Invoke-WebRequest -Method "POST" -Uri "$BaseUrl/api/requests" -Headers $Headers -ContentType "application/json" -Body $BodyJson -SkipHttpErrorCheck -ErrorAction Stop
                        $sw.Stop()
                        $latencyMs = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
                        break
                    }
                    catch {
                        $sw.Stop()
                        if ($attempt -ge $transientSubmitRetryAttempts -or -not (Test-IsTransientSubmitExceptionLocal -Exception $_.Exception)) {
                            throw
                        }
                        Start-Sleep -Milliseconds ($transientSubmitRetryDelayMs * $attempt)
                    }
                }

                $parsed = $null
                try {
                    $parsed = $resp.Content | ConvertFrom-Json
                }
                catch {
                    $parsed = $null
                }

                $parsedIntentId = $null
                $parsedJobId = $null
                if ($null -ne $parsed) {
                    $intentProp = $parsed.PSObject.Properties["intent_id"]
                    if ($null -ne $intentProp) {
                        $parsedIntentId = $intentProp.Value
                    }
                    $jobProp = $parsed.PSObject.Properties["job_id"]
                    if ($null -ne $jobProp) {
                        $parsedJobId = $jobProp.Value
                    }
                }

                $result.SubmitStatus = [int]$resp.StatusCode
                $result.SubmitLatencyMs = $latencyMs
                $result.ResponseBody = $parsed
                $result.ResponseRaw = $resp.Content
                $result.Accepted = ($result.SubmitStatus -ge 200 -and $result.SubmitStatus -lt 300)
                if ($null -ne $parsedIntentId) {
                    $result.AcceptedIntentId = [string]$parsedIntentId
                }
                if ($null -ne $parsedJobId) {
                    $result.JobId = [string]$parsedJobId
                }
            }
            catch {
                $result.SubmitStatus = 599
                $result.SubmitLatencyMs = 0
                $result.ResponseBody = $null
                $result.ResponseRaw = [string]$_.Exception.Message
                $result.Accepted = $false
            }

            [pscustomobject]$result
        } -ArgumentList $Base, $submitHeaders, $bodyJson, $request.Index, $request.DuplicateGroup, $request.IntentId, $request.IdempotencyKey
    }

    $results = @()
    if ($jobs.Count -gt 0) {
        Wait-Job -Job $jobs | Out-Null
        $results = @($jobs | Receive-Job)
        $jobs | Remove-Job -Force | Out-Null
    }
    return $results | Sort-Object Index
}

function Invoke-ClusterHealth([string]$Phase) {
    if ($SkipClusterHealth) {
        return $null
    }
    try {
        $args = @(
            "-File", (Join-Path $PSScriptRoot "check_platform_health.ps1"),
            "-Runtime", $resolvedRuntime,
            "-BaseUrl", $base,
            "-ComposeProject", $ComposeProject
        )
        if (-not [string]::IsNullOrWhiteSpace($Namespace)) {
            $args += @("-Namespace", $Namespace)
        }
        $output = & pwsh @args 2>&1 | Out-String
        [pscustomobject]@{
            Phase      = $Phase
            ExitCode   = $LASTEXITCODE
            OutputText = $output.Trim()
        }
    }
    catch {
        [pscustomobject]@{
            Phase      = $Phase
            ExitCode   = -1
            OutputText = $_.Exception.Message
        }
    }
}

function Get-RequestSnapshot([string]$IntentId) {
    $statusResp = Invoke-JsonRequest -Method "GET" -Url "$statusBase/requests/$IntentId" -Headers $statusHeaders
    if ($statusResp.Status -lt 200 -or $statusResp.Status -ge 300 -or $null -eq $statusResp.Body) {
        return $null
    }
    $receiptResp = Invoke-JsonRequest -Method "GET" -Url "$statusBase/requests/$IntentId/receipt" -Headers $statusHeaders
    $callbacksResp = Invoke-JsonRequest -Method "GET" -Url "$statusBase/requests/$IntentId/callbacks?include_attempts=true&attempt_limit=50" -Headers $statusHeaders
    [pscustomobject]@{
        Status    = $statusResp.Body
        Receipt   = $receiptResp.Body
        Callbacks = $callbacksResp.Body
    }
}

if ($RequestCount -lt 1) {
    throw "RequestCount must be >= 1."
}
if ($SubmitConcurrency -lt 1) {
    throw "SubmitConcurrency must be >= 1."
}
if ($DuplicateGroupSize -lt 1) {
    throw "DuplicateGroupSize must be >= 1."
}
if ($TerminalTimeoutSec -le 0) {
    $TerminalTimeoutSec = Get-ScenarioTerminalTimeoutSec -SelectedScenario $Scenario
    if ($UseFastFailureProfile -and $Scenario -eq "rpc_timeout") {
        $TerminalTimeoutSec = 180
    }
}

$resolvedRuntime = Resolve-RuntimeTarget
$base = $BaseUrl.TrimEnd("/")
if ($resolvedRuntime -eq "k8s" -and -not [string]::IsNullOrWhiteSpace($Namespace)) {
    if ([string]::IsNullOrWhiteSpace($IngressToken) -or $IngressToken -eq "dev-ingress-token") {
        $resolvedIngressToken = Get-K8sSecretValue -SecretName "azums-platform-secrets" -KeyName "INGRESS_BEARER_TOKEN"
        if (-not [string]::IsNullOrWhiteSpace($resolvedIngressToken)) {
            $IngressToken = $resolvedIngressToken
        }
    }
    if ([string]::IsNullOrWhiteSpace($StatusToken) -or $StatusToken -eq "dev-status-token") {
        $resolvedStatusToken = Get-K8sSecretValue -SecretName "azums-platform-secrets" -KeyName "STATUS_API_BEARER_TOKEN"
        if (-not [string]::IsNullOrWhiteSpace($resolvedStatusToken)) {
            $StatusToken = $resolvedStatusToken
        }
    }
}
$fastFailureProfileApplied = $false
if ($UseFastFailureProfile) {
    Apply-FastFailureProfile
    $fastFailureProfileApplied = $true
}
try {
$ingressBase = $base
$statusBase = $base
if ($resolvedRuntime -eq "compose") {
    $composeIngressPort = Get-ComposeServicePort -Service "ingress_api" -TargetPort 8081
    $composeStatusPort = Get-ComposeServicePort -Service "status_api" -TargetPort 8082
    if ($null -eq $composeIngressPort) {
        throw "compose ingress_api published port for 8081 could not be resolved"
    }
    if ($null -eq $composeStatusPort) {
        throw "compose status_api published port for 8082 could not be resolved"
    }
    $ingressBase = "http://127.0.0.1:$composeIngressPort"
    $statusBase = "http://127.0.0.1:$composeStatusPort"
    Wait-ForHttpEndpoint -Url "$ingressBase/health" -TimeoutSec 90
    Wait-ForHttpEndpoint -Url "$statusBase/health" -TimeoutSec 90
    Wait-ForComposePlatformReady -HealthBaseUrl $base -ComposeProjectName $ComposeProject -TimeoutSec 120
}
$ingHeaders = @{
    authorization      = "Bearer $IngressToken"
    "x-tenant-id"      = $TenantId
    "x-principal-id"   = $IngressPrincipalId
    "x-submitter-kind" = "internal_service"
}
$statusHeaders = @{
    authorization      = "Bearer $StatusToken"
    "x-tenant-id"      = $TenantId
    "x-principal-id"   = $StatusPrincipalId
    "x-principal-role" = $StatusPrincipalRole
}

$runTag = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds().ToString()
$preHealth = Invoke-ClusterHealth -Phase "before"

Write-Host "Benchmark base url              : $base"
if ($resolvedRuntime -eq "compose") {
    Write-Host "Compose ingress base url        : $ingressBase"
    Write-Host "Compose status base url         : $statusBase"
}
Write-Host "Tenant                          : $TenantId"
Write-Host "Scenario                        : $Scenario"
Write-Host "Request count                   : $RequestCount"
Write-Host "Submit concurrency              : $SubmitConcurrency"
Write-Host "Duplicate group size            : $DuplicateGroupSize"
Write-Host "Terminal timeout seconds        : $TerminalTimeoutSec"
Write-Host "Fast failure profile            : $UseFastFailureProfile"
Write-Host "Callback configured             : $ConfigureCallbackDestination"
Write-Host "Callback delivery url           : $CallbackDeliveryUrl"
Write-Host

$health = Invoke-JsonRequest -Method "GET" -Url "$base/healthz"
$ready = Invoke-JsonRequest -Method "GET" -Url "$base/readyz"

if ($ConfigureCallbackDestination) {
    if ([string]::IsNullOrWhiteSpace($CallbackDeliveryUrl)) {
        throw "CallbackDeliveryUrl is required when -ConfigureCallbackDestination is used."
    }

    $callbackBody = @{
        delivery_url               = $CallbackDeliveryUrl
        allow_private_destinations = $AllowPrivateCallbackDestinations
        timeout_ms                 = 3000
        enabled                    = $true
    }
    $callbackResp = Invoke-JsonRequest -Method "POST" -Url "$statusBase/tenant/callback-destination" -Headers $statusHeaders -Body $callbackBody
    if ($callbackResp.Status -lt 200 -or $callbackResp.Status -ge 300) {
        $hint = ""
        if ($callbackResp.Status -eq 403) {
            $hint = " Use a tenant id allowed by the status principal bindings, e.g. `tenant_demo` or a `tenant_ws_*` tenant for the default local/dev config."
        }
        throw "Callback destination setup failed (status=$($callbackResp.Status)): $($callbackResp.Raw)$hint"
    }
}

$requestSpecs = @()
for ($i = 1; $i -le $RequestCount; $i++) {
    $groupIndex = [Math]::Floor(($i - 1) / [Math]::Max($DuplicateGroupSize, 1)) + 1
    $intent = New-BenchmarkIntent -Index $groupIndex -RunTag $runTag
    $requestSpecs += [pscustomobject]@{
        Index          = $i
        DuplicateGroup = $groupIndex
        IntentId       = $intent.IntentId
        Payload        = $intent.Payload
        Metadata       = $intent.Metadata
        IdempotencyKey = Get-IdempotencyKey -Index $i -RunTag $runTag
    }
}

$wallClock = [System.Diagnostics.Stopwatch]::StartNew()
$submitResults = Invoke-SubmitConcurrent -Requests $requestSpecs -Headers $ingHeaders -Base $ingressBase
$accepted = @($submitResults | Where-Object { $_.Accepted })
$pendingIntentIds = New-Object System.Collections.Generic.HashSet[string]
foreach ($row in $accepted) {
    $null = $pendingIntentIds.Add([string]$row.AcceptedIntentId)
}

$latestResponses = @{}
$deadline = (Get-Date).AddSeconds($TerminalTimeoutSec)
while ($pendingIntentIds.Count -gt 0 -and (Get-Date) -lt $deadline) {
    foreach ($intentId in @($pendingIntentIds)) {
        $snapshot = Get-RequestSnapshot -IntentId $intentId
        if ($null -eq $snapshot) {
            continue
        }
        $latestResponses[$intentId] = $snapshot
        if (Get-TerminalState ([string]$snapshot.Status.state)) {
            $pendingIntentIds.Remove($intentId) | Out-Null
        }
    }

    if ($pendingIntentIds.Count -gt 0) {
        Start-Sleep -Milliseconds $StatusPollIntervalMs
    }
}
$wallClock.Stop()

foreach ($intentId in @($pendingIntentIds)) {
    $snapshot = Get-RequestSnapshot -IntentId $intentId
    if ($null -ne $snapshot) {
        $latestResponses[$intentId] = $snapshot
    }
}

$postHealth = Invoke-ClusterHealth -Phase "after"

$benchRows = @()
foreach ($submit in $submitResults) {
    $terminal = $null
    if ($latestResponses.ContainsKey([string]$submit.AcceptedIntentId)) {
        $terminal = $latestResponses[[string]$submit.AcceptedIntentId]
    }

    $receiptEntries = @()
    $callbacks = @()
    $callbackState = ""
    $callbackAttempts = 0
    $callbackDeliveredAtMs = $null
    $statusState = ""
    $classification = ""
    $attemptCount = 0
    $finalAtMs = $null
    $receivedAtMs = $null
    $queuedAtMs = $null
    $leasedAtMs = $null
    $firstExecutingAtMs = $null
    $lastExecutingAtMs = $null
    $retryScheduledCount = 0
    $finalSummary = ""

    if ($null -ne $terminal) {
        $statusState = [string]$terminal.Status.state
        $classification = [string]$terminal.Status.classification
        $receiptEntries = @($terminal.Receipt.entries)
        $callbacks = @($terminal.Callbacks.callbacks)
        $callbackState = if ($callbacks.Count -gt 0) { [string]$callbacks[0].state } else { "" }
        $callbackAttempts = if ($callbacks.Count -gt 0) { [int]$callbacks[0].attempts } else { 0 }
        $callbackDeliveredAtMs = if ($callbacks.Count -gt 0 -and $null -ne $callbacks[0].delivered_at_ms) { [double]$callbacks[0].delivered_at_ms } else { $null }
        $receivedAtMs = Get-FirstEntryTime $receiptEntries "received"
        $queuedAtMs = Get-FirstEntryTime $receiptEntries "queued"
        $leasedAtMs = Get-FirstEntryTime $receiptEntries "leased"
        $firstExecutingAtMs = Get-FirstEntryTime $receiptEntries "executing"
        $lastExecutingAtMs = Get-LastEntryTime $receiptEntries "executing"
        $retryScheduledCount = @($receiptEntries | Where-Object { $_.state -eq "retry_scheduled" }).Count
        $attemptCount = [int]((@($receiptEntries | Measure-Object -Property attempt_no -Maximum).Maximum))
        $finalEntry = Get-FinalEntry $receiptEntries
        if ($null -ne $finalEntry) {
            $finalAtMs = [double]$finalEntry.occurred_at_ms
            $finalSummary = [string]$finalEntry.summary
        }
    }

    $benchRows += [pscustomobject]@{
        Index                        = $submit.Index
        DuplicateGroup               = $submit.DuplicateGroup
        IntentId                     = $submit.AcceptedIntentId
        IdempotencyKey               = $submit.IdempotencyKey
        SubmitStatus                 = $submit.SubmitStatus
        SubmitAccepted               = $submit.Accepted
        SubmitLatencyMs              = $submit.SubmitLatencyMs
        SubmitResponseRaw            = $submit.ResponseRaw
        FinalState                   = $statusState
        Classification               = $classification
        AttemptCount                 = $attemptCount
        RetryScheduledCount          = $retryScheduledCount
        CallbackState                = $callbackState
        CallbackAttempts             = $callbackAttempts
        FinalSummary                 = $finalSummary
        AcceptanceToQueuedMs         = if ($null -ne $receivedAtMs -and $null -ne $queuedAtMs) { [Math]::Round($queuedAtMs - $receivedAtMs, 2) } else { $null }
        WorkerPickupDelayMs          = if ($null -ne $queuedAtMs -and $null -ne $leasedAtMs) { [Math]::Round($leasedAtMs - $queuedAtMs, 2) } else { $null }
        FirstExecuteToFinalMs        = if ($null -ne $firstExecutingAtMs -and $null -ne $finalAtMs) { [Math]::Round($finalAtMs - $firstExecutingAtMs, 2) } else { $null }
        LastExecuteToFinalMs         = if ($null -ne $lastExecutingAtMs -and $null -ne $finalAtMs) { [Math]::Round($finalAtMs - $lastExecutingAtMs, 2) } else { $null }
        AcceptedToFinalMs            = if ($null -ne $receivedAtMs -and $null -ne $finalAtMs) { [Math]::Round($finalAtMs - $receivedAtMs, 2) } else { $null }
        FinalToCallbackDeliveredMs   = if ($null -ne $finalAtMs -and $null -ne $callbackDeliveredAtMs) { [Math]::Round($callbackDeliveredAtMs - $finalAtMs, 2) } else { $null }
        AcceptedToCallbackDeliveredMs = if ($null -ne $receivedAtMs -and $null -ne $callbackDeliveredAtMs) { [Math]::Round($callbackDeliveredAtMs - $receivedAtMs, 2) } else { $null }
    }
}

$acceptedRows = @($benchRows | Where-Object { $_.SubmitAccepted })
$terminalRows = @($benchRows | Where-Object { Get-TerminalState ([string]$_.FinalState) })
$uniqueExecutionRows = @(
    $acceptedRows |
        Group-Object -Property IntentId |
        ForEach-Object { $_.Group | Select-Object -First 1 }
)
$duplicateGroupRows = @()
if ($DuplicateGroupSize -gt 1) {
    $duplicateGroupRows = @(
        $benchRows |
            Group-Object -Property IdempotencyKey |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_.Name) } |
            Sort-Object Name |
            ForEach-Object {
                $groupRows = @($_.Group)
                $acceptedGroupRows = @($groupRows | Where-Object { $_.SubmitAccepted })
                $uniqueAcceptedIntentIds = @(Get-PropertyValuesAsStrings $acceptedGroupRows "IntentId" | Select-Object -Unique)
                $uniqueJobStates = @(Get-PropertyValuesAsStrings $acceptedGroupRows "FinalState" | Select-Object -Unique)
                [pscustomobject]@{
                    IdempotencyKey            = $_.Name
                    RequestCount              = $groupRows.Count
                    UniqueAcceptedIntentCount = $uniqueAcceptedIntentIds.Count
                    UniqueFinalStateCount     = $uniqueJobStates.Count
                    AcceptedIntentId          = if ($uniqueAcceptedIntentIds.Count -eq 1) { [string]$uniqueAcceptedIntentIds[0] } else { "" }
                    FinalState                = if ($uniqueAcceptedIntentIds.Count -eq 1 -and $uniqueJobStates.Count -eq 1) { [string]$uniqueJobStates[0] } else { "" }
                    StableOutcome             = ($uniqueAcceptedIntentIds.Count -le 1)
                }
            }
    )
}
$finalStateRows = @(
    foreach ($row in $acceptedRows) {
        [pscustomobject]@{
            FinalStateDisplay = if ([string]::IsNullOrWhiteSpace($row.FinalState)) { "<pending>" } else { $row.FinalState }
        }
    }
)
$throughputRps = if ($wallClock.Elapsed.TotalSeconds -gt 0) {
    [Math]::Round(($acceptedRows.Count / $wallClock.Elapsed.TotalSeconds), 2)
} else {
    $null
}

$summary = [pscustomobject]@{
    Scenario                           = $Scenario
    RequestCount                       = $RequestCount
    AcceptedCount                      = $acceptedRows.Count
    UniqueAcceptedExecutionCount       = $uniqueExecutionRows.Count
    RejectedCount                      = @($benchRows | Where-Object { -not $_.SubmitAccepted }).Count
    TerminalCount                      = $terminalRows.Count
    UniqueTerminalExecutionCount       = @($uniqueExecutionRows | Where-Object { -not [string]::IsNullOrWhiteSpace($_.FinalState) }).Count
    PendingCount                       = $acceptedRows.Count - $terminalRows.Count
    CallbackDeliveredCount             = @($terminalRows | Where-Object { $_.CallbackState -eq "delivered" }).Count
    DuplicateGroupCount                = $duplicateGroupRows.Count
    DuplicateStableGroupCount          = @($duplicateGroupRows | Where-Object { $_.StableOutcome }).Count
    DuplicateMultiExecutionGroupCount  = @($duplicateGroupRows | Where-Object { $_.UniqueAcceptedIntentCount -gt 1 }).Count
    WallClockMs                        = [Math]::Round($wallClock.Elapsed.TotalMilliseconds, 2)
    ThroughputRequestsPerSecond        = $throughputRps
    AcceptanceLatencyP50Ms            = Get-Percentile (Get-PropertyValues $benchRows "SubmitLatencyMs") 50
    AcceptanceLatencyP95Ms            = Get-Percentile (Get-PropertyValues $benchRows "SubmitLatencyMs") 95
    AcceptanceLatencyP99Ms            = Get-Percentile (Get-PropertyValues $benchRows "SubmitLatencyMs") 99
    QueueEnqueueLatencyP50Ms          = Get-Percentile (Get-PropertyValues $acceptedRows "AcceptanceToQueuedMs") 50
    QueueEnqueueLatencyP95Ms          = Get-Percentile (Get-PropertyValues $acceptedRows "AcceptanceToQueuedMs") 95
    WorkerPickupDelayP50Ms            = Get-Percentile (Get-PropertyValues $acceptedRows "WorkerPickupDelayMs") 50
    WorkerPickupDelayP95Ms            = Get-Percentile (Get-PropertyValues $acceptedRows "WorkerPickupDelayMs") 95
    AdapterExecutionWindowP50Ms       = Get-Percentile (Get-PropertyValues $acceptedRows "FirstExecuteToFinalMs") 50
    AdapterExecutionWindowP95Ms       = Get-Percentile (Get-PropertyValues $acceptedRows "FirstExecuteToFinalMs") 95
    ReceiptFinalizationWindowP50Ms    = Get-Percentile (Get-PropertyValues $acceptedRows "LastExecuteToFinalMs") 50
    ReceiptFinalizationWindowP95Ms    = Get-Percentile (Get-PropertyValues $acceptedRows "LastExecuteToFinalMs") 95
    AcceptedToFinalP50Ms              = Get-Percentile (Get-PropertyValues $acceptedRows "AcceptedToFinalMs") 50
    AcceptedToFinalP95Ms              = Get-Percentile (Get-PropertyValues $acceptedRows "AcceptedToFinalMs") 95
    FinalToCallbackP50Ms              = Get-Percentile (Get-PropertyValues $acceptedRows "FinalToCallbackDeliveredMs") 50
    FinalToCallbackP95Ms              = Get-Percentile (Get-PropertyValues $acceptedRows "FinalToCallbackDeliveredMs") 95
    AcceptedToCallbackP50Ms           = Get-Percentile (Get-PropertyValues $acceptedRows "AcceptedToCallbackDeliveredMs") 50
    AcceptedToCallbackP95Ms           = Get-Percentile (Get-PropertyValues $acceptedRows "AcceptedToCallbackDeliveredMs") 95
    RetryAmplificationAverage         = Get-Average (Get-PropertyValues $acceptedRows "RetryScheduledCount")
    AttemptCountAverage               = Get-Average (Get-PropertyValues $acceptedRows "AttemptCount")
}

$submitStateTable = Get-StateCountTable $benchRows "SubmitStatus"
$finalStateTable = Get-StateCountTable $finalStateRows "FinalStateDisplay"
$classificationTable = Get-StateCountTable $terminalRows "Classification"
$callbackTable = Get-StateCountTable $terminalRows "CallbackState"

$result = [pscustomobject]@{
    Environment = [pscustomobject]@{
        BaseUrl                 = $base
        TenantId                = $TenantId
        Scenario                = $Scenario
        Runtime                 = $resolvedRuntime
        ComposeProject          = $ComposeProject
        RequestCount            = $RequestCount
        SubmitConcurrency       = $SubmitConcurrency
        DuplicateGroupSize      = $DuplicateGroupSize
        HealthStatus            = $health.Status
        ReadyStatus             = $ready.Status
        PreClusterHealth        = $preHealth
        PostClusterHealth       = $postHealth
    }
    Summary = $summary
    SubmitStatusCounts = $submitStateTable
    FinalStateCounts = $finalStateTable
    ClassificationCounts = $classificationTable
    CallbackStateCounts = $callbackTable
    DuplicateGroups = $duplicateGroupRows
    Requests = $benchRows
}

Write-Host "=== Benchmark Summary ==="
$summary | Format-List

Write-Host ""
Write-Host "=== Submit Status Counts ==="
$submitStateTable | Format-Table -AutoSize

Write-Host ""
Write-Host "=== Final State Counts ==="
$finalStateTable | Format-Table -AutoSize

Write-Host ""
Write-Host "=== Classification Counts ==="
$classificationTable | Format-Table -AutoSize

Write-Host ""
Write-Host "=== Callback State Counts ==="
$callbackTable | Format-Table -AutoSize

if ($duplicateGroupRows.Count -gt 0) {
    Write-Host ""
    Write-Host "=== Duplicate Group Outcomes ==="
    $duplicateGroupRows | Format-Table -AutoSize
}

if (-not [string]::IsNullOrWhiteSpace($OutputJsonPath)) {
    $result | ConvertTo-Json -Depth 16 | Set-Content -Path $OutputJsonPath
    Write-Host ""
    Write-Host "Wrote benchmark json to $OutputJsonPath"
}
}
finally {
    if ($fastFailureProfileApplied) {
        Restore-FastFailureProfile
    }
}
