# Windows counterpart to codex-runtime-smoke.sh (issue #3).
# The hosted runner is disposable, so it temporarily backs up and replaces
# only that runner user's Known Folder profile settings, then restores them.
# This is deliberately guarded against local execution: dirs::home_dir uses
# FOLDERID_Profile on Windows, so HOME/CODEX_HOME do not isolate the app.
$ErrorActionPreference = 'Stop'

if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or
    $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or
    [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    throw 'This smoke test may run only on a disposable GitHub-hosted Windows runner.'
}

$profileRoot = [IO.Path]::GetFullPath([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile))
if ([string]::IsNullOrWhiteSpace($profileRoot) -or -not (Test-Path -LiteralPath $profileRoot -PathType Container)) {
    throw 'Windows did not return an existing runner user profile.'
}
$runnerTemp = [IO.Path]::GetFullPath($env:RUNNER_TEMP)
if ($runnerTemp.TrimEnd('\') -eq $profileRoot.TrimEnd('\')) {
    throw 'RUNNER_TEMP must be separate from the user profile.'
}

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$releaseDir = Join-Path $repoRoot 'src-tauri\target\release'
$appPath = Join-Path $releaseDir 'lobster-pulse.exe'
$sidecarPath = Join-Path $releaseDir 'lobster-pulse-hook.exe'
foreach ($binary in @($appPath, $sidecarPath)) {
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Missing Windows build artifact: $binary"
    }
}

$runId = [Guid]::NewGuid().ToString('N')
$scratch = Join-Path $runnerTemp "lobsterpulse-codex-smoke-$runId"
$backup = Join-Path $profileRoot ".lobsterpulse-codex-smoke-backup-$runId"
$scratchCreated = $false
$backupCreated = $false
$appProcess = $null
$sidecar = $null
$originalPresent = @{}
$profileEntries = @(
    @{ Name = '.codex'; Backup = (Join-Path $backup 'codex') },
    @{ Name = '.lobsterpulse'; Backup = (Join-Path $backup 'lobsterpulse') }
)

function Get-ExistingPath([string]$Path) {
    try {
        return Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    }
    catch {
        if ($_.CategoryInfo.Category -eq 'ObjectNotFound') { return $null }
        throw "Could not safely inspect runner profile entry (category: $($_.CategoryInfo.Category))."
    }
}

