# collector/scripts/register_tasks.ps1
# Register MachineCollector (ONLOGON) + MachineCollectorWatchdog (every 5 min).
# Run with: pwsh -File register_tasks.ps1
# Note: schtasks default priority is BelowNormal (pitfall 9) - acceptable for
# this lightweight collector; do not host latency-critical work in these tasks.
$ErrorActionPreference = "Stop"

$CollectorDir = Split-Path -Parent $PSScriptRoot
$PythonW = (Get-Command pythonw.exe).Source
$RunScript = Join-Path $CollectorDir "run_collector.py"
$WatchdogScript = Join-Path $PSScriptRoot "watchdog.py"

if (-not (Test-Path $RunScript)) { throw "run_collector.py not found: $RunScript" }

schtasks /Create /F /TN "MachineCollector" /SC ONLOGON `
  /TR "`"$PythonW`" `"$RunScript`""
schtasks /Create /F /TN "MachineCollectorWatchdog" /SC MINUTE /MO 5 `
  /TR "`"$PythonW`" `"$WatchdogScript`""

schtasks /Run /TN "MachineCollector"
Write-Output "Registered: MachineCollector (ONLOGON) + MachineCollectorWatchdog (5 min)"
