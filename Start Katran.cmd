@echo off
cd /d "%~dp0"
if not exist "%~dp0runtime\current-version.txt" (
  echo Katran build is missing.
  echo Build: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-working.ps1
  pause
  exit /b 1
)
set /p KATRAN_BUILD_VERSION=<"%~dp0runtime\current-version.txt"
start "" "%~dp0runtime\versions\%KATRAN_BUILD_VERSION%\katran-ui.exe" "%~dp0katran-data"
2
