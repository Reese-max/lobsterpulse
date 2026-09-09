# LobsterPulse Product Board Audit

Date: 2026-09-09  
Audited default-branch SHA before this report: e4a2333349fb3ca7892a1c7f576303fc968f3672  
Scope: product discovery, current competitive intelligence, virtual executive board, 50 synthetic personas, switching test, red team, Quality Gate, GitHub Issue closure, regression and portfolio review.

> Every persona and preference result in this document is a synthetic model simulation, not human research, browser usability testing, market share, or a claim about real users. Runtime claims are made only where a concrete run/endpoint is cited.

## Executive Summary

LobsterPulse is a differentiated Windows-first, local desktop monitor for live AI coding-agent task state, waiting/completion cues, quota signals and diagnostics across four local CLIs plus a personalized OpenAB bot fleet. The repository is technically substantial and the latest default-branch Build is green, but the product contract is not yet trustworthy enough for broad adoption.

Three findings passed Quality Gate:

1. Existing Issue [#3](https://github.com/Reese-max/lobsterpulse/issues/3) is now **PARTIALLY FIXED / NEEDS_RUNTIME_VERIFICATION**. PR #4 is merged and main Build #24 is green, but no packaged app has proven false→true Codex enablement followed by a real Codex hook event.
2. New Issue [#5](https://github.com/Reese-max/lobsterpulse/issues/5) tracks the mismatch between the advertised 13-provider universe and a post-deadline KPI denominator that can exclude providers yet report zero structural gap.
3. New Issue [#6](https://github.com/Reese-max/lobsterpulse/issues/6) tracks the absence of any tag/Release or truthful source-only onboarding path.

Decision: **SIMPLIFY**. Preserve the live-state capsule, OpenAB integration, local privacy and Prometheus surface. Before adding providers or product surfaces, make support status truthful, ship one verifiable Windows acquisition path, and finish the packaged Codex runtime acceptance.

CEO—if resources only fund three things:

1. Finish #3 with packaged, disposable-home Codex runtime evidence.
2. Reconcile the 13-provider promise and KPI denominator through one versioned registry (#5).
3. Publish a minimal, verifiable Windows portable release or declare source-only (#6).

Do not build cloud SaaS, mobile, team collaboration, orchestration, billing, an IDE extension, or a “69 providers” race.

## Project Discovery

| Dimension | Assessment | Evidence |
|---|---|---|
| Product type | Windows-first local desktop observability utility for AI coding agents | **CONFIRMED:** README, MISSION, Tauri config |
| Maturity | Advanced internal prototype / pre-distribution beta | **CONFIRMED:** substantial code and CI; **CONFIRMED:** no tags/releases; **UNKNOWN:** clean-user runtime |
| Target user | Solo/power developer running several local CLIs and Reese-max OpenAB bots | **LIKELY:** product wording, custom provider order and bot IDs |
| Core job | Know which agent is working, waiting, idle, stale or dead without terminal switching | **CONFIRMED:** MISSION North Star and state machine |
| Value | One compact capsule, localized audio, normalized hooks, local-only integration, metrics | **CONFIRMED:** README/source |
| Current strength | Personalized OpenAB + local CLI live-state integration; not just quota reporting | **CONFIRMED:** 13 configured providers and hook/metrics architecture |
| Largest weakness | Product truth and distribution lag implementation breadth | **CONFIRMED:** #5 and #6 evidence |
| Security/privacy | Primarily local, but hook configuration mutates user files and requires conservative ownership/rollback | **CONFIRMED:** hooks configurator and #1/#3 history |
| CI | Build #24 succeeded on main across Linux/macOS/Windows | **CONFIRMED:** [run 34114733520](https://github.com/Reese-max/lobsterpulse/actions/runs/34114733520) |
| Tests | 23 hook tests per platform on the merged change; broader build succeeds | **CONFIRMED:** PR #4 receipts and main run |
| Runtime | Historical README claims launch; current audit did not execute app, CLI or real Codex | **UNKNOWN / NEEDS_RUNTIME_VERIFICATION** |
| Issues | #1 closed; #3 open; #5/#6 opened this round | **CONFIRMED:** GitHub |
| PRs/branches | No open PR. github-3 branch is a merged ancestor; other old branches have no active goal evidence | **CONFIRMED:** GitHub queries |
| Audit history | Two prior fixed-50-persona rounds in docs/audits | **CONFIRMED** |
| Release | Workflow exists; releases and tags APIs are empty | **CONFIRMED** |

### Evidence classification

- **CONFIRMED code/CI:** provider registry, hook mutation logic, build workflows, merge SHA, Build #24, empty release/tag collections.
- **LIKELY static inference:** new users cannot complete executable onboarding; compact status semantics can mislead when scope/freshness is hidden.
- **UNKNOWN until runtime:** actual packaged Windows launch, sidecar discovery, Codex event receipt, keyboard/screen-reader behavior, current health of each OpenAB bot.

## Competitive Intelligence

Freshness check: 2026-09-09. Primary sources are official repositories/sites.

Core capability matrix:

| Product | Target user / value | Core + killer feature | Onboarding / UX | Automation / AI | Integrations / API |
|---|---|---|---|---|---|
| **LobsterPulse** | Reese-max Windows/OpenAB power workflow; live task truth | 13 registered integrations, capsule, sounds, state machine, Prometheus | Source build only; localized 300px capsule | Passive monitor; no orchestration by design | 4 local hooks + 9 OpenAB IDs; local /metrics |
| [AgentPulse](https://github.com/yazelin/AgentPulse) | Cross-platform developers; live session state | Exact hook-event matrix for 4 assistants; Dynamic Island UI | Published Linux/macOS/Windows zip releases with steps | Passive monitor; Telegram notification optional | Local hooks and bundled sidecar |
| [CodexBar](https://github.com/steipete/CodexBar) | macOS users and CLI consumers planning around limits | Broad provider limits, reset countdowns, incident state | GitHub Releases, Homebrew, AUR/CLI tarballs; mature menu UI | Automated refresh/status polling, not agent orchestration | Many credential/data sources, CLI/config surfaces |
| [OpenUsage](https://github.com/robinebers/openusage) | macOS developers tracking subscriptions | Provider-grouped limits, pins, pace and stale-while-revalidate | DMG, Homebrew, signed/notarized Sparkle updates | Automatic refresh/cache | One-shot CLI + loopback HTTP API |
| [OpenUsage Community](https://github.com/openusage-community/openusage) | Linux/Windows users wanting quota UI | Cross-platform installers/portable builds and plugin providers | Windows installer/portable, AppImage/deb/rpm | Scheduled refresh/auto-update where supported | Plugin model, local API, proxy |
| [ccusage](https://ccusage.com/) | CLI/data users tracking tokens and estimated cost | Local-log multi-agent analysis, JSON and offline cache | One-command CLI flow | Batch analysis, no live task-state UI | Structured JSON; broad local log readers |

Business/quality matrix:

| Product | Mobile | Performance / reliability | Security / privacy | Pricing / source | Community / docs / distribution | Common strength / weakness |
|---|---|---|---|---|---|---|
| LobsterPulse | None; correct non-goal | Rust/Tauri, green matrix CI; runtime/release evidence incomplete | Local; config mutation requires care | Free hobby OSS | Rich internal docs, no stable release | Unique OpenAB live state; truth/distribution gap |
| AgentPulse | None | Native sidecar and timeout state machine; published builds | Local hooks; Telegram token stored locally if used | OSS | Exact provider/event docs; v0.5.3 assets published 2026-07-29 | Direct baseline and installable; fewer personalized bots |
| CodexBar | None | Native macOS app + CLI; frequent release v0.57.0 on 2026-09-08 | Reuses local sessions/keys; many sources increase trust surface | MIT/free | Large provider docs, Homebrew/AUR/Releases | Breadth and distribution; mostly usage, macOS GUI |
| OpenUsage | None | Native Swift, cache/refresh, v0.7.11 on 2026-09-05 | Provider-specific credential/privacy docs | Free OSS | DMG/Homebrew/auto-update and architecture docs | Strong quota UX; macOS 15+ only |
| OpenUsage Community | None | Tauri cross-platform; platform caveats documented | Local credentials; unsigned platform caveats disclosed | Free OSS | Installer/portable/packages; v0.6.38 on 2026-07-14 | Windows/Linux distribution; quota rather than live work state |
| ccusage | None | Fast local CLI; offline cached pricing | Reads local logs, no GUI credential aggregation | MIT/free | Strong web docs and machine output | Excellent analysis/export; not waiting/working monitor |

### Competitive Gap classification

- **MUST MATCH:** one obtainable/truthful install path; explicit current-vs-stale-vs-unavailable semantics; reproducible version/provenance.
- **SHOULD BE BETTER:** Windows/OpenAB live-task diagnosis, local audio cues, low-switching-cost capsule, Prometheus contract.
- **DIFFERENTIATOR:** OpenAB + local CLI normalized live task state in one capsule; keep this narrow.
- **DO NOT COPY:** provider-count arms race, cloud accounts, analytics, team dashboards, marketplace, mobile, autonomous orchestration, paid auto-update/signing before demand.

## Virtual Executive Board

| Role | Independent question | Opportunity / priority | Cross-review |
|---|---|---|---|
| CEO | Can a non-maintainer obtain the product and trust its all-clear? | #3, #5, #6 only | Reject feature expansion until these close |
| CPO | Is “13 providers” a user outcome or registry fact? | Separate registered/configured/live/quota support | Agrees with CTO; Growth must stop count marketing |
| CTO | Is there one source of truth across config, UI, metrics and docs? | Versioned capability registry | Avoid a second parallel registry |
| Staff/Principal Engineer | Can scope change without rewriting history? | Immutable receipts and validators | Merge symptoms by root cause into #5 |
| UX Lead | Does the 300px capsule explain stale/external/disabled? | Text/icon semantics, not color alone | Runtime test needed; no speculative a11y bug |
| UX Researcher | Which onboarding and state terms are understood? | 50-person comprehension benchmark, then human test if warranted | Synthetic evidence is directional only |
| Growth Lead | What is the shortest path to first value? | Working download→launch→one provider | Provider breadth is vanity before activation |
| CFO/Business Analyst | What maintenance cost is justified for a hobby tool? | Manual portable release; no paid infrastructure | Signing can wait; checksums cannot |
| Security/Privacy Lead | Does installation expand trust or mutate credentials unsafely? | Direct release, checksum, disposable config tests | Reject pipe-to-shell installer and extra credential probes |
| QA Lead | What proves #3? | Tagged package + isolated Codex home + real event | Unit/CI evidence is necessary but not sufficient |
| SRE Lead | Can metrics distinguish absence from health? | Freshness, owner, denominator, exclusion labels | “0 structural gap” is not an operational health signal |
| Accessibility Specialist | Can status and install flow work without hover/color/mouse? | Include keyboard/text/reduced-motion smoke in release receipt | No defect claim until runtime |
| Customer Support Lead | What are the top tickets? | “Where is download?”, “why no event?”, “is provider monitored?” | README should answer before adding support burden |

### Board disagreements

- Majority: **SIMPLIFY** to a trustworthy Windows-first live-state monitor.
- Minority: keep all 13 registered providers for roadmap visibility. Accepted only if “registered” is not represented as live or verified support.
- Minority: stay source-only to avoid release toil. Accepted only if onboarding is rewritten honestly and executable-first language is removed.
- Rejected: broaden to match CodexBar/OpenUsage. Their breadth is not LobsterPulse’s advantage.

## 50 Synthetic Personas

Coverage: 30 regression baseline personas (60%) + 20 rotating exploration personas (40%). Includes ages 17–63, first-time and experienced users, Windows/macOS/Linux, slow/offline/proxy networks, power users, maintainers, SRE/QA/support, and accessibility situations.

| ID / cohort | Background | Goal + expectation | Task + journey | Friction | Synthetic outcome + comment + severity | Suggestion | Scenario choice |
|---|---|---|---|---|---|---|---|
| B01 Baseline | 24，初次 Windows 開發者；中等熟練；Win11 筆電；家用網路 | 安裝後看 Claude/Codex 狀態；期待可下載程式 | 進 repo→找下載→啟動→啟用 provider | 沒有 Release，只看到 source build | FAIL；「第一步就不是產品流程」；P2 | 提供 Windows portable Release＋checksum | OpenUsage |
| B02 Baseline | 31，後端工程師；Power User；Linux 桌機；高速網路 | 同時看 4 個本機 CLI；期待跨平台 | 下載→設定 hooks→跑兩個 session→看 capsule | LobsterPulse 無可取得包；上游有明確壓縮包 | FAIL；轉向可立即執行方案；P2 | 誠實標 source-only 或發布包 | AgentPulse |
| B03 Baseline | 35，OpenAB 維運者；專家；Win11 工作站；LAN | 同時看 OpenAB＋本機 agent；期待單一狀態面 | 自行 build→連 OpenAB→查看工作/等待/死亡 | 13-provider 狀態分類與 freshness 不透明 | PARTIAL；差異化很強但信任不足；P2 | 顯示 live/stale/external/disabled | LobsterPulse |
| B04 Baseline | 28，全端工程師；熟練；Win11；穩定網路 | 任務完成時收到提示；期待不用切終端 | 自行 build→啟用 Codex→等待事件→看膠囊 | #3 packaged runtime 尚未驗證 | PARTIAL；程式碼證據夠、產品證據不足；P1 | 完成隔離 Codex home runtime receipt | LobsterPulse |
| B05 Baseline | 42，DevOps；專家；Ubuntu；公司代理 | 監控多 CLI 且能快速診斷；期待可攜 | 找 Linux artifact→設定→觀察 stale timeout | 本 repo 無 artifact；上游 release 路徑清楚 | FAIL；維護成本阻斷採用；P2 | 發布或收斂 Windows-only 承諾 | AgentPulse |
| B06 Baseline | 39，資深工程師；Power User；Win11；高速 | 看 9 個 OpenAB bot 與 4 CLI；期待一個 view | build→接入 bot→比較 /metrics 與 UI | KPI 可把 4 bot 排除仍報 0 gap | PARTIAL；最符合工作流但不能信 all-clear；P2 | 版本化 capability registry | LobsterPulse |
| B07 Baseline | 30，macOS iOS 工程師；熟練；光纖 | 看配額與 reset；期待 menu bar 安裝即用 | brew/install→登入既有 provider→看 quota | LobsterPulse Windows 主力且無 release | FAIL；平台與配額需求更匹配競品；P2 | 不要擴 macOS，維持 Windows 聚焦 | CodexBar |
| B08 Baseline | 26，CLI 愛好者；專家；Linux；離線時多 | 查每日 token/cost；期待純 CLI/JSON | npx→讀本機 logs→匯出 JSON | LobsterPulse 以 GUI/live state 為主 | SUCCESS 替代方案；非缺陷；P3 | 保留 Prometheus，不複製完整 cost CLI | ccusage |
| B09 Baseline | 33，QA 工程師；熟練；Win11 VM；受限網路 | 驗證 hooks 狀態機；期待可重現安裝 | 下載固定版→測 working/waiting/stale→回滾 | 無 tag、checksum、前版下載 | FAIL；無法建立版本矩陣；P2 | tagged release＋回滾說明 | AgentPulse |
| B10 Baseline | 45，技術主管；中高熟練；Win11；公司網路 | 5 個 agent 一眼看狀態；期待低打擾 | 安裝→啟用常用 provider→工作中 glance | 需自行建置；13 的含義不清 | PARTIAL；核心價值明確；P2 | 先做 install 與 truth labels | LobsterPulse |
| B11 Baseline | 29，AI 研究工程師；Power User；macOS；高速 | 管理多家配額；期待 provider breadth | 啟用十多 provider→看 reset/incident | LobsterPulse 13 但多為內部 OpenAB 名稱 | FAIL；廣度不是此產品的 moat；P3 | 不要追 69 providers | CodexBar |
| B12 Baseline | 37，獨立開發者；熟練；Win11；行動熱點 | 離桌也能聽到等待提示；期待本機隱私 | build→配 sounds→跑 Codex/Claude→離桌 | packaged sidecar 與真實 Codex event 未證 | PARTIAL；本機提示很有吸引力；P1 | 完成真實 hook smoke | LobsterPulse |
| B13 Baseline | 34，產品工程師；中等；macOS；穩定 | 查看週額度與花費；期待 Homebrew | 找安裝→啟用 provider→看 reset | LobsterPulse 不提供 brew/DMG | FAIL；選成熟分發方案；P2 | 不必做 Homebrew，先誠實 Windows release | OpenUsage |
| B14 Baseline | 22，學生；中等；Linux 舊機；校園網路 | 控制免費額度；期待低資源 CLI | 安裝→掃 log→看 daily cost | 桌面 WebView 與 build 依賴較重 | SUCCESS 替代方案；非核心缺陷；P3 | 保留輕量 metrics export | ccusage |
| B15 Baseline | 41，Windows 自動化工程師；專家；Win11；LAN | OpenAB bot 出錯時快速定位；期待 metrics | 啟動 app→scrape /metrics→對照 bot 狀態 | provider freshness/owner 未進統一契約 | PARTIAL；Prometheus 是潛在 moat；P2 | registry 驅動 metrics label | LobsterPulse |
| B16 Baseline | 36，跨平台維護者；專家；三 OS；高速 | 驗證 release matrix；期待每平台 artifact | 下載三平台包→smoke→比較行為 | 本 repo Build 有 artifact 但沒有穩定 release | FAIL；選上游基線；P2 | 首次 release 前先定義支援層級 | AgentPulse |
| B17 Baseline | 32，Copilot＋Codex 使用者；熟練；Win11；穩定 | 完成/等待即時提示；期待零資料上傳 | build→啟用兩 CLI→工作→收提示 | 安裝摩擦與 #3 runtime pending | PARTIAL；需求高度貼合；P1 | release 後做 disposable-home smoke | LobsterPulse |
| B18 Baseline | 48，顧問；中等；macOS；飯店 Wi‑Fi | 快速看限額；期待 signed app | 下載→開啟→查看 quota | LobsterPulse 無包且非 macOS 主力 | FAIL；安全提示與安裝成本較高；P2 | 不要承諾簽章，標示風險 | CodexBar |
| B19 Baseline | 27，前端工程師；熟練；macOS；高速 | 一眼看 Claude/Codex 額度；期待自動更新 | brew→啟動→固定 meter | LobsterPulse 聚焦 task state 而非成熟 quota UX | FAIL；選 quota 專長產品；P3 | 保持 live-state 差異化 | OpenUsage |
| B20 Baseline | 44，SRE；專家；Win11；公司 LAN | 建立 agent 健康告警；期待 Prometheus | 啟動 exporter→抓 metrics→設 stale alert | K0 將外部/永久 skip 混成 0 gap | PARTIAL；metrics 可整合但語義需修；P2 | 明確 freshness/denominator | LobsterPulse |
| B21 Baseline | 25，初階工程師；低中熟練；Win11；家用 | 想知道 agent 是否卡住；期待雙擊就用 | 找 exe→雙擊→看 working/stale | repo 沒 exe/installer | FAIL；回到有 release 的上游；P2 | README 第一 CTA 必須可完成 | AgentPulse |
| B22 Baseline | 38，資料工程師；專家；Linux；離線 | 把使用量餵進腳本；期待 JSON | 執行 CLI→輸出 JSON→存報表 | LobsterPulse metrics 偏即時狀態，不是歷史成本 | SUCCESS 替代方案；非缺陷；P3 | 勿建重型歷史資料倉 | ccusage |
| B23 Baseline | 30，繁中 Windows 開發者；熟練；Win11；穩定 | 要中文、聲音與自訂順序；期待本地化 | build→設定中文→同跑四 CLI | 沒有發布包；部分 provider 宣稱不易理解 | PARTIAL；本地化形成偏好；P2 | Windows-first release＋中文 status legend | LobsterPulse |
| B24 Baseline | 52，工程經理；中等；macOS；公司網路 | 看團隊工具額度；期待穩定來源 | 安裝→連 providers→看 status incidents | LobsterPulse 不做團隊/雲端且無 macOS 發布 | FAIL；產品定位本就不匹配；P3 | 堅守 non-goal | CodexBar |
| B25 Baseline | 33，安全工程師；專家；macOS；受管控 | 本機讀既有登入；期待簽章與最小權限 | 驗證下載→啟用一 provider→看網路行為 | LobsterPulse 無 provenance/release receipt | FAIL；供應鏈證據不足；P2 | checksum＋release receipt，暫不 pipe-to-shell | OpenUsage |
| B26 Baseline | 21，研究生；中等；Linux；校園離線 | 估算模型成本；期待不用 GUI | 讀本機 logs→日/週表→離線快取 | LobsterPulse 核心不是成本分析 | SUCCESS 替代方案；非缺陷；P3 | Prometheus export 足夠 | ccusage |
| B27 Baseline | 40，企業桌面支援；熟練；Win11；受限 | 大量機器部署；期待固定版與 rollback | 取得版本→驗 checksum→安裝→移除 | 無 release，且版本字串 0.2.2/0.5.4/v5.1 不一致 | FAIL；無法支援；P2 | 單一版本契約＋portable bundle | AgentPulse |
| B28 Baseline | 34，CLI 平台工程師；專家；Linux；高速 | 機器可讀 usage；期待 JSON 與無頭模式 | 執行→解析 JSON→CI gate | LobsterPulse exporter 可用但桌面啟動是前提 | PARTIAL；選專用 CLI；P3 | 不要加入通用 CI 成本平台 | ccusage |
| B29 Baseline | 46，OpenAB owner；專家；Win11；LAN | 九 bot＋四 CLI 一起看；期待內部 bot 命名 | 接 OpenAB snapshots→看狀態→診斷 | 四 bot 永久 skip 卻仍列 13 | PARTIAL；唯一直接覆蓋內部工作流；P2 | 把 registered 與 monitored 分開 | LobsterPulse |
| B30 Baseline | 27，macOS 工程師；熟練；光纖 | 看 provider incident 與 reset；期待成熟更新 | 安裝→啟用→看事件 badge | LobsterPulse 無 release/incident distribution UX | FAIL；選成熟可裝工具；P2 | 不抄 incident breadth，先發佈 | CodexBar |
| E01 Rotate | 56，低數位熟練維護者；Win11；慢網 | 確認 agent 是否完成；期待明確步驟 | README→下載→啟動→辨識顏色 | 沒有下載；必須裝 toolchain | FAIL；認知負擔過高；P2 | 提供一步一圖的 portable 安裝 | AgentPulse |
| E02 Rotate | 19，色弱學生；中等；Win11；宿舍網 | 同跑 Claude/Codex；期待非純色狀態 | build→看 capsule→辨識 working/waiting | PRODUCT 要求非純 hover/顏色，但未做 runtime a11y | UNKNOWN；不捏造缺陷；P3 | release 後鍵盤/文字標籤驗收 | LobsterPulse |
| E03 Rotate | 63，視力低弱資深顧問；低熟練；macOS；穩定 | 看放大後 quota；期待系統級可讀 | 安裝→放大→VoiceOver→查 reset | 300px 固定膠囊可能擁擠，未 runtime 證實 | UNKNOWN；需真機驗證；P3 | 研究而非直接報 bug | CodexBar |
| E04 Rotate | 38，單手鍵盤使用者；熟練；macOS；高速 | 鍵盤開關 popover；期待 global shortcut | brew→設定快捷鍵→鍵盤瀏覽 | LobsterPulse 主界面互動尚未實測 | FAIL 相對成熟方案；P3 | 將 keyboard smoke 納入 release receipt | OpenUsage |
| E05 Rotate | 29，遠端值班工程師；專家；Win11；4G | 在低頻寬下看 agent 健康；期待本機運作 | portable 啟動→離線 hooks→查看 stale | 本地架構合適但沒有 portable 包 | PARTIAL；分發修好即可勝；P2 | 發布小型 zip＋離線說明 | LobsterPulse |
| E06 Rotate | 47，IT 稽核員；專家；Win11；隔離網 | 驗供應鏈與版本；期待 checksum | 下載→核 hash→保存 SBOM/notes→安裝 | 無 tag/release/checksum | FAIL；上游至少有不可變版號資產；P2 | checksum＋tagged SHA receipt | AgentPulse |
| E07 Rotate | 25，多 provider 自由工作者；熟練；macOS；高速 | 看 10+ 額度窗口；期待廣覆蓋 | 啟用 providers→比較 reset→規劃任務 | LobsterPulse 的 OpenAB 名單偏個人化 | FAIL；選 provider breadth；P3 | 不要追廣度，服務 Windows/OpenAB niche | CodexBar |
| E08 Rotate | 43，夜班 Windows 工程師；中等；Win11；穩定 | 用聲音分辨等待/完成；期待繁中提示 | 啟動→配聲音→離開桌面→回應 | 聲音功能符合但 package runtime 未驗 | PARTIAL；差異化高；P1 | 完成聲音/等待真機 smoke | LobsterPulse |
| E09 Rotate | 32，隱私敏感研究者；專家；macOS；代理 | 不把 log 上傳；期待資料來源文件 | 安裝→檢視 provider credential 路徑→使用 | LobsterPulse 本地導向佳但文件/發布不足 | PARTIAL；競品隱私文件更完整；P2 | 記錄每 provider 資料/網路邊界 | OpenUsage |
| E10 Rotate | 17，低階 Linux Chromebook 使用者；初學；慢網 | 查免費 token；期待低資源 | npx→讀 log→純文字結果 | 桌面 Tauri 不適合環境 | SUCCESS 替代方案；非缺陷；P3 | 不做 Chromebook/PWA | ccusage |
| E11 Rotate | 36，雙螢幕 Power User；Win11；LAN | 膠囊固定置頂看 5 agent；期待可拖曳 | build→拖到副螢幕→多 session→診斷 | 無發佈與 packaged multi-monitor 驗收 | PARTIAL；核心形態最吻合；P2 | clean Windows 多螢幕 smoke | LobsterPulse |
| E12 Rotate | 50，跨平台開源維護者；專家；三 OS；高速 | 比較 hook 兼容；期待可重現 release | 抓 tag→跑 fixtures→回報 | 本 repo 沒 tag；上游有 release | FAIL；貢獻入口不穩定；P2 | 先定 canonical version | AgentPulse |
| E13 Rotate | 28，財務敏感創業者；中等；macOS；穩定 | 避免超額；期待 cost/pace | 安裝→看週額度→調整工作 | LobsterPulse 的首要價值是 task state | FAIL；選 quota/cost 專長；P3 | 不建計費 SaaS | OpenUsage |
| E14 Rotate | 41，Linux 平台主管；專家；伺服器網 | CLI 取得跨 provider 限額；期待無 GUI | 下載 CLI→JSON→告警 | LobsterPulse 必須啟桌面 app | FAIL；選有 CLI tarball 的工具；P3 | 保留 /metrics 作整合點 | CodexBar |
| E15 Rotate | 23，繁中 QA；熟練；Win11 VM；高速 | 驗 false→true hooks 回歸；期待產品證據 | 準備 disposable home→安裝→啟用→真實 Codex event | main CI 綠但 packaged path 未跑 | PARTIAL；可直接協助關 #3；P1 | 執行五步 runtime acceptance | LobsterPulse |
| E16 Rotate | 58，資料分析師；中等；Linux；離線 | 輸出月成本表；期待 CSV/JSON | 讀 logs→匯出→比較月份 | LobsterPulse 無此核心 journey | SUCCESS 替代方案；非缺陷；P3 | 拒絕加入報表倉儲 | ccusage |
| E17 Rotate | 34，代理網路下的 macOS 開發者；熟練；公司網 | 多 provider 刷新；期待 proxy 與 cache | 設定 proxy→啟用→看 stale-while-revalidate | LobsterPulse OpenAB/local 模式不同且無安裝 | FAIL；選已有 proxy/docs 的方案；P3 | 只在核心需要時補 proxy | OpenUsage |
| E18 Rotate | 45，值班 SRE；專家；Win11；LAN | Prometheus 告警 agent stale/dead；期待語義穩定 | scrape→建立 alert→scope change→比較歷史 | KPI scope 可變、歷史判讀失真 | PARTIAL；修 #5 後是強差異化；P2 | 不可變 receipt＋狀態 label | LobsterPulse |
| E19 Rotate | 20，開源貢獻者；中等；Ubuntu；高速 | 快速重現 bug；期待 release/source 對照 | 下載版→checkout tag→測試→PR | 無 release/tag，難對照使用者環境 | FAIL；上游 contribution loop 較清楚；P2 | 發布對應 tag/notes | AgentPulse |
| E20 Rotate | 39，無障礙 QA；專家；macOS；穩定 | 驗鍵盤、reduced motion、對比；期待可安裝 build | 下載→VoiceOver→reduce motion→狹窄版面 | LobsterPulse 無發布包，無法完成產品層驗收 | FAIL；選可取得 app；P2 | 把 a11y smoke 放入 release gate | CodexBar |

## Competitor Switching Test

Synthetic preference share (scenario choices, n=50; not real market share or survey):

| Choice | Personas | Share | Main reason |
|---|---:|---:|---|
| LobsterPulse | 16 | 32% | Best fit for Windows/OpenAB live task state, sounds and Prometheus |
| AgentPulse | 10 | 20% | Installable cross-platform baseline with explicit hook/release docs |
| CodexBar | 9 | 18% | Mature quota/reset/incident UX and distribution |
| OpenUsage | 8 | 16% | Strong install, privacy/source docs and quota experience |
| ccusage | 7 | 14% | Fast local CLI, JSON, historical token/cost analysis |

Interpretation: LobsterPulse can win its narrow niche, but #5/#6 suppress trust and activation. This is a model result, not a demand estimate.

## Red Team

1. **Persona bias:** the set overweights technical users able to understand hooks and metrics; real first-time failure may be worse.
2. **Competitor selection risk:** CodexBar/OpenUsage emphasize usage limits, while LobsterPulse emphasizes live task state. They are partial substitutes, not identical.
3. **Confirmation bias:** a personalized OpenAB fit can make the product appear broadly differentiated when most public users do not have those bot IDs.
4. **Overengineering:** #5 must reuse existing provider inventory and guards, not create a governance platform.
5. **Feature bloat:** a release Issue does not justify auto-update, signing service, package managers or telemetry.
6. **Copying competitors:** do not match provider counts or cloud integrations.
7. **Growth excess:** download counts and provider totals are not the current goal; successful first launch and truthful state are.
8. **Delete/simplify option:** relabel unsupported/internal providers, retire dead IDs, shorten MISSION history and link immutable receipts.
9. **Runtime humility:** green CI does not prove the packaged UI, OS path discovery or real Codex hook.
10. **Minority counterclaim:** the 13-provider text might be intended as a registry inventory. Even so, the wording must say that explicitly; #5 remains valid.

## Findings and Quality Gate

| ID | Type | Priority | Impact | Strategic value | Competitive gap | Risk reduction | Confidence | Effort | Gate / mapping |
|---|---|---|---:|---:|---:|---:|---|---|---|
| F1 | BUG / RELIABILITY | P1 | 5 | 4 | 3 | 5 | High static, runtime pending | Low–Med | Distinct/actionable; UPDATED #3 |
| F2 | GROWTH / RELIABILITY / DOCUMENTATION | P2 | 4 | 5 | 4 | 4 | High | Medium | Distinct/actionable; NEW #5 |
| F3 | UX / DOCUMENTATION / COMPETITIVE_GAP | P2 | 4 | 4 | 5 | 3 | High | Low–Med | Distinct/actionable; NEW #6 |

Quality Gate checked Evidence, Distinctness, Actionability, Impact, Acceptance Criteria, Duplicate Check and Confidence for each. Mapping completeness: **3/3**.

### Stable fingerprints

- F1: hooks/config + enable with existing false + success while effective feature remains disabled + substring/non-semantic mutation; existing #3.
- F2: provider capability/K0 + post-deadline evaluation + advertised providers excluded while zero gap reported + no versioned immutable denominator; new #5.
- F3: desktop distribution + new-user install + no obtainable binary/source-only contract + release workflow never exercised; new #6.

## GitHub Issue Mapping

- **Updated existing:** [lobsterpulse #3](https://github.com/Reese-max/lobsterpulse/issues/3) — PARTIALLY FIXED / NEEDS_RUNTIME_VERIFICATION.
- **New:** [lobsterpulse #5](https://github.com/Reese-max/lobsterpulse/issues/5) — provider promise/K0 denominator truth.
- **New:** [lobsterpulse #6](https://github.com/Reese-max/lobsterpulse/issues/6) — verifiable installation path.
- **Reopened:** none.
- **Research:** none.
- **Issue write blocked:** none.

## Priority and Roadmap

### NOW

- FIX #3 packaged Codex acceptance.
- SIMPLIFY #5 capability/support truth.
- IMPROVE #6 acquisition path with the smallest sufficient release or source-only contract.

### NEXT

- Generate support legend/receipts from the canonical provider registry.
- Add clean Windows install/rollback and keyboard/text-status smoke.
- Make OpenAB ownership and freshness explicit.

### LATER

- Consider code signing and automatic updates only if active use warrants recurring cost.
- Consider provider-level diagnostic drill-down only after state truth is stable.

### DON'T

- Cloud dashboard/SaaS, mobile, IDE extensions, team accounts, orchestration, billing, analytics, marketplace, public provider-count race.

Priority order applied: REMOVE/SIMPLIFY → FIX → IMPROVE → ADD.

## Regression

Issue #3 status: **PARTIALLY FIXED / NEEDS_RUNTIME_VERIFICATION**.

Confirmed:

- PR #4 merged to main at e4a2333 on 2026-09-07.
- Main Build #24 succeeded on Linux/macOS/Windows.
- Static fixtures cover false/absent/true, dotted/inline/multiline, malformed/duplicate and lifecycle cases.

Not enough for VERIFIED FIXED:

- No packaged binary run with a disposable Codex home.
- No real Codex-generated hook event observed through LobsterPulse.
- Windows path/sidecar resolution not proven in the final package.

Issue #1 remains closed; this audit found no evidence that its destructive overwrite root cause regressed. It is not re-opened.

## Runtime Pending

1. Packaged Windows clean install and sidecar discovery.
2. Disposable Codex home false→true semantic verification.
3. Real Codex session event through hook→server→state UI.
4. Provider-state matrix: live, idle, stale, disabled, external/unavailable.
5. Keyboard, text-label, contrast and reduced-motion smoke.
6. Release download/checksum/upgrade/rollback journey.

## Delta from Prior LobsterPulse Audit

- #3 moved from branch-level READY_FOR_MERGE to merged-main/green-CI PARTIALLY FIXED.
- Default-branch verification blocker is cleared.
- Packaged runtime blocker remains.
- Competitive/product-board scope added two root findings: support/KPI truth (#5) and distribution/onboarding (#6).
- No product source, release, deployment, secrets, permissions or settings were changed.

## Decision Memo

- **What this product should become:** the most trustworthy Windows-first local live-state monitor for a small, explicit set of local/OpenAB coding agents.
- **Who it should serve:** Reese-max and similar solo power developers running multiple agents who need working/waiting/dead truth without terminal switching.
- **Why users would choose it:** OpenAB integration, Traditional Chinese workflow, audio cues, compact always-on-top capsule, local privacy and Prometheus.
- **Why users choose competitors:** they can download them; support/data sources are clearer; quota/reset/history UX is more mature.
- **Biggest competitive gaps:** acquisition, provenance, truthful support/freshness semantics, packaged runtime receipts.
- **Potential moat:** normalized live task-state signals across personalized OpenAB bots and local CLIs, not provider count.
- **Top priorities:** #3, #5, #6.
- **What NOT to build:** cloud, collaboration, mobile, IDE, orchestration, marketplace, general cost platform.
- **Features worth removing/simplifying:** remove ambiguous “13 monitored” and “0 gap” language; collapse historical KPI prose into immutable receipts.
- **Biggest risks:** false confidence, setup failure, internal-only personalization, release toil, user config mutation.
- **Next experiments:** one Windows portable release smoke; one 50-person state-label comprehension rerun; one isolated Codex event receipt.
- **Decision:** **SIMPLIFY** — invest only after product truth and first-run completion are proven.

## Portfolio CEO Review

This is a provisional portfolio view using the existing portfolio baseline; only LobsterPulse received a fresh deep audit in this run.

Shared/merge opportunities:

- cf-ai-router should remain the shared AI gateway rather than each product creating provider routing.
- autodev-ng should remain the shared agent/Issue governance layer; audit repos should reuse one fingerprint/receipt schema.
- taiwan-intel-dashboard and taichung-police-intel should share provenance, freshness and evidence-status components where schemas overlap.
- police-exam-practice should remain a compatibility/fusion entry to police-exam-archive, not rebuild the exam product.
- chatgpt-dual-pipeline and obsidian-vault can share publish-state and portable-path validation patterns without merging user data.
- A lightweight design/status vocabulary can be shared; do not create a standalone design-system product yet.

Provisional ranking by actionable product value and current evidence:

1. taiwan-intel-dashboard — INVEST after reliability truth gates.
2. UkePack — INVEST in teacher-safe core workflow.
3. LobsterPulse — SIMPLIFY; differentiated but not distributable/truthful enough.
4. cf-ai-router — MAINTAIN/strategic research.
5. tick-stock-panel — MAINTAIN and clarify coverage.
6. avatar-vfo — MAINTAIN pending production/runtime closure.
7. chatgpt-dual-pipeline — SIMPLIFY publish gates.
8. obsidian-vault — SIMPLIFY security/recovery.
9. police-exam-practice — MAINTAIN as compatibility layer.
10. gooaye — ARCHIVE unless a distinct user/task/value is defined.

## Mandatory End-of-Run Verification

- **Total Findings:** 3
- **New Issues Created:** 2
  - [Reese-max/lobsterpulse #5 — Make the 13-provider promise and K0 denominator truthful](https://github.com/Reese-max/lobsterpulse/issues/5)
  - [Reese-max/lobsterpulse #6 — Provide a truthful, verifiable desktop installation path](https://github.com/Reese-max/lobsterpulse/issues/6)
- **Updated Existing Issues:** 1
  - [Reese-max/lobsterpulse #3](https://github.com/Reese-max/lobsterpulse/issues/3)
- **Reopened Issues:** 0
- **Research Issues:** 0
- **Duplicate Avoided:** 11 symptom candidates consolidated into #3/#5/#6; version mismatch remains acceptance evidence in #6 rather than a separate Issue.
- **Issue Write Blocked:** 0
- **SKIPPED_LOCKED:** 0 for LobsterPulse; no open PR or active autodev goal won an earlier lease.
- **Verified Fixed:** 0
- **Priority distribution:** P0 0 / P1 1 / P2 2 / P3 0 / STRATEGIC 0
- **Highest Priority:** #3, then #5 and #6
- **Finding mapping:** 3/3 complete.

Rejected findings (12):

1. Add more providers — feature bloat before support truth.
2. Match CodexBar’s provider count — copies a competitor without niche value.
3. Cloud dashboard/SaaS — explicit non-goal and privacy/ops burden.
4. Mobile app — no core evidence.
5. Team collaboration/accounts — unsupported market expansion.
6. Agent orchestration/routing — belongs outside a monitor.
7. Paid plan/monetization — explicit hobby/non-goal.
8. IDE extension — explicit non-goal.
9. Immediate auto-update/signing program — premature recurring cost; not required for first release.
10. Security defect from broad Tauri CSP alone — no exploit/runtime evidence; keep as future hardening review.
11. Accessibility defect from fixed 300px UI — static concern only; requires runtime verification.
12. Mark #3 VERIFIED FIXED from green CI — rejected because packaged Codex runtime evidence is missing.

## Sources

- LobsterPulse [README](https://github.com/Reese-max/lobsterpulse/blob/main/README.md), [MISSION](https://github.com/Reese-max/lobsterpulse/blob/main/MISSION.md), [release workflow](https://github.com/Reese-max/lobsterpulse/blob/main/.github/workflows/release.yml), [Build #24](https://github.com/Reese-max/lobsterpulse/actions/runs/34114733520)
- [AgentPulse](https://github.com/yazelin/AgentPulse), [AgentPulse v0.5.3](https://github.com/yazelin/AgentPulse/releases/tag/v0.5.3)
- [CodexBar](https://github.com/steipete/CodexBar), [CodexBar v0.57.0](https://github.com/steipete/CodexBar/releases/tag/v0.57.0)
- [OpenUsage](https://github.com/robinebers/openusage), [OpenUsage v0.7.11](https://github.com/robinebers/openusage/releases/tag/v0.7.11)
- [OpenUsage Community](https://github.com/openusage-community/openusage)
- [ccusage](https://ccusage.com/)
