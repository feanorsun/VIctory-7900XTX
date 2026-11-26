@echo off
setlocal
rem Starts the title with this fork's settings (scripts\play.ps1).
rem   set VICTORY_GAME_DIR=C:\path\to\your\own\dump
rem   play.cmd                     the console is chosen by this PC's memory
rem   play.cmd -Output 1280x720    a smaller picture: the least work for the card
rem Other options are those of scripts\play.ps1 (-Console, -Safe, -Build, -More).
rem Build first: cargo build --locked --profile diag -p victory --features diagnostics
if "%VICTORY_GAME_DIR%"=="" (
  echo Set VICTORY_GAME_DIR to your own dump folder, the one with game_scarlett_master.exe in it.
  exit /b 1
)
set "TITLE_EXE=%VICTORY_GAME_DIR%\game_scarlett_master.exe"
if not exist "%TITLE_EXE%" set "TITLE_EXE=%VICTORY_GAME_DIR%\game_scarlett_master\game_scarlett_master.exe"
if not exist "%TITLE_EXE%" (
  echo No game_scarlett_master.exe in "%VICTORY_GAME_DIR%" or in its game_scarlett_master folder.
  exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\play.ps1" -Exe "%TITLE_EXE%" %*
exit /b %ERRORLEVEL%
