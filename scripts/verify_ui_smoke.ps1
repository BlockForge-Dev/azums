param(
    [string]$BaseUrl = "https://app.example.com",
    [string]$Email = "",
    [string]$Password = "",
    [string]$SignupEmail = "",
    [string]$SignupPassword = "",
    [switch]$ExercisePasswordReset
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Read-OptionalProperty {
    param(
        [object]$Object,
        [string]$Name,
        [object]$Default = $null
    )
    if ($null -eq $Object) { return $Default }
    $prop = $Object.PSObject.Properties[$Name]
    if ($null -eq $prop) { return $Default }
    return $prop.Value
}

function Invoke-HttpCheck {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [int[]]$ExpectedStatus = @(200, 302, 307, 308),
        [Microsoft.PowerShell.Commands.WebRequestSession]$Session = $null,
        [string]$Method = "GET",
        [object]$Body = $null
    )

    $params = @{
        Uri                = $Url
        Method             = $Method
        MaximumRedirection = 0
        SkipHttpErrorCheck = $true
        ErrorAction        = "Stop"
    }
    if ($null -ne $Session) {
        $params.WebSession = $Session
    }
    if ($null -ne $Body) {
        $params.ContentType = "application/json"
        $params.Body = ($Body | ConvertTo-Json -Depth 16 -Compress)
    }

    $response = Invoke-WebRequest @params
    $ok = @($ExpectedStatus) -contains [int]$response.StatusCode
    [pscustomobject]@{
        Url        = $Url
        Method     = $Method
        StatusCode = [int]$response.StatusCode
        Ok         = $ok
        Location   = [string]$response.Headers["Location"]
    }
}

function Invoke-JsonApi {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [object]$Body = $null,
        [Microsoft.PowerShell.Commands.WebRequestSession]$Session = $null
    )

    $params = @{
        Uri                = $Url
        Method             = $Method
        SkipHttpErrorCheck = $true
        ErrorAction        = "Stop"
    }
    if ($null -ne $Session) {
        $params.WebSession = $Session
    }
    if ($null -ne $Body) {
        $params.ContentType = "application/json"
        $params.Body = ($Body | ConvertTo-Json -Depth 16 -Compress)
    }

    $response = Invoke-WebRequest @params
    $parsed = $null
    if (-not [string]::IsNullOrWhiteSpace($response.Content)) {
        try {
            $parsed = $response.Content | ConvertFrom-Json
        }
        catch {
            $parsed = $response.Content
        }
    }
    [pscustomobject]@{
        StatusCode = [int]$response.StatusCode
        Body       = $parsed
        Raw        = [string]$response.Content
    }
}

$root = $BaseUrl.TrimEnd("/")
$session = New-Object Microsoft.PowerShell.Commands.WebRequestSession
$failures = New-Object System.Collections.Generic.List[string]

$pageChecks = @(
    Invoke-HttpCheck -Url "$root/"
    Invoke-HttpCheck -Url "$root/login"
    Invoke-HttpCheck -Url "$root/signup"
    Invoke-HttpCheck -Url "$root/pricing"
    Invoke-HttpCheck -Url "$root/forgot-password"
    Invoke-HttpCheck -Url "$root/verify-email"
    Invoke-HttpCheck -Url "$root/app"
    Invoke-HttpCheck -Url "$root/ops"
)

$uiConfig = Invoke-JsonApi -Method "GET" -Url "$root/api/ui/config"
$uiHealth = Invoke-JsonApi -Method "GET" -Url "$root/api/ui/health"

$loginSummary = $null
$sessionSummary = $null
$operatorOverviewSummary = $null
$billingProvidersSummary = $null
if (-not [string]::IsNullOrWhiteSpace($Email) -and -not [string]::IsNullOrWhiteSpace($Password)) {
    $login = Invoke-JsonApi -Method "POST" -Url "$root/api/ui/account/login" -Session $session -Body @{
        email = $Email
        password = $Password
    }
    $loginSummary = [pscustomobject]@{
        StatusCode = $login.StatusCode
        Ok         = (
            $login.StatusCode -ge 200 -and
            $login.StatusCode -lt 300 -and
            [bool](Read-OptionalProperty -Object $login.Body -Name "ok" -Default $false)
        )
    }
    if ($loginSummary.Ok) {
        $sessionResp = Invoke-JsonApi -Method "GET" -Url "$root/api/ui/account/session" -Session $session
        $sessionSummary = [pscustomobject]@{
            StatusCode = $sessionResp.StatusCode
            Ok         = (
                $sessionResp.StatusCode -ge 200 -and
                $sessionResp.StatusCode -lt 300 -and
                [bool](Read-OptionalProperty -Object $sessionResp.Body -Name "ok" -Default $false)
            )
        }
        $operatorOverview = Invoke-JsonApi -Method "GET" -Url "$root/api/ui/operator/overview" -Session $session
        $operatorOverviewSummary = [pscustomobject]@{
            StatusCode = $operatorOverview.StatusCode
            Ok         = ($operatorOverview.StatusCode -ge 200 -and $operatorOverview.StatusCode -lt 300)
        }
        $billingProviders = Invoke-JsonApi -Method "GET" -Url "$root/api/ui/account/billing/providers" -Session $session
        $billingProvidersSummary = [pscustomobject]@{
            StatusCode = $billingProviders.StatusCode
            Ok         = ($billingProviders.StatusCode -ge 200 -and $billingProviders.StatusCode -lt 300)
        }
        if ($uiConfig.StatusCode -eq 401) {
            $uiConfig = Invoke-JsonApi -Method "GET" -Url "$root/api/ui/config" -Session $session
        }
    }
}

