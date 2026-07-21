@echo off
:loop
call "%USERPROFILE%\.lobsterpulse\scripts\update-ccusage-cache.cmd"
timeout /t 300 /nobreak >/dev/null
goto loop
