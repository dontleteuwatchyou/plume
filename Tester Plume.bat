@echo off
title Plume - compilation et installation
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install-local.ps1"
echo.
pause
