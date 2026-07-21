@echo off
:loop
node "%USERPROFILE%\.lobsterpulse\scripts\update-ccusage-cache.js" >nul 2>&1
node "%USERPROFILE%\.lobsterpulse\scripts\update-codex-cache.js" >nul 2>&1
timeout /t 180 /nobreak >nul
goto loop
