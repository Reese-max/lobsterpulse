# LobsterPulse Product Board Audit

Date: 2026-09-14 04:10 Asia/Taipei  
Audited default-branch SHA before this report: 5c09da0cf7b01fcfd80c60ee2a529ae650aed265  
Scope: incremental product discovery, current competitive intelligence, 13-role virtual executive board, 50 synthetic personas, competitor switching test, Red Team, Quality Gate, GitHub Issue closure, regression review, and portfolio decision memo.

> All personas and preference shares are synthetic model simulations, not human research, market share, browser/CLI/API testing, or claims about real users. Runtime claims appear only where a concrete repository or CI receipt exists.

## Executive Summary

LobsterPulse remains a differentiated local desktop monitor for live AI coding-agent state: a Traditional-Chinese Tauri capsule, per-provider sounds, OpenAB plus local CLI normalization, quota/status data, and a local Prometheus endpoint. Its primary weakness is no longer feature count; it is lifecycle truth across product, distribution, provider coverage, and metrics.

One new root-cause finding passed Quality Gate:

1. [Issue #11](https://github.com/Reese-max/lobsterpulse/issues/11) — the six-counter Prometheus migration publicly scheduled for 2026-07-03 is still locked in T-1 dual emit. Code emits all legacy and replacement series, tests require 47 metrics and both names, while the closed OpenSpec explicitly leaves T-2 through T-5 unfinished.

Decision: **INVEST / SIMPLIFY**. Finish truthful contracts before adding providers, telemetry families, SaaS surfaces, or autonomous actions.

CEO — if resources only fund three things:

1. Close the product’s real runtime gate: #3 packaged Codex enablement/config-preservation evidence.
2. Make distribution and provider truth real: #6 and #5, without provider-count marketing.
3. Resolve #11 by choosing a documented CUTOVER or POSTPONE/CANCEL path and enforcing it across docs, exporter, tests, and receipts.

Do not build cloud accounts, mobile, team collaboration, billing, an IDE extension, broad agent orchestration, or another observability dashboard.

## Project Discovery

| Dimension | Assessment | Evidence |
|---|---|---|
| Product type | Local desktop observability utility for AI coding agents | **CONFIRMED:** README, Tauri/Rust source, local hook sidecar and Prometheus exporter |
| Maturity | Advanced internal prototype / pre-distribution beta | **CONFIRMED:** substantial source and build CI; **CONFIRMED:** #6 says no stable download; **UNKNOWN:** clean-user runtime |
| Target users | Reese-max/OpenAB power users and Windows developers running several coding CLIs | **LIKELY:** product language, provider order, custom bot IDs and Traditional-Chinese UI |
| Core task | Know which agent is working, waiting, stale, failed or complete without switching terminals | **CONFIRMED:** README, event normalization, sounds, capsule state |
| Value | Personalized OpenAB coverage plus local CLI events, low-glance UI and machine-readable metrics | **CONFIRMED:** provider registry, hook server, /metrics |
| Main strength | Live task-state integration rather than quota-only reporting | **CONFIRMED:** state/event code and direct competitors |
| Main weakness | Governance declares partial phases closed while runtime/product contracts remain unresolved | **CONFIRMED:** #5, #6, #11 and OpenSpec metadata |
| Manifests | Node/Tauri frontend plus Rust backend; versions remain separately expressed | **CONFIRMED:** package.json, Cargo.toml and Tauri configuration |
| CI | Default branch Build previously succeeded; current report commit has no receipt yet | **CONFIRMED:** repository workflow history; do not infer report commit success |
| Tests | Large Rust contract suite includes Prometheus completeness and dual-value guards | **CONFIRMED:** lib.rs and OpenSpec |
| Issues | #1 closed; #3/#5/#6/#9/#11 open | **CONFIRMED:** GitHub issue index |
| Open PRs | #7 provider truth, #8 install path, #10 attention design | **CONFIRMED:** GitHub; excluded from writes |
| Branches | Dedicated branches exist for #3/#5/#6/#9 scopes | **CONFIRMED:** branch search; no branch existed for #11 before creation |
| Recent change | main head is the previous Round-3 audit commit | **CONFIRMED:** commit history |
| Runtime | No packaged app, live CLI event, Prometheus server, Grafana, screen reader or real operator session was run in this audit | **UNKNOWN / NEEDS_RUNTIME_VERIFICATION** |

### Evidence boundaries

- **CONFIRMED code/CI:** six dual-emit pairs, 47-name contract, passing prior build, past-due public date, closed phase-one OpenSpec with unfinished successors, open PR/branch state.
- **LIKELY static inference:** duplicate per-provider series create avoidable storage/query ambiguity; operators will not know which name is canonical.
- **UNKNOWN until runtime:** external scraper/dashboard/alert consumers, packaged launch, current provider health, real notification fatigue, keyboard/screen-reader outcomes.

## Competitive Intelligence

Freshness check: 2026-09-13 UTC. Primary sources were re-read on that date.

### Capability matrix

| Product | Target user / value | Core / killer feature | Onboarding / UX | Automation / AI | Integrations / API | Mobile |
|---|---|---|---|---|---|---|
| **LobsterPulse** | Reese-max/OpenAB and Windows multi-agent users; live state | 13 registered integrations, local capsule, sounds, Prometheus | Source-build/current PR-driven onboarding; Traditional Chinese | Passive monitoring; attention queue only research | Local hooks, OpenAB events, loopback metrics | None; correct non-goal |
| [AgentPulse](https://github.com/yazelin/AgentPulse) | Cross-platform developers; live session state | Exact four-assistant hook-event matrix and normalized sidecar | Published zip flow and explicit first launch | Passive monitor; optional Telegram notification | Claude, Antigravity, Codex, Copilot hooks | None |
| [CodexBar](https://github.com/steipete/CodexBar) | macOS users managing many AI tools | Very broad provider quota/usage/status menu | Mature provider settings and menu-bar UX | Refresh, incident and usage polling | Many provider-specific auth/data sources; CLI/config | None |
| [OpenUsage](https://github.com/robinebers/openusage) | macOS subscription users | Provider limits, pins, pace and stale-while-revalidate | Homebrew/DMG, signed updates, detailed docs | Five-minute refresh/cache | One-shot CLI and loopback HTTP API | None |
| [ccusage](https://ccusage.com/) | CLI/data users | Local multi-agent token/cost analysis | CLI-first, structured output, offline cache | Batch analysis | JSON output; many local log readers | Terminal only |
| Prometheus + Grafana | SREs wanting general monitoring | Open ecosystem, PromQL, alerts/dashboards | High operator setup | Rules/alerts, not agent semantics | Universal metrics ecosystem | Web responsive, not native product |

### Operating model matrix

| Product | Performance / reliability | Security / privacy | Pricing / source | Community / docs / distribution | Common strength | Common weakness |
|---|---|---|---|---|---|---|
| LobsterPulse | Rust/Tauri and strong contract tests; runtime/release evidence incomplete | Primarily local; config mutation requires conservative ownership | Free/open repository | Rich internal specs; product delivery still incomplete | OpenAB live-state differentiation | Truth and lifecycle drift |
| AgentPulse | Native sidecar, explicit timeout/event normalization | Local hooks; optional Telegram adds bounded external surface | MIT/open | Exact provider-event docs and downloadable archives | Installable direct baseline | Fewer custom providers |
| CodexBar | Native menu app with extensive polling/status handling | Many auth sources expand trust surface | MIT/open | Frequent docs/releases and authoring guidance | Breadth and mature usage UX | macOS-heavy and larger maintenance surface |
| OpenUsage | Native Swift, caching and stable local API | Provider-specific credential/privacy docs | MIT/open | Homebrew/DMG/signed updates | Clear freshness and distribution | macOS 15+ |
| ccusage | Fast local CLI and offline pricing cache | Reads local logs without cloud upload by default | MIT/open | Web docs, package distribution, JSON contract | Automation/data portability | Not a live waiting/working monitor |
| Prometheus + Grafana | Battle-tested query/alert ecosystem | Deployment-dependent | Open core | Very mature docs/community | General integration and alerts | Too heavy for a simple personal agent capsule |

Evidence highlights:
- Prometheus says accumulating counts use the total suffix and exporter counters should use _total: [naming](https://prometheus.io/docs/practices/naming/), [exporter guidance](https://prometheus.io/docs/instrumenting/writing_exporters/).
- AgentPulse publishes exact provider/hook events and cross-platform zip instructions: [AgentPulse](https://github.com/yazelin/AgentPulse).
- OpenUsage documents supported providers, stale-while-revalidate behavior, CLI and loopback API: [OpenUsage](https://github.com/robinebers/openusage).
- CodexBar documents provider-specific sources and visible stale/error behavior: [CodexBar](https://github.com/steipete/CodexBar).
- ccusage names its local sources and stable JSON/offline paths: [ccusage](https://ccusage.com/).

### Competitive gap classification

- **MUST MATCH:** truthful install path; explicit lifecycle/freshness; stable machine-readable compatibility contract; no expired deadline presented as future.
- **SHOULD BE BETTER:** Windows/OpenAB live-task diagnosis, low-glance local UX, bounded Prometheus integration, evidence-rich migration receipts.
- **DIFFERENTIATOR:** OpenAB plus local CLI live state in one localized capsule.
- **DO NOT COPY:** provider-count race, cloud account, enterprise dashboard, mobile, team analytics, marketplace, autonomous repair, broad credential collection.

## Virtual Executive Board

| Role | Independent question | Opportunity / priority | Cross-review |
|---|---|---|---|
| CEO | Can users obtain and trust the product? | #3, #5, #6, #11 before expansion | Three things only; reject feature breadth |
| CPO | Which promise is the product contract? | One explicit state for providers and metric lifecycle | Supports CTO; challenges Growth count marketing |
| CTO | Can docs, code, tests and runtime disagree while CI is green? | Canonical manifests and evidence gates | #11 shows a green test can protect a stale phase |
| Staff/Principal Engineer | Is phase closure end-to-end or merely local scope completion? | Link required successor phases before closed status | Avoid another parallel registry |
| UX Lead | Can an operator identify canonical names and actionable states? | Human-readable status and current dates | No new dashboard; improve truth |
| UX Researcher | Do users understand registered/live and legacy/canonical? | Comprehension fixtures, then bounded human test | Synthetic results are directional only |
| Growth Lead | What shortens time to first trusted value? | Download→launch→one provider→one reliable signal | Count and surface expansion harm activation |
| CFO/Business Analyst | Which maintenance cost is justified? | Remove indefinite duplicate series or document its cost | Manual release and local-only remain sufficient |
| Security/Privacy Lead | Will consumer discovery expand file/network access? | Explicit owner-supplied inventories only | Reject arbitrary host scanning |
| QA Lead | What proves the migration? | Endpoint, rule, dashboard, restart and docs consistency | Static tests alone cannot close |
| SRE Lead | Can existing alerts survive a change? | Bounded dual-query/cutover/post-mortem receipt | Do not surprise unknown consumers |
| Accessibility Specialist | Is state understandable without color/hover/source reading? | Plain text lifecycle plus keyboard/screen-reader runtime smoke | No accessibility defect claim without runtime |
| Customer Support Lead | Can support answer which metric to use today? | One canonical answer and current compatibility notice | Expired future tense is a support defect |

### Deliberation and minority opinions

Majority: **INVEST / SIMPLIFY** — close the contracts already promised.  
Minority: retain dual emit indefinitely to protect unknown consumers. Accepted only as a formal CANCELLED/PERMANENT_COMPATIBILITY decision with explicit cost and no false deadline.  
Minority: add dashboards to control consumer migration. Rejected; operator tooling can use representative fixtures and owner inventories without creating another product.  
Minority: broaden into CodexBar/OpenUsage territory. Rejected; LobsterPulse’s advantage is live OpenAB task state, not quota/provider breadth.

## 50 Synthetic Personas

Coverage: 30 regression baseline personas (60%) and 20 rotating exploration personas (40%). Ages 19–62; first-time, experienced and power users; Windows/macOS/Linux; broadband, hotspot, proxy, offline and restricted networks; maintainers, SRE, QA, support, privacy, finance; keyboard, low-vision and neurodiversity cases.

| ID / cohort | Background | Goal; expectation | Task; journey | Friction | Synthetic result; comment | Severity | Suggestion | Scenario choice |
|---|---|---|---|---|---|---|---|---|
| B01 baseline | 24歲初次 Windows 開發者；中等熟練；Win11；家用光纖 | 安裝後看 Claude/Codex 狀態; 有可下載、可回滾的程式 | 找下載並啟用 Codex; Repo→安裝→Provider→啟用→等事件 | main 仍無已發布 Release；#6/PR #8 未合併 | FAIL — 核心旅程在取得產品前停止 | P2 | 先完成誠實分發路徑 | AgentPulse |
| B02 baseline | 31歲後端工程師；Power User；Ubuntu；高速網路 | 同時追蹤四個本機 CLI; 跨平台且 provider 契約清楚 | 啟動兩個 session 並觀察狀態; 建置→掛 hooks→工作→等待→完成 | 需要自行建置；main 的支援分母仍待 #5 | PARTIAL — live-state 概念好但維運證據不足 | P2 | 先收斂 provider truth | AgentPulse |
| B03 baseline | 35歲 OpenAB 維運者；專家；Win11 工作站；LAN | 看九個 bot 與四個 CLI; UI、metrics、KPI 是同一份真相 | 連 OpenAB 後比對卡片與 /metrics; 啟動→產生事件→scrape→比對 | 13 個註冊不等於 13 個可觀測；#5 未合併 | PARTIAL — 差異化最強但 all-clear 尚不可完全信任 | P2 | 完成 canonical provider registry | LobsterPulse |
| B04 baseline | 28歲全端工程師；熟練；Win11；穩定網路 | 離開終端仍知道 Codex 何時等人; 啟用流程真的把 hooks 打開 | 用 codex_hooks=false 的既有設定啟用; 安裝→啟用→跑 Codex→收事件 | #3 只有程式/CI證據，缺 packaged runtime receipt | PARTIAL — 尚不能宣稱真機完成 | P1 | 完成 disposable-home runtime smoke | LobsterPulse |
| B05 baseline | 42歲 DevOps；專家；Ubuntu；公司代理 | 建立可靠的 agent 狀態告警; metrics 名稱與生命週期可預測 | scrape token/failure/session counters; 建置→啟動→scrape→寫 recording rule | 文件說 2026-07-03 改名，端點仍 dual emit | FAIL — 無法判斷哪個名稱是長期契約 | P2 | 完成或正式取消 counter cutover | LobsterPulse |
| B06 baseline | 39歲資深工程師；Power User；Win11；高速 | 同時管理多代理人; 通知少而可信 | 比較 waiting/completed/stale; 啟動多 session→聽提示→看卡片 | Decision-only queue 仍是 #9 研究分支 | PARTIAL — 現況能監看但仍需人工分流 | P2 research | 先驗證 queue，不擴張自動化 | LobsterPulse |
| B07 baseline | 30歲 macOS 工程師；熟練；光纖 | 看額度與 reset; 原生安裝、自動更新 | 安裝並固定 quota meter; brew→啟用 provider→看 reset | LobsterPulse 非此平台/需求主場 | SUCCESS alternative — 競品更貼近 quota JTBD | P3 | 不要追求跨平台 quota 廣度 | CodexBar |
| B08 baseline | 26歲 CLI 愛好者；專家；Linux；常離線 | 查每日 token/cost; 純 CLI、JSON、離線 | 匯出資料給腳本; 安裝 CLI→掃本機 logs→JSON | LobsterPulse 重點是 live state，不是歷史成本 | SUCCESS alternative — 非產品缺陷 | P3 | 保留 Prometheus，不重做成本分析器 | ccusage |
| B09 baseline | 33歲 QA；熟練；Win11 VM；受限網路 | 建立版本化回歸矩陣; 每版可下載、核對 checksum、回滾 | 跑安裝與 provider smoke; 下載固定版→啟動→重啟→回滾 | 無 tag/release；PR #8 僅文件 | FAIL — 無法建立可重現產品測試 | P2 | 完成 #6 的 release/source-only 決策 | AgentPulse |
| B10 baseline | 45歲技術主管；中高熟練；Win11；公司網路 | 一眼看到現在需要處理什麼; 狀態可信且不被通知轟炸 | 同跑五個代理人; 工作→事件湧入→找 blocker→處理 | 現況仍是 event/status surface，attention queue 未驗證 | PARTIAL — 核心價值清楚但決策壓縮尚未證明 | P2 research | 量測人類待處理事件，不先加 AI 摘要 | LobsterPulse |
| B11 baseline | 29歲 AI 研究工程師；Power User；macOS；高速 | 管理多家配額; provider 廣、reset 明確 | 啟用十多家服務並比較; 安裝→登入→看用量/incident | LobsterPulse 的 13 多為客製 bot，不符此需求 | FAIL — provider count 不是同一種價值 | P3 | 不參與廣度競賽 | CodexBar |
| B12 baseline | 37歲獨立開發者；熟練；Win11；行動熱點 | 用聲音知道 agent 在等我; 本機、低資料外傳 | 自訂聲音並跑兩 CLI; 建置→設定→離桌→收提示 | 產品下載與 Codex packaged runtime 未閉環 | PARTIAL — 需求高度貼合 | P1 | 先驗證 sidecar/hook/重啟 | LobsterPulse |
| B13 baseline | 34歲產品工程師；中等；macOS；穩定 | 查看週額度與花費; DMG/Homebrew、更新簡單 | 安裝後看多 provider 用量; brew→啟動→pin meter | LobsterPulse 不做成熟 quota UX | SUCCESS alternative — 競品更省維護 | P3 | 維持 live-state 定位 | OpenUsage |
| B14 baseline | 22歲學生；中等；Linux 舊機；校園網路 | 控制免費額度; 低資源、離線可用 | 查看每日用量; CLI→本機 logs→表格 | Tauri UI/自建置成本過高 | SUCCESS alternative — 非 LobsterPulse 核心 persona | P3 | 不建另一個 ccusage | ccusage |
| B15 baseline | 41歲 Windows 自動化工程師；專家；LAN | OpenAB 錯誤時快速定位; Prometheus 介面穩定 | 建立 failure/session 告警; scrape→寫 PromQL→演練 failure | 6 組 counter 名稱長期重複且期限過期 | FAIL — 告警依賴名稱選擇不明 | P2 | 以 #11 做明確 CUTOVER 或 POSTPONE | LobsterPulse |
| B16 baseline | 36歲跨平台維護者；專家；三 OS；高速 | 驗證 build/release matrix; 每平台 artifact 有 provenance | 下載三平台包做 smoke; release→checksum→launch→event | CI build 與產品分發仍分離 | FAIL — CI 綠不等於可取得產品 | P2 | 一個 Windows-first artifact 即可 | AgentPulse |
| B17 baseline | 32歲 Copilot＋Codex 使用者；熟練；Win11 | 不用切終端就看等待/完成; 一次啟用且不破壞現有 hooks | 帶第三方 hooks 的 install/reinstall/remove; 備份→啟用→事件→停用→比對 | #1 靜態修正已關閉但本輪無 Runtime | PARTIAL — 資料保全需實機收據 | P1 historical | 維持 NEEDS_RUNTIME_VERIFICATION | LobsterPulse |
| B18 baseline | 48歲顧問；中等；macOS；飯店 Wi‑Fi | 快速看限額; 簽章 app、低設定 | 下載→開啟→查看 quota; 官方包→provider→meter | LobsterPulse 無包且不是 macOS 主力 | SUCCESS alternative — 使用者會選成熟分發 | P2 | 不為此 persona 擴張 | CodexBar |
| B19 baseline | 27歲前端工程師；熟練；macOS；高速 | 固定看 Claude/Codex 額度; 自動更新與 stale cache | 安裝並 pin meter; DMG→啟用→等待 refresh | LobsterPulse 價值是即時任務而非成熟配額 | SUCCESS alternative — 競品更貼近 | P3 | 保持差異化 | OpenUsage |
| B20 baseline | 44歲 SRE；專家；Win11；公司 LAN | 把 agent 健康接進既有監控; metrics 稳定且遷移可稽核 | 寫 rate/alert/dashboard 查詢; scrape→rule test→dashboard→升級 | 已關閉的 T-1 spec 還強制 dual emit | FAIL — 遷移治理不可依賴 | P2 | machine-readable migration state | LobsterPulse |
| B21 baseline | 25歲初階工程師；低中熟練；Win11 | 知道 agent 是否卡住; 雙擊即用 | 找 exe 後啟用 Claude; Repo→下載→啟動→toggle | main 無 release | FAIL — 第一步失敗 | P2 | README CTA 必須可完成 | AgentPulse |
| B22 baseline | 38歲資料工程師；專家；Linux；離線 | 把用量餵進報表; JSON schema 穩定 | 排程每日匯出; CLI→JSON→warehouse | LobsterPulse /metrics 不是歷史用量倉 | SUCCESS alternative — 避免功能膨脹 | P3 | 保留出口即可 | ccusage |
| B23 baseline | 30歲繁中 Windows 開發者；熟練 | 中文、聲音、自訂順序; 本地化且狀態清楚 | 同跑四 CLI; 建置→設定中文→工作→查看 | 本地化好；安裝與 truth contract 未閉環 | PARTIAL — 偏好仍高 | P2 | 先修基本契約 | LobsterPulse |
| B24 baseline | 52歲工程經理；中等；macOS；公司網路 | 看團隊工具額度; 不碰每人本機設定 | 查看整體用量; 團隊登入→dashboard→report | LobsterPulse 是單機個人工具 | SUCCESS alternative — 不該變團隊 SaaS | P3 | 明確排除 multi-tenant | CodexBar |
| B25 baseline | 40歲客服主管；中等；Win11 | 回答哪個 metric 還能用; 文件日期與產品一致 | 依 README 回覆客戶; 查 README→CHANGELOG→endpoint | 日期說將於 7/3，現在仍未切 | FAIL — 無法給確定答案 | P2 | 狀態頁/manifest 一致 | LobsterPulse |
| B26 baseline | 23歲實習生；初階；Win11；校園網路 | 啟動後看 working/waiting; 設定步驟少且可理解 | 照 README 建置; 安裝 Rust/Node→build→run | 門檻遠高於桌面工具預期 | FAIL — 會改用可下載版本 | P2 | 不新增 onboarding tour，先交付 artifact | AgentPulse |
| B27 baseline | 46歲資安工程師；專家；Win11；隔離網 | 監控時不擴張秘密讀取; 本機、明確邊界 | 檢查 config mutation 與 API; disposable home→啟用→audit→停用 | #1/#3 仍需 runtime；新 #11 不需掃任意 host | PARTIAL — 方向正確但證據未完 | P1 | 只讀明確 consumer inventory | LobsterPulse |
| B28 baseline | 34歲可觀測性工程師；專家；Linux | 對 metric 做錄製規則; 命名符合 Prometheus | 建立 rate 與 failure ratio; scrape→promtool→Grafana | 新名稱正確但舊名稱無期限殘留 | FAIL — 雙契約提高錯誤與成本 | P2 | 完成 T-4/T-5 或正式延期 | LobsterPulse |
| B29 baseline | 58歲低數位熟練主管；Win11；大字模式 | 看哪個 agent 要處理; 文字清楚，不靠 hover/顏色 | 打開膠囊辨識狀態; 啟動→放大→鍵盤瀏覽→讀狀態 | 本輪未做輔助科技 Runtime | CANNOT VERIFY — 不能宣稱缺陷或通過 | Research pending | 做 packaged keyboard/screen-reader smoke | AgentPulse |
| B30 baseline | 27歲自由工作者；熟練；Win11；有限流量 | 小工具本機運作; 無帳號、低網路 | 啟用 Claude/Codex 並離線工作; 下載→設定→斷網→收事件 | 產品概念符合；仍缺可取得包 | PARTIAL — 若可下載會優先選 | P2 | 維持 no-cloud | LobsterPulse |
| R31 rotating | 49歲平台 SRE；專家；Win11 Server；內網 | 升級 exporter 不破 alert; 遷移有 owner、窗口、回滾 | 從舊 metric 切到 _total; inventory→dual query→cutover→觀察 | T-2~T-5 無完成證據 | FAIL — closed spec 讓人誤判 | P2 | #11 建 migration receipt | LobsterPulse |
| R32 rotating | 43歲 Grafana 管理員；專家；Linux | 維護十個 dashboard; 明確 deprecation lifecycle | 尋找 legacy query; export dashboards→搜尋→改寫→驗證 | 外部 dashboard inventory 為 UNKNOWN | CANNOT VERIFY — 不能安全直接移除 | P2 research | 只用 owner 提供 inventory | LobsterPulse |
| R33 rotating | 29歲 PromQL 新手；中等；macOS | 畫 token rate; 唯一推薦 metric 名 | 從 /metrics 複製名稱; scrape→autocomplete→query | 看到兩組相同值，不知選哪個 | FAIL — HELP 的過期 week 4 反而困惑 | P2 | 診斷輸出顯示 current state | LobsterPulse |
| R34 rotating | 37歲自架監控者；熟練；Raspberry Pi；低資源 | 降低 series 數; 不重複輸出相同資訊 | scrape 13 provider; 啟動→scrape→估計 series→保留 | 六組 per-provider 重複增加儲存 | FAIL — 小環境更在意成本 | P2 | CUTOVER 後移除 legacy | manual Prometheus |
| R35 rotating | 51歲 incident commander；專家；Win11 | 事故時讀懂 alert; 名稱直接揭示 counter 語意 | 查看 YAML 與 runbook; alert→runbook→metric→判斷 | legacy 名稱缺 _total，日期也失真 | FAIL — 降低故障時認知負擔 | P2 | 遵守 canonical suffix | LobsterPulse |
| R36 rotating | 33歲 release engineer；專家；三 OS | 把 contract 綁到 commit; migration phase 可自動驗證 | 檢查 spec、code、test 同步; manifest→CI→artifact→runtime receipt | T-1 closed 但 end-to-end 未追蹤 | FAIL — 狀態模型缺 successor gate | P2 | closed phase validator | AgentPulse |
| R37 rotating | 19歲學生；初階；Windows hotspot | 試用 AI monitor; 免費、簡單、低流量 | 找 release 並開啟; repo→download→run | 無 binary；不會自行建置 | FAIL — 改用可下載工具 | P2 | 先完成 #6 | AgentPulse |
| R38 rotating | 62歲顧問；低視力；Win11；鍵盤優先 | 知道 agent 是否等人; 大字、文字、鍵盤 | 調整縮放並讀 status; 啟動→zoom→tab→讀取 | Runtime accessibility 未驗證 | CANNOT VERIFY — 不能靠合成推演宣稱 bug | Research pending | 做 screen-reader/zoom smoke | AgentPulse |
| R39 rotating | 35歲網路受限企業工程師；熟練 | 離線監控本機 CLI; 不需雲端登入 | 安裝後封鎖外網測試; 下載→hash→離線→hooks | 沒有穩定 artifact；source build 需依賴 | FAIL — 可攜包有明顯價值 | P2 | Windows portable＋checksum | AgentPulse |
| R40 rotating | 41歲資料保護官；中高熟練 | 確保監控不收 prompt; 資料邊界有文件與測試 | 檢查 metrics/receipts; 啟用→scrape→確認無內容→停用 | 本輪未發現內容上傳，但未做流量實測 | CANNOT VERIFY — 不應把 local 推論成已驗證隱私 | P2 research | 最小化 receipt，不掃任意資料 | LobsterPulse |
| R41 rotating | 32歲 OpenAB bot 作者；專家 | 新增 provider 後被正確分類; registry、metrics、UI 同步 | 加入 bot fixture; 註冊→事件→metrics→KPI | #5 尚在 PR，main 仍非 canonical | PARTIAL — 可能重複清單漂移 | P2 | 先完成 #5 | LobsterPulse |
| R42 rotating | 28歲多螢幕 Power User；Win11 | 背景工作時只看 blocker; routine completion 不打斷 | 同跑八個 session; 啟動→事件 storm→找 waiting | #9 研究未驗證實際壓縮/漏報 | PARTIAL — 不要直接加 LLM 摘要 | P2 research | deterministic correlation canary | LobsterPulse |
| R43 rotating | 55歲財務分析師；中等；macOS | 看工具花費趨勢; 價格與成本表清楚 | 匯出月度使用; provider→cost→CSV | LobsterPulse 不以財務報告為核心 | SUCCESS alternative — 不值得為此擴張 | P3 | 維持本地任務監控 | OpenUsage |
| R44 rotating | 36歲 OSS 維護者；專家；Linux | 理解版本與棄用承諾; CHANGELOG 可執行 | 依 deprecation 排程改 downstream; 公告→改 query→等 removal→清理 | 日期過兩月仍 dual emit | FAIL — 破壞 release 信任 | P2 | 要嘛 cutover，要嘛正式延期 | LobsterPulse |
| R45 rotating | 47歲客服工程師；中等；Win11 | 快速診斷為何 alert 不工作; 文件能定位 canonical 名稱 | 照文件重建 query; support ticket→README→endpoint→rule | 三處都保留過期未來式 | FAIL — 支援成本升高 | P2 | 一份 machine-readable state | LobsterPulse |
| R46 rotating | 26歲神經多樣性使用者；熟練；Win11 | 減少通知切換成本; 低噪音且狀態持續 | 自訂提示並工作; 設定→多 session→snooze→回看 | 目前無 snooze/attention queue production evidence | PARTIAL — 方向吻合但不能宣稱改善 | P2 research | 先 replay 再真人 canary | LobsterPulse |
| R47 rotating | 38歲 CI 管理員；專家 | 阻止 spec/code drift; 預設分支 gate 可讀 | 提交只改日期或 metric; PR→validator→test→status | 現有 test 反而鎖住過期狀態 | FAIL — 綠色 gate 保護錯誤 phase | P2 | 測 migration state，不硬編 T-1 | LobsterPulse |
| R48 rotating | 44歲 Windows 無管理員權限使用者；中等 | 在公司電腦看 agent; portable 無安裝權限需求 | 下載 zip 到使用者目錄; download→hash→run→toggle | 無正式 zip；自行建置不可行 | FAIL — 選可下載替代品 | P2 | 最小 portable release | AgentPulse |
| R49 rotating | 31歲 API 整合者；專家；Linux | 從本機 endpoint 取 machine data; API/metrics 有穩定契約 | 每分鐘 scrape 並存檔; configure→scrape→schema check→upgrade | dual names 與過期期限讓 schema 無法收斂 | FAIL — 需要 lifecycle metadata | P2 | manifest + compatibility test | OpenUsage |
| R50 rotating | 60歲技術顧問；低頻使用；Win11 | 偶爾看 agent 是否完成; 不用維護另一套平台 | 雙擊→啟用一個 provider→關閉; 取得→首次啟動→一次任務→退出 | source build、provider semantics、無 runtime receipt | FAIL — 產品範圍比可用性成熟 | P2 | 先 SIMPLIFY/FIX，不加功能 | AgentPulse |

All rows above are synthetic journey simulations. No真人 user, browser, packaged app, CLI, Prometheus, Grafana or assistive-technology session was tested.

## Competitor Switching Test

Synthetic preference share from the 50 scenario choices:

| Choice | Personas | Share | Primary synthetic reason |
|---|---:|---:|---|
| LobsterPulse | 16 | 32% | Best fit for Traditional-Chinese OpenAB plus local live-task monitoring |
| AgentPulse | 12 | 24% | Obtainable cross-platform product with explicit hook coverage |
| CodexBar | 9 | 18% | Mature provider breadth and quota/status UX |
| OpenUsage | 7 | 14% | Clear install, freshness, CLI/API and subscription UX |
| ccusage | 4 | 8% | Local/offline structured token-cost workflow |
| Manual Prometheus / terminal workflow | 2 | 4% | Full operator control or low-resource custom integration |

This is a synthetic preference model, not market share, product analytics or human survey data.

## Red Team

1. **Persona bias:** the sample overrepresents technical operators because LobsterPulse is a developer tool. It may understate nontechnical onboarding pain.
2. **Competitor selection risk:** CodexBar/OpenUsage/ccusage emphasize quota rather than live task state. They are alternatives, not perfect direct substitutes.
3. **Deadline inference:** a missed public date does not prove production damage; external consumers are UNKNOWN.
4. **Over-engineering risk:** a generic migration platform would exceed the six-pair problem. One small manifest and validator is enough.
5. **Feature bloat:** dashboards, hosted telemetry, team accounts and auto-discovery are unnecessary.
6. **Copy risk:** Prometheus naming is a compatibility convention, not permission to copy Grafana or enterprise observability surfaces.
7. **Confirmation bias:** the repository intentionally kept dual emit to avoid breakage; indefinite compatibility can be rational. The defect is the contradictory lifecycle, not merely presence of legacy names.
8. **Growth bias:** more providers and visible metrics would increase maintenance before trust.
9. **Simplification test:** the solution can be DELETE (legacy names) or REWRITE (cancel the deadline); no new user-facing feature is required.
10. **Safety test:** do not scan unrelated host paths, credentials or networks to find consumers.
11. **Runtime humility:** no claim that current alerts are broken, series cost is material, or users are confused in production.
12. **Minority preserved:** POSTPONE/CANCEL is a valid acceptance path if owner evidence cannot support CUTOVER.

## Findings and Quality Gate

| Finding | Type | Priority | Impact | Strategic value | Gap / risk reduction | Confidence | Effort | Gate |
|---|---|---:|---|---|---|---|---|---|
| Expired Prometheus counter migration remains locked in dual emit | RELIABILITY / DOCUMENTATION / TECH_DEBT | P2 | Medium-high operator ambiguity; possible duplicate-series cost | High: metrics are the machine contract | MUST MATCH; high contract-risk reduction | High 0.98 for code/docs contradiction; medium for external impact | Medium | PASS: evidence, distinct fingerprint, actionable alternatives, explicit acceptance/regression, duplicate and lock check |

### Stable fingerprint

Reese-max/lobsterpulse + Prometheus exporter counter-name migration + scrape/upgrade after 2026-07-03 + legacy and _total series remain dual-emitted while docs still describe a future cutover + T-2/T-3/T-4/T-5 owner follow-ups were never converted into an executable completion gate.

### Issue mapping

| Finding | Mapping | Tracking object |
|---|---|---|
| Expired six-counter migration | NEW | [LobsterPulse #11](https://github.com/Reese-max/lobsterpulse/issues/11) |

Mapping completeness: **1/1 PASS**.

## Roadmap

### NOW

- FIX/SIMPLIFY #11: choose CUTOVER or POSTPONE/CANCEL and align code, tests, docs and evidence.
- Finish #3 packaged Codex runtime verification without disturbing #1’s config-preservation contract.
- Finish #5 provider truth and #6 distribution decisions through their existing PRs.

### NEXT

- Complete #9’s research-only replay/evidence gates after higher-priority truth/runtime work.
- If #11 chooses CUTOVER, perform the bounded Prometheus/PromQL/dashboard/alert post-mortem.

### LATER

- Improve accessibility and low-noise attention UX only after packaged runtime validation.
- Consider reusable lifecycle-manifest patterns across the portfolio, without centralizing product-specific behavior.

### DON'T

- No cloud SaaS, mobile, team dashboard, marketplace, billing, broad credential collection, provider-count race, automatic agent steering, arbitrary consumer scan, or second observability backend.

Priority order applied: **REMOVE > SIMPLIFY > FIX > IMPROVE > ADD**.

## Change Since Previous Audit

- Previous audited SHA: e4a2333349fb3ca7892a1c7f576303fc968f3672.
- Current audited SHA: 5c09da0cf7b01fcfd80c60ee2a529ae650aed265.
- #5 now has active PR #7; #6 has active PR #8; #9 has active PR #10. None was modified by this audit.
- New confirmed delta: the publicly announced 2026-07-03 Prometheus cutover is now more than two months past, while main still enforces the T-1 dual-emit phase.
- No runtime, release, or external consumer evidence was added by this audit.

## Regression Review

| Issue / scope | Status | Evidence / boundary |
|---|---|---|
| #1 preserve existing Codex hooks | CANNOT VERIFY / NEEDS_RUNTIME_VERIFICATION | Closed in code history; no disposable real-filesystem round-trip in this run |
| #3 codex_hooks=false→true | PARTIALLY FIXED / NEEDS_RUNTIME_VERIFICATION | Prior merge/build evidence; packaged real Codex event still absent |
| #5 truthful 13-provider denominator | STILL REPRODUCIBLE on main / ACTIVE PR #7 | Dedicated branch exists; skipped under mutual exclusion |
| #6 truthful install path | STILL REPRODUCIBLE on main / ACTIVE PR #8 | Dedicated branch exists; no published artifact verified |
| #9 decision-only attention queue | RESEARCH IN PROGRESS / ACTIVE PR #10 | Design PR exists; compression and miss-rate need replay/runtime |
| #11 counter migration | STILL REPRODUCIBLE / NEEDS_RUNTIME_VERIFICATION | Current main code, tests and docs directly reproduce contract drift |

Verified Fixed this round: **0**.

## Runtime Pending

- Packaged Windows launch and sidecar discovery.
- Codex install/reinstall/remove with existing hooks and codex_hooks=false.
- Real Claude/Codex/OpenAB events, freshness and restart behavior.
- /metrics endpoint scrape across the six pairs and 13 provider labels.
- Owner-supplied external Prometheus/Grafana/alert inventory.
- Disposable Prometheus rule/dashboard queries for #11.
- Keyboard-only, screen-reader, zoom/reduced-motion and notification comprehension.
- Real multi-agent attention-queue usability and miss-rate.

## Decision Memo

- **What this product should become:** a trustworthy, Windows-first local attention/status layer for OpenAB and a small set of coding CLIs.
- **Who it should serve:** Reese-max/OpenAB operators and power developers who run several local agents and want low-glance, local state.
- **Why users would choose it:** personalized OpenAB integration, Traditional Chinese, local sounds, live working/waiting semantics and Prometheus output.
- **Why users choose competitors:** AgentPulse is obtainable and explicit; CodexBar/OpenUsage provide mature quota/status UX; ccusage provides scriptable local analysis.
- **Biggest competitive gaps:** acquisition, runtime receipts, provider truth, lifecycle-consistent metrics.
- **Potential moat:** trustworthy normalization of personalized OpenAB plus local CLI events into a tiny local surface and auditable machine contract.
- **Top strategic priorities:** #3, #5/#6, #11; then #9 research.
- **Top engineering priorities:** canonical manifests, phase closure validator, packaged runtime harness, exact evidence receipts.
- **Top UX priorities:** one truthful install CTA, explicit registered/live/stale/external states, one canonical metric name/lifecycle.
- **What NOT to build:** SaaS, mobile, collaboration, billing, IDE extension, generic dashboard, autonomous repair, broad provider catalog.
- **Features worth removing:** expired future-tense deprecation copy; legacy counter emits if CUTOVER evidence passes; vanity provider-count framing.
- **Biggest risks:** false all-clear, config mutation without runtime proof, indefinite compatibility debt, green CI guarding stale contracts, attention bloat.
- **Next experiments:** disposable packaged Codex canary; six-pair Prometheus consumer inventory/cutover rehearsal; #9 deterministic 100-event replay.
- **Decision:** **INVEST / SIMPLIFY** — differentiation is credible, but trust boundaries must precede expansion.

## Portfolio CEO Review

This run deep-audited LobsterPulse only; other repositories use the existing portfolio baseline and are not claimed re-verified.

| Rank | Portfolio product | Direction | Shared leverage |
|---:|---|---|---|
| 1 | academic-mcp | INVEST | Evidence ledger, recovery/admission receipts |
| 2 | autodev-ng | INVEST / SIMPLIFY | Locks, goals, runtime evidence and external-effect policy |
| 3 | police-exam-archive | INVEST | Domain content/data integrity and learning state |
| 4 | ai-flight-radar | INVEST / SIMPLIFY | Freshness, provenance and fail-closed collection |
| 5 | LobsterPulse | INVEST / SIMPLIFY | Local agent event/attention observability |
| 6 | skill-foundry + herdr-skills | CONSOLIDATE CONTROL PLANE | Skill certification, promotion and memory-candidate boundaries |
| 7 | remaining narrow utilities | MAINTAIN / MERGE / PAUSE by evidence | Reuse CI receipts, design system and data contracts; do not centralize product logic |

Portfolio overlap and consolidation:
- LobsterPulse should consume provider/execution identity and receipt patterns from autodev-ng, not become another orchestrator.
- skill-foundry should remain the single Skill certification/promotion control plane; herdr-skills supplies reflection candidates.
- police-exam-practice remains a compatibility layer for police-exam-archive rather than rebuilding learning features.
- Shared infrastructure candidates: exact-SHA evidence receipts, lifecycle manifests, runtime verification schemas, AI gateway/cost guard, accessible status tokens.
- Do not merge LobsterPulse with CodexBar-like quota products; its moat is local live-task attention.

## Mandatory Validation

- **Total Findings:** 1
- **New Issues Created:** 1
  - [Reese-max/lobsterpulse #11 — [P2][RELIABILITY][DOCUMENTATION] Complete or retire the expired Prometheus counter migration](https://github.com/Reese-max/lobsterpulse/issues/11)
- **Updated Existing Issues:** 0
- **Reopened Issues:** 0
- **Research Issues:** 0 new
- **Duplicate Avoided:** 12 symptom/opportunity groups
  - provider denominator→#5/PR #7;
  - installation/release→#6/PR #8;
  - attention triage→#9/PR #10;
  - Codex hook preservation/enabling→#1/#3;
  - six metric-name symptoms merged into root #11;
  - dashboard product, metric auto-discovery and storage-cost complaints rejected rather than split.
- **Issue Write Blocked:** 0
- **SKIPPED_LOCKED / excluded active scopes:** #5/PR #7/devin/issue-5-truthful-denominator; #6/PR #8/devin/issue-6-install-path; #9/PR #10/devin/issue-9-attention-queue; github-3-enable-codex-hooks branch.
- **Rejected Findings:** 14
  1. Production alert/dashboard breakage — no runtime evidence.
  2. Material storage-cost claim — series duplication confirmed, cost magnitude unmeasured.
  3. Immediate hard delete of legacy names — unknown consumers require evidence.
  4. Keep dual emit silently forever — contradicts public lifecycle.
  5. Build a hosted Grafana service — feature bloat.
  6. Add a second telemetry backend — no validated need.
  7. Auto-scan arbitrary host files/endpoints for consumers — privacy/security expansion.
  8. Add more provider metrics — expansion before contract repair.
  9. Cloud accounts/team analytics — outside product moat.
  10. Mobile/native companion — no evidence and high maintenance.
  11. LLM-generated alert summaries — alert-noise risk; deterministic state first.
  12. Copy CodexBar/OpenUsage provider breadth — wrong competitive dimension.
  13. Accessibility defect claim — no assistive-technology runtime.
  14. Mark #1/#3/#5/#6/#9 fixed from code/PR presence — acceptance/runtime evidence incomplete.
- **Verified Fixed:** 0
- **Priority distribution:** P0 0 / P1 0 / P2 1 / P3 0 / STRATEGIC 0
- **Highest Priority Finding:** #11 within this round; portfolio execution still puts #3 packaged runtime ahead where user config safety is concerned.
- **Finding Mapping:** 1/1 PASS
- **Lock:** #11 lock marker created and re-read; no earlier unexpired competing marker.
- **Runtime Pending:** listed above.
