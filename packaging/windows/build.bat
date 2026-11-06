@echo off
rem Builds "Kincaid Label Generator.exe" (with the K icon).
rem Needs Rust from https://rustup.rs (its installer also sets up the Microsoft C++ build tools it needs).
cd /d "%~dp0\..\.."
cargo build --release || goto :err
copy /y "target\release\kincaid-label-generator.exe" "Kincaid Label Generator.exe" >nul
echo.
echo Done: "Kincaid Label Generator.exe" - it runs on its own, copy it anywhere.
pause
exit /b
:err
echo Build failed - see the messages above.
pause
