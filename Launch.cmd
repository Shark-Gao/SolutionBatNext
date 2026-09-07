@echo off
if exist "%~dp0release\SolutionBatNext.exe" (
    start "" "%~dp0release\SolutionBatNext.exe"
) else (
    echo Run scripts\build.ps1 to build SolutionBat Next first.
    pause
)
