# LobsterPulse watchdog — 掛了自己爬起來。
#
# 為什麼需要：開機自啟只解決「開機」，不解決「崩潰」。而 LobsterPulse 掛掉是
# 最難察覺的一種——它自己就是那個顯示「有沒有問題」的東西，沒了只會覺得今天
# 比較安靜。這台機器上其他常駐服務都有 watchdog，只有它沒有。
#
# 判活用 healthz 200，不看 PID：進程活著但 wedge 住的情況照樣要救（踩雷 §5 §18）。
# 連續失敗才動作，避免重啟／短暫卡頓時誤殺。重啟只做 Start-Process：app 有
# single-instance plugin，真的還活著時多啟的那份會自己退出，不必先殺再起。
#
# 由 lobsterpulse-watchdog.vbs 隱藏啟動（見 runtime/README.md）。

param(
  [int]$IntervalSec = 60,
  [int]$FailuresBeforeRestart = 3,
  [string]$Exe = "D:\Users\Administrator\Desktop\監控\src-tauri\target\release\lobster-pulse.exe"
)

$ErrorActionPreference = "Continue"
$logDir = Join-Path $env:LOCALAPPDATA "com.lobsterpulse.desktop\logs"
$log = Join-Path $logDir "watchdog.log"
$portFile = Join-Path $env:USERPROFILE ".lobsterpulse\port"

function Write-Log([string]$msg) {
  New-Item -ItemType Directory -Path $logDir -Force | Out-Null
  $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $msg
  Add-Content -Path $log -Value $line -Encoding utf8
  # 只留最後 500 行，watchdog 的 log 不該自己長成磁碟問題
  $c = @(Get-Content $log -ErrorAction SilentlyContinue)
  if ($c.Count -gt 800) { Set-Content -Path $log -Value ($c | Select-Object -Last 500) -Encoding utf8 }
}

function Test-Healthy {
  if (-not (Test-Path $portFile)) { return $false }
  $port = (Get-Content $portFile -ErrorAction SilentlyContinue | Select-Object -First 1)
  if (-not $port) { return $false }
  try {
    $r = Invoke-WebRequest "http://127.0.0.1:$port/healthz" -UseBasicParsing -TimeoutSec 5
    return ($r.StatusCode -eq 200)
  } catch { return $false }
}

# 單一實例：登入觸發＋手動測試很容易疊出第二隻，兩隻同時判失敗會連續重啟。
# 用具名 Mutex（跨進程），拿不到就安靜退出。
$mutex = New-Object System.Threading.Mutex($false, "Local\LobsterPulseWatchdog")
if (-not $mutex.WaitOne(0)) {
  Write-Log "已有另一個 watchdog 在跑，本次退出"
  exit 0
}

Write-Log "watchdog 啟動（每 ${IntervalSec}s 檢查，連續 ${FailuresBeforeRestart} 次失敗才重啟）"
$fails = 0
while ($true) {
  if (Test-Healthy) {
    if ($fails -gt 0) { Write-Log "恢復正常（先前連續失敗 $fails 次）" }
    $fails = 0
  } else {
    $fails++
    Write-Log "healthz 失敗（第 $fails 次）"
    if ($fails -ge $FailuresBeforeRestart) {
      if (Test-Path $Exe) {
        Write-Log "重啟 $Exe"
        Start-Process $Exe
      } else {
        Write-Log "找不到執行檔 $Exe，無法重啟"
      }
      $fails = 0
      Start-Sleep -Seconds 30  # 給啟動時間，避免連續重啟風暴
    }
  }
  Start-Sleep -Seconds $IntervalSec
}
