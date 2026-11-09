@echo off
cd /d "%~dp0"
if not exist "%~dp0runtime\Katran.exe" (
  echo Katran build is missing.
  echo Build: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-working.ps1
  pause
  exit /b 1
)
start "" "%~dp0runtime\Katran.exe" "%~dp0katran-data"
