@echo off
if exist "%~dp0release\MHAutoUpdateCompilerNext.exe" (
    start "" "%~dp0release\MHAutoUpdateCompilerNext.exe"
) else (
    echo Run scripts\build.ps1 to build MHAutoUpdateCompilerNext first.
    pause
)
