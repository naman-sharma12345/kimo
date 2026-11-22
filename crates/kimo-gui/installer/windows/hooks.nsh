; kimo installer hooks (Tauri NSIS, bundle.windows.nsis.installerHooks).
;
; Coding agents (Claude Code, Codex, Cursor...) start their own `kimo-gui.exe mcp` from this folder, and it runs the engine
; `kimo-mcp.exe` beside it. Those keep running while the app updates, and Windows cannot overwrite a running .exe, so an
; update replaced the app but left the agents' engine at its old version. Before installing (and before uninstalling)
; every kimo-gui.exe / kimo-mcp.exe running FROM THIS FOLDER is closed; the agent restarts the MCP server on its next call
; and gets the new engine. Copies running from anywhere else (a source checkout, another install) are left alone.

!macro KIMO_CLOSE_RUNNING
  DetailPrint "Closing kimo processes started from $INSTDIR"
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d = '$INSTDIR\'; Get-CimInstance Win32_Process | Where-Object { ($$_.Name -eq 'kimo-mcp.exe' -or $$_.Name -eq 'kimo-gui.exe') -and $$_.ExecutablePath -and $$_.ExecutablePath.StartsWith($$d, [System.StringComparison]::OrdinalIgnoreCase) } | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force -ErrorAction SilentlyContinue }"`
  Pop $0
  Sleep 400 ; let Windows release the files
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro KIMO_CLOSE_RUNNING
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro KIMO_CLOSE_RUNNING
!macroend
