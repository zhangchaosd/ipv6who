# Runs the Windows install block from README.md verbatim, except that the
# release download is replaced by a zip of the freshly built binary with the
# same layout. The final curl is left to verify-windows.ps1.
# Usage: readme-install.ps1 <path to ipv6who.exe>
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
$stage = Join-Path $env:TEMP 'ipv6who-stage'
New-Item -ItemType Directory -Force "$stage\ipv6who-windows-$arch" | Out-Null
Copy-Item $Exe "$stage\ipv6who-windows-$arch\ipv6who.exe"
tar.exe -a -cf "$env:TEMP\ipv6who.zip" -C $stage "ipv6who-windows-$arch"

$lines = Get-Content README.md
$section = [array]::FindIndex($lines, [Predicate[string]] { $args[0].StartsWith('### Windows') })
$start = [array]::FindIndex($lines, $section, [Predicate[string]] { $args[0] -eq '```powershell' }) + 1
$end = [array]::FindIndex($lines, $start, [Predicate[string]] { $args[0] -eq '```' })
$block = $lines[$start..($end - 1)]
$download = $block | Where-Object { $_ -like 'curl.exe -fsSLo *' }
if (-not $download) { throw 'download line not found in README' }
$block = $block | Where-Object { $_ -notlike 'curl.exe -fsSLo *' -and $_ -notlike 'curl.exe -s "http://*' }

$script = Join-Path $env:TEMP 'readme-install-block.ps1'
Set-Content $script $block
Get-Content $script
& $script
