param(
    [string]$NewOriginUrl
)

$ErrorActionPreference = "Stop"

function Ensure-RemoteRenamed {
    $originExists = git remote | Select-String -Pattern "^origin$" -Quiet
    $upstreamExists = git remote | Select-String -Pattern "^upstream$" -Quiet

    if ($originExists -and -not $upstreamExists) {
        git remote rename origin upstream
    }
}

function Ensure-Branch {
    param(
        [string]$BranchName,
        [string]$StartPoint = "HEAD"
    )

    $branchExists = git branch --list $BranchName
    if (-not $branchExists) {
        git branch $BranchName $StartPoint | Out-Null
    }
}

Ensure-RemoteRenamed
Ensure-Branch -BranchName "lobsterpulse/main"

git checkout lobsterpulse/main | Out-Null

$hasUpstreamMain = git branch -r | Select-String -Pattern "upstream/main" -Quiet
if ($hasUpstreamMain) {
    git branch --set-upstream-to upstream/main lobsterpulse/main | Out-Null
}

if ($NewOriginUrl) {
    $originExists = git remote | Select-String -Pattern "^origin$" -Quiet
    if ($originExists) {
        git remote set-url origin $NewOriginUrl
    } else {
        git remote add origin $NewOriginUrl
    }
}

Write-Host "目前分支：" -NoNewline
git branch --show-current
Write-Host "目前 remotes："
git remote -v
