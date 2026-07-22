# LobsterPulse watchdog — 掛了自己爬起來。
#
# 為什麼需要：開機自啟只解決「開機」，不解決「崩潰」。而 LobsterPulse 掛掉是
# 最難察覺的一種——它自己就是那個顯示「有沒有問題」的東西，沒了只會覺得今天
# 比較安靜。這台機器上其他常駐服務都有 watchdog，只有它沒有。
#
# 判活用 healthz 200，不看 PID：進程活著但 wedge 住的情況照樣要救（踩雷 §5 §18）。
# 連續失敗才動作，避免重啟／短暫卡頓時誤殺。重啟前會先清場（Clear-BeforeRestart）：
# 舊實例 wedge 掛掉時，它的 socket 常卡在首選 port 不放，新實例只好漂到 19281，
# 還會疊出第二個窗（實測踩過，死 socket 見踩雷 §22）。既然到這步已判定不健康，
# 先殺光本專案實例＋等原 port 釋放再起，保證單一實例落回 19280。
#
# 由 lobsterpulse-watchdog.vbs 隱藏啟動（見 runtime/README.md）。

param(
  [int]$IntervalSec = 60,
  [int]$FailuresBeforeRestart = 3,
  [int]$StaleMinutes = 15,
  [int]$PauseMaxMinutes = 30,
  [int]$BasePort = 19280,  # app 依序試 19280..19289（hook_server.rs:172），這是首選 port
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

# healthz 200 不代表在做事（踩雷 §8：健康 ≠ 活著）。額度快照該每分鐘更新，
# 停超過 StaleMinutes 就是 runner 那條鏈死了——進程還在，重啟救不了，先出聲。
function Test-DataFresh {
  $f = Join-Path $env:USERPROFILE ".lobsterpulse\usage-local.json"
  if (-not (Test-Path $f)) { return $false }
  return ((Get-Date) - (Get-Item $f).LastWriteTime).TotalMinutes -lt $StaleMinutes
}

# 維護模式：部署時 app 會被停掉幾分鐘去重建，watchdog 會在那個空檔把舊版拉
# 回來、鎖住 exe 讓 cargo build 失敗（實測 os error 5）。放這個檔就暫停檢查。
# 超過 PauseMaxMinutes 自動失效——忘了刪不會讓 watchdog 永久啞掉。
function Test-Paused {
  $f = Join-Path $env:USERPROFILE ".lobsterpulse\watchdog-pause"
  if (-not (Test-Path $f)) { return $false }
  if (((Get-Date) - (Get-Item $f).LastWriteTime).TotalMinutes -gt $PauseMaxMinutes) {
    Write-Log "維護模式檔已超過 ${PauseMaxMinutes} 分鐘，自動失效並刪除"
    Remove-Item $f -Force -ErrorAction SilentlyContinue
    return $false
  }
  return $true
}

# 重啟前清場：殺光本專案 exe 的所有實例（已判定不健康，安全），並等首選 port 的
# 死 socket 釋放。雙鍵匹配（名稱 + 完整路徑）避免誤傷別人的進程（硬規則 §6）。
function Clear-BeforeRestart {
  Get-Process lobster-pulse -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $Exe } |
    ForEach-Object {
      Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
      Write-Log "  清場：殺舊實例 PID=$($_.Id)"
    }
  # 等 $BasePort 上的 listener 釋放（最多 ~10s）
  for ($i = 0; $i -lt 20; $i++) {
    $c = Get-NetTCPConnection -LocalPort $BasePort -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $c) { return }  # 已釋放，收工
    $ownerPid = $c.OwningProcess
    if ($ownerPid -and $ownerPid -ne 0) {
      $p = Get-Process -Id $ownerPid -ErrorAction SilentlyContinue
      if ($p -and $p.Path -eq $Exe) {
        Stop-Process -Id $ownerPid -Force -ErrorAction SilentlyContinue
        Write-Log "  清場：port $BasePort 仍被本專案 PID=$ownerPid 佔用，補殺"
      } elseif ($p) {
        # 不是本專案的進程搶了 19280——不動它，讓 app 自己往 19281 退
        Write-Log "  port $BasePort 被非本專案 PID=$ownerPid ($($p.ProcessName)) 佔用，不干預"
        return
      }
      # $p 為空 = owner 已死、socket 殘留（死 socket）→ 續等它自然釋放
    }
    Start-Sleep -Milliseconds 500
  }
  Write-Log "  警告：port $BasePort 逾時未釋放，仍嘗試啟動（app 可能暫時漂到 19281）"
}

Write-Log "watchdog 啟動（每 ${IntervalSec}s 檢查，連續 ${FailuresBeforeRestart} 次失敗才重啟）"
$fails = 0
$staleLogged = $false
$pauseLogged = $false
while ($true) {
  if (Test-Paused) {
    if (-not $pauseLogged) { Write-Log "維護模式：暫停檢查"; $pauseLogged = $true }
    $fails = 0
    Start-Sleep -Seconds $IntervalSec
    continue
  }
  if ($pauseLogged) { Write-Log "維護模式結束，恢復檢查"; $pauseLogged = $false }
  if (Test-Healthy) {
    if ($fails -gt 0) { Write-Log "恢復正常（先前連續失敗 $fails 次）" }
    $fails = 0
    if (-not (Test-DataFresh)) {
      if (-not $staleLogged) {
        Write-Log "警告：healthz 正常但 usage-local.json 已超過 ${StaleMinutes} 分鐘沒更新——runner 鏈可能已死"
        $staleLogged = $true
      }
    } elseif ($staleLogged) {
      Write-Log "額度快照恢復更新"
      $staleLogged = $false
    }
  } else {
    $fails++
    Write-Log "healthz 失敗（第 $fails 次）"
    if ($fails -ge $FailuresBeforeRestart) {
      if (Test-Path $Exe) {
        Clear-BeforeRestart
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
