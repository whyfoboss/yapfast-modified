@echo off
cd /d "%~dp0"
echo ========================================================
echo Starting Yapfast in Preview / Testing Mode
echo ========================================================
target\fast\yapfast.exe --preview %*
if errorlevel 1 (
  echo.
  echo [Yapfast exited with code %errorlevel%]
  pause
)
