# Checks the installed ipv6who Windows service.
$ErrorActionPreference = 'Stop'

$r = $null
for ($i = 0; $i -lt 30; $i++) {
    try { $r = Invoke-RestMethod 'http://[::1]:8066/v1/ipv6' } catch { $r = $null }
    if ($r -and $r.address -eq '2001:db8::1') { break }
    Start-Sleep -Seconds 1
}
$r | ConvertTo-Json -Depth 5
if (-not $r -or $r.address -ne '2001:db8::1' -or $r.type -ne 'static') { throw 'unexpected /v1/ipv6 response' }

Write-Host 'IPv4 through the dual-stack socket:'
Invoke-RestMethod 'http://127.0.0.1:8066/healthz'

$account = (Get-CimInstance Win32_Service -Filter "Name='ipv6who'").StartName
Write-Host "service account: $account"
if ($account -ne 'NT AUTHORITY\LocalService') { throw "unexpected service account $account" }

$ip = (Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notmatch '^(127|169\.254)\.' } |
    Select-Object -First 1).IPAddress
Write-Host "connecting from $ip, which is not allowlisted:"
curl.exe -s -m 5 "http://${ip}:8066/healthz"
Write-Host "curl exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 52 -and $LASTEXITCODE -ne 56) { throw 'connection was not dropped' }
exit 0
