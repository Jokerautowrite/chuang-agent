[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $ChuangArgs = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = [System.IO.Path]::GetDirectoryName($PSCommandPath)
$repoRoot = [System.IO.Directory]::GetParent($scriptDir).FullName
$callerRoot = (Get-Location).Path
$configPath = Join-Path $repoRoot 'config.toml'
$configExample = Join-Path $repoRoot 'config.example.toml'
$credentialRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'chuang-agent\credentials'
$providerKeyPath = Join-Path $credentialRoot 'provider-key.dpapi'

if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) {
    Copy-Item -LiteralPath $configExample -Destination $configPath
}
$configText = [System.IO.File]::ReadAllText($configPath)
if ($configText -match '(?m)^actuator\s*=\s*"fake"\s*$') {
    $actuatorBlock = @'
actuator = "command"
actuator_program = "powershell.exe"
actuator_args = "-NoProfile -ExecutionPolicy Bypass -File scripts/chuang-real-actuator-adapter.ps1"
actuator_timeout_ms = 30000
'@
    $configText = [regex]::Replace(
        $configText,
        '(?m)^actuator\s*=\s*"fake"\s*$',
        $actuatorBlock.Trim(),
        1
    )
    [System.IO.File]::WriteAllText($configPath, $configText, [System.Text.UTF8Encoding]::new($false))
}

if ($ChuangArgs.Count -gt 0 -and $ChuangArgs[0] -eq 'login') {
    $providerKey = Read-Host 'Paste API Key (input hidden)' -AsSecureString
    if ($providerKey.Length -eq 0) {
        throw 'API Key cannot be empty.'
    }
    New-Item -ItemType Directory -Path $credentialRoot -Force | Out-Null
    $encrypted = ConvertFrom-SecureString $providerKey
    [System.IO.File]::WriteAllText(
        $providerKeyPath,
        $encrypted,
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-Host 'API Key saved with Windows DPAPI for the current user.'
    exit 0
}

if (-not $env:CHUANG_AGENT_API_KEY -and (Test-Path -LiteralPath $providerKeyPath -PathType Leaf)) {
    try {
        $encrypted = [System.IO.File]::ReadAllText($providerKeyPath).Trim()
        $providerKey = ConvertTo-SecureString $encrypted
        $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($providerKey)
        try {
            $env:CHUANG_AGENT_API_KEY = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
        }
        finally {
            [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
        }
    }
    catch {
        Write-Warning 'Saved API Key cannot be decrypted for this Windows user/session. Run: chuang login'
    }
}

$env:CHUANG_AGENT_ROOT = $repoRoot
$env:CHUANG_AGENT_WORKSPACE_ROOT = $callerRoot
$env:CHUANG_REPL_WORKSPACE_ROOT = $callerRoot
if (-not $env:CHUANG_APP_SERVER_MODE) { $env:CHUANG_APP_SERVER_MODE = 'local' }
if (-not $env:CHUANG_REAL_ACTUATOR_ENABLE) { $env:CHUANG_REAL_ACTUATOR_ENABLE = '1' }
if (-not $env:CHUANG_REAL_CONTROL_ENABLE) { $env:CHUANG_REAL_CONTROL_ENABLE = '0' }
if (-not $env:CHUANG_CODEX_RUNNER_ENABLE) { $env:CHUANG_CODEX_RUNNER_ENABLE = '0' }

$installedBinary = Join-Path $repoRoot 'bin\chuang-agent.exe'
$sourceBinary = Join-Path $repoRoot 'target\release\chuang-agent.exe'
$isSourceCheckout = Test-Path -LiteralPath (Join-Path $repoRoot 'Cargo.toml') -PathType Leaf
$binary = if (Test-Path -LiteralPath $installedBinary -PathType Leaf) {
    $installedBinary
}
else {
    $sourceBinary
}
$needsBuild = $isSourceCheckout -and -not (Test-Path -LiteralPath $binary -PathType Leaf)
if ($isSourceCheckout -and -not $needsBuild) {
    $binaryTime = (Get-Item -LiteralPath $binary).LastWriteTimeUtc
    $inputs = @(
        Get-Item -LiteralPath (Join-Path $repoRoot 'Cargo.toml'), (Join-Path $repoRoot 'Cargo.lock')
        Get-ChildItem -LiteralPath (Join-Path $repoRoot 'src') -Recurse -File -Filter '*.rs'
    )
    $needsBuild = [bool]($inputs | Where-Object LastWriteTimeUtc -GT $binaryTime | Select-Object -First 1)
}

if (-not $isSourceCheckout -and -not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "installed chuang-agent binary was not found: $binary"
}

Push-Location $repoRoot
try {
    if ($needsBuild) {
        $cargo = Get-Command cargo -ErrorAction SilentlyContinue
        if (-not $cargo) {
            throw 'cargo was not found. Install the stable Rust toolchain from https://rustup.rs and reopen PowerShell.'
        }
        & $cargo.Source build --locked --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    }

    $arguments = [System.Collections.Generic.List[string]]::new()
    if ($ChuangArgs.Count -eq 0) {
        $arguments.Add('repl')
        $arguments.Add('--config')
        $arguments.Add($configPath)
    }
    elseif ($ChuangArgs[0] -eq 'ask') {
        if ($ChuangArgs.Count -lt 2) { throw 'usage: chuang ask "your question"' }
        $arguments.Add('run')
        $arguments.Add('--config')
        $arguments.Add($configPath)
        $arguments.Add('--input')
        $arguments.Add(($ChuangArgs[1..($ChuangArgs.Count - 1)] -join ' '))
    }
    else {
        foreach ($argument in $ChuangArgs) { $arguments.Add($argument) }
        if ($ChuangArgs[0] -in @('status', 'doctor') -and $ChuangArgs -notcontains '--config') {
            $arguments.Add('--config')
            $arguments.Add($configPath)
        }
    }

    & $binary @arguments
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
