@echo off
setlocal
cd /d "%~dp0"
where powershell >nul 2>nul
if not errorlevel 1 (
  start "" powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0tools\dev\launcher-gui.ps1"
  exit /b 0
)
where pwsh >nul 2>nul
if not errorlevel 1 (
  start "" pwsh -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0tools\dev\launcher-gui.ps1"
  exit /b 0
)
echo PowerShell not found. Please install PowerShell 7 or use Windows PowerShell.
pause
exit /b 1
