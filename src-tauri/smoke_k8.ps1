Start-Process -FilePath "D:\Users\Administrator\Desktop\監控\src-tauri\target\release\lobster-pulse.exe" -WindowStyle Hidden
Start-Sleep -Seconds 4
$port = Get-Content "$env:USERPROFILE\.lobsterpulse\port" -ErrorAction SilentlyContinue
if (-not $port) { Write-Output "PORT_FILE_MISSING"; exit 1 }
Write-Output "PORT=$port"
$metricsPort = [int]$port + 100
$body = Invoke-WebRequest -Uri "http://127.0.0.1:$metricsPort/metrics" -UseBasicParsing -TimeoutSec 5
$body.Content | Select-String -Pattern "idle_seconds"
Get-Process lobster-pulse -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output "DONE"
