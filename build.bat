@echo off
:start
cargo +nightly build --target aarch64-linux-android --profile smol -Z build-std=std,panic_abort
timeout -t 1
pause
goto start
pause