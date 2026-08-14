param(
    [string] $InstallRoot,
    [switch] $NoPathUpdate
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
    if ([string]::IsNullOrWhiteSpace($localAppData)) {
        throw 'Windows LocalApplicationData directory is unavailable.'
    }
    $InstallRoot = Join-Path $localAppData 'Programs\chuang-agent'
}

$scriptDir = [System.IO.Path]::GetDirectoryName($PSCommandPath)
$repoRoot = [System.IO.Directory]::GetParent($scriptDir).FullName
$launcher = Join-Path $scriptDir 'chuang.ps1'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'cargo was not found. Install the stable Rust toolchain from https://rustup.rs and reopen PowerShell.'
}

Push-Location $repoRoot
try {
    cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
}

$binDir = Join-Path $InstallRoot 'bin'
$installedScriptsDir = Join-Path $InstallRoot 'scripts'
$configDir = Join-Path $InstallRoot 'config'
$identityDir = Join-Path $InstallRoot 'identity'
$rulesDir = Join-Path $InstallRoot 'rules'
$serviceDir = Join-Path $InstallRoot 'ops\systemd'
$docsDir = Join-Path $InstallRoot 'docs'
$assetsDir = Join-Path $InstallRoot 'assets'
$pluginsDir = Join-Path $InstallRoot 'plugins'
New-Item -ItemType Directory -Path $binDir -Force | Out-Null
New-Item -ItemType Directory -Path $installedScriptsDir -Force | Out-Null
New-Item -ItemType Directory -Path $configDir -Force | Out-Null
New-Item -ItemType Directory -Path $identityDir -Force | Out-Null
New-Item -ItemType Directory -Path $rulesDir -Force | Out-Null
New-Item -ItemType Directory -Path $serviceDir -Force | Out-Null
New-Item -ItemType Directory -Path $docsDir -Force | Out-Null
New-Item -ItemType Directory -Path $assetsDir -Force | Out-Null
New-Item -ItemType Directory -Path $pluginsDir -Force | Out-Null

$sourceBinary = Join-Path $repoRoot 'target\release\chuang-agent.exe'
$installedBinary = Join-Path $binDir 'chuang-agent.exe'
$installedLauncher = Join-Path $installedScriptsDir 'chuang.ps1'
Copy-Item -LiteralPath $sourceBinary -Destination $installedBinary -Force
Copy-Item -LiteralPath $launcher -Destination $installedLauncher -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'config.example.toml') -Destination (Join-Path $InstallRoot 'config.example.toml') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'config.example-provider-fallback.toml') -Destination (Join-Path $InstallRoot 'config.example-provider-fallback.toml') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'rules\core.md') -Destination (Join-Path $rulesDir 'core.md') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'docs\feishu-dedicated-channel-checklist.md') -Destination (Join-Path $docsDir 'feishu-dedicated-channel-checklist.md') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'assets\capability_primer.txt') -Destination (Join-Path $assetsDir 'capability_primer.txt') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'plugins\registry.example.json') -Destination (Join-Path $pluginsDir 'registry.example.json') -Force
Get-ChildItem -LiteralPath (Join-Path $repoRoot 'ops\systemd') -File | Copy-Item -Destination $serviceDir -Force
Get-ChildItem -LiteralPath (Join-Path $repoRoot 'scripts') -File -Filter 'chuang-feishu-*' | Copy-Item -Destination $installedScriptsDir -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\chuang-real-actuator-adapter.ps1') -Destination $installedScriptsDir -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'config\actuator-allowlist.windows.json') -Destination $configDir -Force

$installedConfig = Join-Path $InstallRoot 'config.toml'
if (-not (Test-Path -LiteralPath $installedConfig -PathType Leaf)) {
    Copy-Item -LiteralPath (Join-Path $InstallRoot 'config.example.toml') -Destination $installedConfig
}
$configText = [System.IO.File]::ReadAllText($installedConfig)
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
    [System.IO.File]::WriteAllText($installedConfig, $configText, [System.Text.UTF8Encoding]::new($false))
}

foreach ($name in @('SOUL', 'STORY', 'FIRST_WAKE')) {
    $exampleName = "$name.example.md"
    $exampleTarget = Join-Path $identityDir $exampleName
    Copy-Item -LiteralPath (Join-Path $repoRoot "identity\$exampleName") -Destination $exampleTarget -Force
    $identityTarget = Join-Path $identityDir "$name.md"
    if (-not (Test-Path -LiteralPath $identityTarget -PathType Leaf)) {
        Copy-Item -LiteralPath $exampleTarget -Destination $identityTarget
    }
}

$cmdPath = Join-Path $binDir 'chuang.cmd'
$cmdBody = "@echo off`r`npowershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$installedLauncher`" %*`r`n"
[System.IO.File]::WriteAllText($cmdPath, $cmdBody, [System.Text.Encoding]::ASCII)

if (-not $NoPathUpdate) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($userPath -split ';' | Where-Object { $_ })
    if ($parts -notcontains $binDir) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $binDir) -join ';'), 'User')
    }
}

Write-Host "chuang-agent installed: $cmdPath"
Write-Host 'Open a new PowerShell window, then run: chuang doctor'
Write-Host "Current window: & '$cmdPath' doctor"
