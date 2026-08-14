[CmdletBinding()]
param(
    [string] $Allowlist = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$auditLabel = 'actuator.operation.live'
$requiredEnv = 'CHUANG_REAL_ACTUATOR_ENABLE'
if ([string]::IsNullOrWhiteSpace($Allowlist)) {
    $Allowlist = Join-Path $PSScriptRoot '..\config\actuator-allowlist.windows.json'
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName Microsoft.VisualBasic
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class ChuangWindowsDesktop {
    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    private static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    private static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);

    public static string ActiveWindowTitle() {
        IntPtr handle = GetForegroundWindow();
        if (handle == IntPtr.Zero) return "";
        StringBuilder title = new StringBuilder(1024);
        return GetWindowText(handle, title, title.Capacity) > 0 ? title.ToString() : "";
    }

    public static void Click(int x, int y) {
        if (!SetCursorPos(x, y)) throw new InvalidOperationException("SetCursorPos failed");
        mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
        mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    }
}
'@

function New-Response {
    param($Observation, $AppHandle, $EvidenceRef, [string] $Message = 'ok')
    [ordered]@{
        observation = $Observation
        app_handle = $AppHandle
        evidence_ref = $EvidenceRef
        message = $Message
    }
}

function Get-BoundaryMessage {
    param(
        [string] $Action,
        [switch] $RealExecution,
        [switch] $ReadOnly,
        [string] $EvidencePath = ''
    )
    $dryRun = if ($RealExecution -or $ReadOnly) { 'false' } else { 'true' }
    $real = if ($RealExecution) { 'true' } else { 'false' }
    $readOnlyValue = if ($ReadOnly) { 'true' } else { 'false' }
    $liveGateRequired = if ($ReadOnly) { 'false' } else { 'true' }
    $prefix = if ($RealExecution) {
        'allowlisted live actuator operation requested'
    }
    elseif ($ReadOnly) {
        'allowlisted read-only actuator observation'
    }
    else {
        'dry-run actuator operation accepted'
    }
    $message = "$prefix; allowed=true dry_run=$dryRun action=$Action real_execution=$real read_only=$readOnlyValue live_gate_required=$liveGateRequired audit_label=$auditLabel required_env=$requiredEnv platform=windows"
    if ($EvidencePath) { $message += " evidence_path=$EvidencePath" }
    $message
}

function Test-LiveEnabled {
    $env:CHUANG_REAL_ACTUATOR_ENABLE -eq '1'
}

function Get-AppEntry {
    param($Config, [string] $Name)
    @($Config.apps) | Where-Object { $_.app_name -eq $Name } | Select-Object -First 1
}

function Save-Screenshot {
    param($Config)
    if (-not $Config.screenshot_allowed) { throw 'screenshot not allowlisted' }
    $evidenceRoot = $env:CHUANG_ACTUATOR_EVIDENCE_DIR
    if ([string]::IsNullOrWhiteSpace($evidenceRoot)) {
        $evidenceRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'chuang-agent\evidence'
    }
    New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
    $path = Join-Path $evidenceRoot ("screenshot-{0}-{1}.png" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(), $PID)
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) { throw 'Windows virtual screen is unavailable' }
    $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($bounds.X, $bounds.Y, 0, 0, $bounds.Size)
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw 'Windows screenshot file was not created' }
    $uri = [System.Uri]::new($path).AbsoluteUri
    New-Response -EvidenceRef ([ordered]@{ uri = $uri }) -Message (Get-BoundaryMessage -Action screenshot -ReadOnly -EvidencePath $path)
}

function Send-PlainText {
    param([string] $Text)
    foreach ($character in $Text.ToCharArray()) {
        switch ([int]$character) {
            10 { [System.Windows.Forms.SendKeys]::SendWait('{ENTER}'); continue }
            13 { continue }
            9 { [System.Windows.Forms.SendKeys]::SendWait('{TAB}'); continue }
        }
        $value = [string]$character
        if ('+^%~(){}[]'.Contains($value)) { $value = "{$value}" }
        [System.Windows.Forms.SendKeys]::SendWait($value)
    }
}

$requestText = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($requestText)) { throw 'actuator request JSON is empty' }
if (-not (Test-Path -LiteralPath $Allowlist -PathType Leaf)) { throw "actuator allowlist not found: $Allowlist" }
$config = Get-Content -LiteralPath $Allowlist -Raw -Encoding UTF8 | ConvertFrom-Json
$request = $requestText | ConvertFrom-Json
$action = [string]$request.action

$result = switch ($action) {
    'observe' {
        $title = [ChuangWindowsDesktop]::ActiveWindowTitle()
        $screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $summary = "current_window_title=$title screen=$($screen.Width)x$($screen.Height) platform=windows"
        New-Response -Observation ([ordered]@{
            target = $request.observe_target
            summary = $summary
            evidence_ref = [ordered]@{ uri = 'chuang-actuator://observe/windows' }
        }) -Message (Get-BoundaryMessage -Action observe -ReadOnly)
        break
    }
    'screenshot' { Save-Screenshot -Config $config; break }
    'open_app' {
        $name = [string]$request.open_app.app_name
        $app = Get-AppEntry -Config $config -Name $name
        if ($null -eq $app) { throw "app not allowlisted: $name" }
        if (Test-LiveEnabled) {
            $command = @($app.open_command)
            if ($command.Count -eq 0) { throw "app missing open_command: $name" }
            $arguments = if ($command.Count -gt 1) { $command[1..($command.Count - 1)] } else { @() }
            Start-Process -FilePath $command[0] -ArgumentList $arguments | Out-Null
            $message = Get-BoundaryMessage -Action open_app -RealExecution
        }
        else { $message = Get-BoundaryMessage -Action open_app }
        New-Response -AppHandle ([ordered]@{ app_name = $name; handle_id = "chuang-actuator://app/$name" }) -Message $message
        break
    }
    'focus' {
        if (-not $config.focus_allowed) { throw 'focus not allowlisted' }
        $target = [string]$request.focus_target
        if (Test-LiveEnabled -and $target) {
            $activated = [Microsoft.VisualBasic.Interaction]::AppActivate($target)
            if (-not $activated) { throw "window not found: $target" }
            New-Response -Message (Get-BoundaryMessage -Action focus -RealExecution)
        }
        else { New-Response -Message (Get-BoundaryMessage -Action focus) }
        break
    }
    'click' {
        if (-not $config.click_allowed) { throw 'click not allowlisted' }
        if (Test-LiveEnabled) {
            $coordinates = $request.click_target.Coordinates
            if ($null -eq $coordinates) { throw 'click target missing Coordinates' }
            [ChuangWindowsDesktop]::Click([int]$coordinates.x, [int]$coordinates.y)
            New-Response -Message (Get-BoundaryMessage -Action click -RealExecution)
        }
        else { New-Response -Message (Get-BoundaryMessage -Action click) }
        break
    }
    'input_text' {
        if (-not $config.input_allowed) { throw 'input_text not allowlisted' }
        if ($null -ne $request.text.Secret) { throw 'secret input is not supported by this command adapter' }
        $text = [string]$request.text.Plain
        if (Test-LiveEnabled) {
            Send-PlainText -Text $text
            New-Response -Message (Get-BoundaryMessage -Action input_text -RealExecution)
        }
        else { New-Response -Message (Get-BoundaryMessage -Action input_text) }
        break
    }
    default { throw "unsupported actuator action: $action" }
}

$result | ConvertTo-Json -Depth 8 -Compress
