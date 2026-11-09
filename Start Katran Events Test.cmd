@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start-events-test.ps1"
if errorlevel 1 pause
