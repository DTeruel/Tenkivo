@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\publish-tenkivo.ps1"
set "TENKIVO_EXIT=%ERRORLEVEL%"
echo.
pause
exit /b %TENKIVO_EXIT%
