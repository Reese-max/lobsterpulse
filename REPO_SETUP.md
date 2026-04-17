# Repo Setup

這份專案現在不是單純沿用 upstream clone，而是整理成適合你自己維護的結構。

## 目標結構

- `upstream`
  - 指向原始 `https://github.com/yazelin/AgentPulse`
- `lobsterpulse/main`
  - 你的品牌主線
- `feature/*`
  - 日常功能分支

## 已整理的原則

- 保留 upstream，方便之後拉回 AgentPulse 的更新
- 品牌版不再直接在 upstream 的 `main` 上累積
- 之後如果你建立自己的 GitHub repo，建議再把它加成 `origin`

## 一鍵整理

```powershell
.\scripts\bootstrap_git.ps1
```

這會做三件事：

1. 把 `origin` 改名成 `upstream`
2. 建立 `lobsterpulse/main`
3. 切到 `lobsterpulse/main`

## 之後加你自己的 repo

```powershell
.\scripts\bootstrap_git.ps1 -NewOriginUrl https://github.com/<you>/lobsterpulse.git
```

## 建議日常流程

```powershell
git checkout lobsterpulse/main
git checkout -b feature/some-change
git add .
git commit -m "feat: some change"
git checkout lobsterpulse/main
git merge --ff-only feature/some-change
```
