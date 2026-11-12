@echo off
cd /d "%~dp0"
echo ========================================================
echo Starting Yapfast (TASK 9 Native Windows Build)
echo ========================================================
target\fast\yapfast.exe %*
if errorlevel 1 (
  echo.
  echo [Yapfast exited with code %errorlevel%]
  pause
)
