@echo off
set RUSTFLAGS=-C target-feature=+crt-static
cargo build --target=i686-pc-windows-msvc --release
::echo f | xcopy /y "C:\Users\Public\Desktop\sm_closestpos-rs\target\i686-pc-windows-msvc\release\sm_closestpos.dll" "D:\steamcmd\cstrike\cstrike\addons\sourcemod\extensions\closestpos.ext.dll"
