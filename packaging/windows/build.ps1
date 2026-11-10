param(
    [string]$Version = '1.0.0',
    [string]$Binary = 'target/release/kimo-mcp.exe',
    [string]$Compiler = '',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/../..").Path
Set-Location $root
if ($Version -notmatch '^\d+\.\d+\.\d+([.-][A-Za-z0-9.-]+)?$') { throw 'Invalid version' }
if (-not $SkipBuild) {
    cargo build --release --locked -p kimo-mcp
    if ($LASTEXITCODE) { throw 'Cargo build failed' }
}
if (-not (Test-Path $Binary)) { throw "Missing binary: $Binary" }

# Use the same font manifest as Linux; include each family's license.
python packaging/windows/fetch_fonts.py
if ($LASTEXITCODE) { throw 'Font download failed' }
$manifest = Get-Content fonts/fetch.sh
$fonts = @(Get-ChildItem fonts/*.ttf | Where-Object Length -ge 10000)
if ($fonts.Count -lt 70) { throw "Only $($fonts.Count) fonts downloaded; need at least 70" }
if (-not $Compiler) {
    $candidates = @("${env:ProgramFiles(x86)}/Inno Setup 6/ISCC.exe", "$env:LOCALAPPDATA/Programs/Inno Setup 6/ISCC.exe")
    $Compiler = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Compiler) { throw 'Install Inno Setup 6 or pass -Compiler' }
$name = "kimo-$Version-windows-x86_64"
$stage = Join-Path $root "target/windows-package/$name"
if (Test-Path $stage) { throw "Staging directory already exists: $stage. Use a new version or remove it before rebuilding." }
New-Item -ItemType Directory -Force "$stage/bin", "$stage/fonts", "$root/dist" | Out-Null
Copy-Item $Binary "$stage/bin/kimo-mcp.exe"
foreach ($font in $fonts) {
    $line = $manifest | Where-Object { ($_ -match '/') -and (($_.Split('/')[-1] -replace '%5B.*%5D', '-Var') -eq $font.Name) } | Select-Object -First 1
    if (-not $line) { continue }
    $family = $line.Split('/')[0]
    $license = "target/font-licenses/$family-OFL.txt"
    if (-not (Test-Path $license)) { throw "Missing license for $family" }
    Copy-Item $font.FullName "$stage/fonts/"
    Copy-Item $license "$stage/fonts/"
}
if (@(Get-ChildItem "$stage/fonts/*.ttf").Count -lt 70) { throw 'Packaged font set is incomplete' }
Copy-Item README.md, FEATURES.md, LICENSE, docs/RELEASE_BINARIES.md $stage
Copy-Item packaging/windows/WINDOWS.md $stage
python packaging/windows/smoke.py "$stage/bin/kimo-mcp.exe"
if ($LASTEXITCODE) { throw 'Packaged binary smoke test failed' }
Compress-Archive -Path $stage -DestinationPath "dist/$name.zip" -Force
& $Compiler "/DAppVersion=$Version" "/DPackageDir=$stage" "/DOutputDir=$root/dist" packaging/windows/kimo.iss
if ($LASTEXITCODE) { throw 'Installer compilation failed' }
foreach ($file in @("dist/$name.zip", "dist/$name-setup.exe")) {
    $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($file))" | Set-Content "$file.sha256" -Encoding ascii
}