try {
    if ($null -ne (Get-ExistingPath $scratch)) { throw 'Unique smoke scratch path already exists.' }
    [void][IO.Directory]::CreateDirectory($scratch)
    $scratchCreated = $true
    if ($null -ne (Get-ExistingPath $backup)) { throw 'Unique profile backup path already exists.' }
    [void][IO.Directory]::CreateDirectory($backup)
    $backupCreated = $true

    # Quarantine any pre-existing runner-profile data so this run starts from
    # a known state and can restore it even when an assertion fails.
    foreach ($entry in $profileEntries) {
        $path = Join-Path $profileRoot $entry.Name
        $existing = Get-ExistingPath $path
        $originalPresent[$entry.Name] = ($null -ne $existing)
        if ($null -ne $existing) {
            Move-Item -LiteralPath $path -Destination $entry.Backup
            if ($null -eq (Get-ExistingPath $entry.Backup) -or $null -ne (Get-ExistingPath $path)) {
                throw "Could not safely quarantine runner profile entry $($entry.Name)."
            }
        }
    }

    $codexDir = Join-Path $profileRoot '.codex'
    $lpDir = Join-Path $profileRoot '.lobsterpulse'
    [void][IO.Directory]::CreateDirectory($codexDir)
    [void][IO.Directory]::CreateDirectory($lpDir)
    [IO.File]::WriteAllText(
        (Join-Path $codexDir 'config.toml'),
        "# user comment must survive`n[features]`ncodex_hooks = false # user disabled hooks`nhooks = false # canonical key also disabled`n",
        [Text.UTF8Encoding]::new($false)
    )
    [IO.File]::WriteAllText(
        (Join-Path $codexDir 'hooks.json'),
        '{"hooks":{"PreToolUse":[{"matcher":"third-party","hooks":[{"type":"command","command":"third-party-pre"}]}]}}',
        [Text.UTF8Encoding]::new($false)
    )
    Write-Host '[smoke] isolated runner profile prepared'

    $stdoutPath = Join-Path $scratch 'app.stdout.log'
    $stderrPath = Join-Path $scratch 'app.stderr.log'
    $priorHeadless = [Environment]::GetEnvironmentVariable('LOBSTERPULSE_HEADLESS_INSTALL', 'Process')
    try {
        [Environment]::SetEnvironmentVariable('LOBSTERPULSE_HEADLESS_INSTALL', 'codex', 'Process')
        $appProcess = Start-Process -FilePath $appPath -WorkingDirectory $releaseDir -WindowStyle Hidden -PassThru `
            -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    }
    finally {
        [Environment]::SetEnvironmentVariable('LOBSTERPULSE_HEADLESS_INSTALL', $priorHeadless, 'Process')
    }

    $portFile = Join-Path $lpDir 'port'
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not (Test-Path -LiteralPath $portFile -PathType Leaf) -and [DateTime]::UtcNow -lt $deadline) {
        if ($appProcess.HasExited) {
            throw "LobsterPulse exited before writing the port file (exit $($appProcess.ExitCode))."
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not (Test-Path -LiteralPath $portFile -PathType Leaf)) {
        throw 'LobsterPulse did not publish its hook-server port within 30 seconds.'
    }
    Start-Sleep -Seconds 1

    & python (Join-Path $PSScriptRoot 'codex-runtime-smoke-config.py') `
        (Join-Path $codexDir 'config.toml') (Join-Path $codexDir 'hooks.json')
    if ($LASTEXITCODE -ne 0) { throw 'Effective Codex feature config assertion failed.' }

    $portText = [IO.File]::ReadAllText($portFile).Trim()
    $port = 0
    if (-not [int]::TryParse($portText, [ref]$port) -or $port -lt 1 -or $port -gt 65435) {
        throw "Invalid hook-server port: $portText"
    }
    $metricsUri = "http://127.0.0.1:$($port + 100)/metrics"

    # Exercise the exact sidecar executable configured for Codex, with a
    # synthetic SessionStart event. The runner profile remains disposable.
    $sidecarInfo = [Diagnostics.ProcessStartInfo]::new()
    $sidecarInfo.FileName = $sidecarPath
    $sidecarInfo.Arguments = 'codex'
    $sidecarInfo.UseShellExecute = $false
    $sidecarInfo.CreateNoWindow = $true
    $sidecarInfo.RedirectStandardInput = $true
    $sidecarInfo.RedirectStandardOutput = $true
    $sidecarInfo.RedirectStandardError = $true
    $sidecar = [Diagnostics.Process]::Start($sidecarInfo)
    $stdoutTask = $sidecar.StandardOutput.ReadToEndAsync()
    $stderrTask = $sidecar.StandardError.ReadToEndAsync()
    $event = '{"hook_event_name":"SessionStart","sessionId":"windows-smoke-codex-1","cwd":"C:\\smoke"}'
    $sidecar.StandardInput.WriteLine($event)
    $sidecar.StandardInput.Close()
    if (-not $sidecar.WaitForExit(20000)) {
        $sidecar.Kill()
        throw 'Codex sidecar did not exit within 20 seconds.'
    }
    $sidecarStdout = $stdoutTask.GetAwaiter().GetResult()
    $sidecarStderr = $stderrTask.GetAwaiter().GetResult()
    if ($sidecar.ExitCode -ne 0 -or -not [string]::IsNullOrWhiteSpace($sidecarStderr)) {
        throw "Codex sidecar failed with exit code $($sidecar.ExitCode)."
    }

    $metrics = ''
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ([DateTime]::UtcNow -lt $deadline) {
        try {
            $metrics = (Invoke-WebRequest -Uri $metricsUri -UseBasicParsing -TimeoutSec 2).Content
            if ($metrics -match 'lobsterpulse_provider_event_type_total\{provider="codex",type="SessionStart"\} [1-9]') { break }
        }
        catch { }
        Start-Sleep -Milliseconds 250
    }
    if ($metrics -notmatch 'lobsterpulse_provider_event_type_total\{provider="codex",type="SessionStart"\} [1-9]') {
        throw "Synthetic SessionStart did not reach the app metrics endpoint: $metricsUri"
    }
    Write-Host '[smoke] metrics: synthetic Codex SessionStart counted'
    Write-Host '[smoke] PASS — Windows build artifact enabled Codex flags and ingested a sidecar event'
}
catch {
    Write-Host "[smoke] FAIL: $($_.Exception.Message)"
    throw
}
finally {
    $cleanupErrors = [Collections.Generic.List[string]]::new()
    $processesStopped = $true
    if ($null -ne $sidecar) {
        try {
            if (-not $sidecar.HasExited) {
                $sidecar.Kill()
                [void]$sidecar.WaitForExit(10000)
            }
            if (-not $sidecar.HasExited) { throw 'sidecar process remained alive' }
        }
        catch {
            $cleanupErrors.Add("could not stop sidecar PID $($sidecar.Id)")
            $processesStopped = $false
        }
    }
    if ($null -ne $appProcess) {
        try {
            if (-not $appProcess.HasExited) {
                # Tauri/WebView2 may have child processes that keep the
                # runner profile open after the app process exits.
                & "$env:SystemRoot\System32\taskkill.exe" /PID $appProcess.Id /T /F *> $null
                if ($LASTEXITCODE -ne 0 -and -not $appProcess.HasExited) {
                    throw 'taskkill could not stop the app process tree'
                }
                [void]$appProcess.WaitForExit(10000)
            }
            if (-not $appProcess.HasExited) { throw 'LobsterPulse process remained alive' }
        }
        catch {
            $cleanupErrors.Add("could not stop LobsterPulse PID $($appProcess.Id)")
            $processesStopped = $false
        }
    }

    # Restore precisely the profile entries this script quarantined. The
    # runner is disposable, and no path outside its Known Folder is removed.
    # If a process cannot be stopped, leave both original backups and current
    # test state intact for runner-local recovery instead of racing a restore.
    if ($processesStopped) {
        foreach ($entry in $profileEntries) {
            $path = Join-Path $profileRoot $entry.Name
            try {
                $profileItem = Get-ExistingPath $path
                $backupItem = if ($backupCreated) { Get-ExistingPath $entry.Backup } else { $null }
                if (-not $originalPresent.ContainsKey($entry.Name)) {
                    # The quarantine pass never safely inspected this entry;
                    # the app was not launched until the whole pass completed.
                    continue
                }
                $hadOriginal = $originalPresent[$entry.Name]

                if ($hadOriginal -and $null -eq $backupItem -and $null -ne $profileItem) {
                    # Quarantine failed before moving this entry; it is still
                    # the original and must be left untouched.
                    continue
                }
                if ($hadOriginal -and $null -eq $backupItem -and $null -eq $profileItem) {
                    throw 'original backup and profile entry are both missing'
                }
                if (-not $hadOriginal -and $null -ne $backupItem) {
                    throw 'unexpected backup exists for an originally absent profile entry'
                }

                if ($null -ne $profileItem) {
                    if ($profileItem.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                        Remove-Item -LiteralPath $path -Force
                    }
                    else {
                        Remove-Item -LiteralPath $path -Recurse -Force
                    }
                }
                if ($hadOriginal) {
                    Move-Item -LiteralPath $entry.Backup -Destination $path
                    if ($null -eq (Get-ExistingPath $path) -or $null -ne (Get-ExistingPath $entry.Backup)) {
                        throw 'restored profile entry could not be verified'
                    }
                }
                elseif ($null -ne (Get-ExistingPath $path)) {
                    throw 'smoke profile entry remained after removal'
                }
            }
            catch {
                $cleanupErrors.Add("could not safely restore runner profile entry $($entry.Name)")
            }
        }
    }

    if ($cleanupErrors.Count -eq 0 -and $processesStopped) {
        try {
            if ($backupCreated) {
                $remainingBackupEntries = @(Get-ChildItem -LiteralPath $backup -Force -ErrorAction Stop)
                if ($remainingBackupEntries.Count -gt 0) {
                    throw 'backup still contains entries after profile restoration'
                }
                Remove-Item -LiteralPath $backup -Force -ErrorAction Stop
            }
            if ($scratchCreated) {
                Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction Stop
            }
        }
        catch {
            $cleanupErrors.Add('could not remove temporary smoke files after restoring the runner profile')
        }
    }
    if ($cleanupErrors.Count -gt 0) {
        Write-Host '[smoke] cleanup incomplete; runner-only backup retained for runner teardown'
        throw ($cleanupErrors -join '; ')
    }
}
