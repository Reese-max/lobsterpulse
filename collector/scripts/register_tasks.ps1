# collector/scripts/register_tasks.ps1
# Register MachineCollector (ONLOGON) + MachineCollectorWatchdog (every 5 min).
# Run with: pwsh -File register_tasks.ps1
# Note: schtasks default priority is BelowNormal (pitfall 9) - acceptable for
# this lightweight collector; do not host latency-critical work in these tasks.
$ErrorActionPreference = "Stop"

$CollectorDir = Split-Path -Parent $PSScriptRoot

# Interpreter must actually have the collector's deps (psutil/yaml/requests) -
# `(Get-Command pythonw.exe).Source` alone can resolve to an unrelated venv's
# pythonw (e.g. a foreign tool's venv) that lacks them. Validate candidates.
$Candidates = @()
$py = (Get-Command python.exe -ErrorAction SilentlyContinue)?.Source
if ($py) { $Candidates += (Join-Path (Split-Path $py) "pythonw.exe") }
$pyw = (Get-Command pythonw.exe -ErrorAction SilentlyContinue)?.Source
if ($pyw) { $Candidates += $pyw }
$PythonW = $null
foreach ($c in $Candidates | Select-Object -Unique) {
    if ((Test-Path $c) -and $(& $c -c "import psutil, yaml, requests" 2>$null; $LASTEXITCODE -eq 0)) {
        $PythonW = $c; break
    }
}
if (-not $PythonW) { throw "No pythonw.exe with required deps (psutil/yaml/requests) found" }
Write-Output "Using interpreter: $PythonW"

$RunScript = Join-Path $CollectorDir "run_collector.py"
$WatchdogScript = Join-Path $PSScriptRoot "watchdog.py"

if (-not (Test-Path $RunScript)) { throw "run_collector.py not found: $RunScript" }

schtasks /Create /F /TN "MachineCollector" /SC ONLOGON `
  /TR "`"$PythonW`" `"$RunScript`""
schtasks /Create /F /TN "MachineCollectorWatchdog" /SC MINUTE /MO 5 `
  /TR "`"$PythonW`" `"$WatchdogScript`""

schtasks /Run /TN "MachineCollector"
Write-Output "Registered: MachineCollector (ONLOGON) + MachineCollectorWatchdog (5 min)"
