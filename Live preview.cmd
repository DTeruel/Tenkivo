@echo off
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where pnpm.cmd >nul 2>nul
if errorlevel 1 (
  for /f "delims=" %%P in ('powershell -NoProfile -Command "(Get-AppxPackage '*CodexPrimaryRuntime*' | Sort-Object Version -Descending | Select-Object -First 1).InstallLocation"') do set "STUDIO_RUNTIME=%%P"
)
if defined STUDIO_RUNTIME set "PATH=%STUDIO_RUNTIME%\dependencies\bin\fallback;%STUDIO_RUNTIME%\dependencies\node\bin;%PATH%"
call pnpm desktop dev
if errorlevel 1 pause
