# kimo on Windows

kimo is an MCP stdio server, with no graphical editor. The installer installs
for the current user (no administrator access needed). The portable ZIP has
the same files and can be extracted anywhere. Keep `bin` and `fonts` together.

Install FFmpeg separately, as with the Linux package:

    winget install --id Gyan.FFmpeg --exact

Restart your MCP client after installing FFmpeg so it inherits the updated PATH.
Check `ffmpeg -version` and `ffprobe -version` in a new terminal.

Add this entry to your MCP client's configuration, replacing YOUR_USER with
your Windows username (or use the full path to your portable extraction):

```json
{
  "mcpServers": {
    "kimo": {
      "command": "C:\\Users\\YOUR_USER\\AppData\\Local\\Programs\\kimo\\bin\\kimo-mcp.exe"
    }
  }
}
```

Start with `kimo_search`, `kimo_describe`, or the grouped tools. Fonts are found
relative to the executable, so the client can start it from any directory.
Optional transcription needs `whisper` on PATH. Native OCIO is not bundled.

The installer is unsigned. Verify its SHA-256 against the checksum downloaded
from the same GitHub release. Windows may show a SmartScreen warning.

To uninstall, use Windows Settings > Apps > Installed apps > kimo.