$signupSummary = $null
if (-not [string]::IsNullOrWhiteSpace($SignupEmail) -and -not [string]::IsNullOrWhiteSpace($SignupPassword)) {
    $signup = Invoke-JsonApi -Method "POST" -Url "$root/api/ui/account/signup" -Body @{
        full_name = "Production Smoke"
        email = $SignupEmail
        password = $SignupPassword
        workspace_name = "smoke-$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
        plan = "Developer"
    }
    $signupSummary = [pscustomobject]@{
        StatusCode = $signup.StatusCode
        Ok         = ($signup.StatusCode -ge 200 -and $signup.StatusCode -lt 300)
        Response   = $signup.Body
    }
}

$passwordResetSummary = $null
if ($ExercisePasswordReset -and -not [string]::IsNullOrWhiteSpace($SignupEmail)) {
    $passwordReset = Invoke-JsonApi -Method "POST" -Url "$root/api/ui/account/password-reset/request" -Body @{
        email = $SignupEmail
    }
    $passwordResetSummary = [pscustomobject]@{
        StatusCode = $passwordReset.StatusCode
        Ok         = ($passwordReset.StatusCode -ge 200 -and $passwordReset.StatusCode -lt 300)
        Response   = $passwordReset.Body
    }
}

$uiConfigAccessible = ($uiConfig.StatusCode -ge 200 -and $uiConfig.StatusCode -lt 300)
$emailDeliveryConfigured = if ($uiConfigAccessible) { Read-OptionalProperty -Object $uiConfig.Body -Name "email_delivery_configured" -Default $null } else { $null }
$requiresEmailVerification = if ($uiConfigAccessible) { Read-OptionalProperty -Object $uiConfig.Body -Name "requires_email_verification" -Default $null } else { $null }
$passwordResetEnabled = if ($uiConfigAccessible) { Read-OptionalProperty -Object $uiConfig.Body -Name "password_reset_enabled" -Default $null } else { $null }

$summary = [pscustomobject]@{
    BaseUrl                        = $root
    UiConfigStatusCode             = $uiConfig.StatusCode
    UiConfigOk                     = $uiConfigAccessible
    UiHealthOk                     = (
        $uiHealth.StatusCode -ge 200 -and
        $uiHealth.StatusCode -lt 300 -and
        [bool](Read-OptionalProperty -Object $uiHealth.Body -Name "ok" -Default $false)
    )
    EmailDeliveryConfigured        = $emailDeliveryConfigured
    RequiresEmailVerification      = $requiresEmailVerification
    PasswordResetEnabled           = $passwordResetEnabled
    LoginOk                        = if ($null -ne $loginSummary) { [bool]$loginSummary.Ok } else { $false }
    SessionOk                      = if ($null -ne $sessionSummary) { [bool]$sessionSummary.Ok } else { $false }
    OperatorOverviewOk             = if ($null -ne $operatorOverviewSummary) { [bool]$operatorOverviewSummary.Ok } else { $false }
    BillingProvidersOk             = if ($null -ne $billingProvidersSummary) { [bool]$billingProvidersSummary.Ok } else { $false }
    SignupAttempted                = ($null -ne $signupSummary)
    SignupAccepted                 = if ($null -ne $signupSummary) { [bool]$signupSummary.Ok } else { $false }
    PasswordResetAttempted         = ($null -ne $passwordResetSummary)
    PasswordResetRequestOk         = if ($null -ne $passwordResetSummary) { [bool]$passwordResetSummary.Ok } else { $false }
    ManualInboxVerificationRequired = $true
    ManualBrowserInteractionRequired = $true
}

Write-Host ""
Write-Host "=== UI Smoke Summary ==="
$summary | Format-List

Write-Host ""
Write-Host "=== Page Checks ==="
$pageChecks | Format-Table -AutoSize

if ($null -ne $signupSummary) {
    Write-Host ""
    Write-Host "=== Signup Result ==="
    $signupSummary | Format-List
}

if ($null -ne $passwordResetSummary) {
    Write-Host ""
    Write-Host "=== Password Reset Request ==="
    $passwordResetSummary | Format-List
}

foreach ($pageCheck in $pageChecks) {
    if (-not $pageCheck.Ok) {
        $failures.Add("Page check failed: $($pageCheck.Method) $($pageCheck.Url) -> $($pageCheck.StatusCode)")
    }
}
if (-not [bool]$summary.UiHealthOk) {
    $failures.Add("UI health check failed.")
}
if ($null -ne $loginSummary -and -not [bool]$loginSummary.Ok) {
    $failures.Add("Login failed with status $($loginSummary.StatusCode).")
}
if ($null -ne $sessionSummary -and -not [bool]$sessionSummary.Ok) {
    $failures.Add("Session check failed with status $($sessionSummary.StatusCode).")
}
if ($null -ne $operatorOverviewSummary -and -not [bool]$operatorOverviewSummary.Ok) {
    $failures.Add("Operator overview failed with status $($operatorOverviewSummary.StatusCode).")
}
if ($null -ne $billingProvidersSummary -and -not [bool]$billingProvidersSummary.Ok) {
    $failures.Add("Billing providers check failed with status $($billingProvidersSummary.StatusCode).")
}
if ($null -ne $signupSummary -and -not [bool]$signupSummary.Ok) {
    $failures.Add("Signup failed with status $($signupSummary.StatusCode).")
}
if ($null -ne $passwordResetSummary -and -not [bool]$passwordResetSummary.Ok) {
    $failures.Add("Password reset request failed with status $($passwordResetSummary.StatusCode).")
}
if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "UI smoke failed:"
    foreach ($failure in $failures) {
        Write-Host " - $failure"
    }
    exit 1
}
