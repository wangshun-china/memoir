# Expose WSL memoir-api (port 18081) on the Windows LAN IP so phones on the same Wi-Fi can reach it.
# Run in elevated PowerShell if portproxy/firewall commands fail.
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\win_expose_api.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\win_expose_api.ps1 -Remove

param(
  [switch]$Remove,
  [int]$Port = 18081
)

$ErrorActionPreference = "Stop"

function Get-WslIp {
  $ip = (wsl -e bash -lc "hostname -I | awk '{print `$1}'").Trim()
  if (-not $ip) { throw "Could not read WSL IP" }
  return $ip
}

function Get-LanIp {
  $candidates = Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object {
      $_.IPAddress -notlike "127.*" -and
      $_.IPAddress -notlike "172.1[6-9].*" -and
      $_.IPAddress -notlike "172.2[0-9].*" -and
      $_.IPAddress -notlike "172.3[0-1].*" -and
      $_.IPAddress -notlike "169.254.*" -and
      $_.IPAddress -notlike "198.18.*" -and
      $_.PrefixOrigin -ne "WellKnown"
    } |
    Sort-Object -Property InterfaceMetric
  # Prefer 192.168.x private LAN
  $lan = $candidates | Where-Object { $_.IPAddress -like "192.168.*" } | Select-Object -First 1
  if (-not $lan) { $lan = $candidates | Select-Object -First 1 }
  if (-not $lan) { throw "Could not find a Windows LAN IPv4 address" }
  return $lan.IPAddress
}

if ($Remove) {
  netsh interface portproxy delete v4tov4 listenaddress=0.0.0.0 listenport=$Port 2>$null | Out-Null
  Write-Host "Removed portproxy 0.0.0.0:$Port"
  exit 0
}

$wslIp = Get-WslIp
$lanIp = Get-LanIp

Write-Host "WSL IP:  $wslIp"
Write-Host "LAN IP:  $lanIp"
Write-Host "Port:    $Port"

# Refresh proxy rule
netsh interface portproxy delete v4tov4 listenaddress=0.0.0.0 listenport=$Port 2>$null | Out-Null
netsh interface portproxy add v4tov4 listenaddress=0.0.0.0 listenport=$Port connectaddress=$wslIp connectport=$Port

# Firewall rule (ignore if exists)
$ruleName = "memoir-api-$Port"
$existing = Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue
if (-not $existing) {
  try {
    New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -Protocol TCP -LocalPort $Port -Action Allow | Out-Null
    Write-Host "Firewall rule added: $ruleName"
  } catch {
    Write-Host "WARN: could not add firewall rule (try Run as Administrator): $_"
  }
}

Write-Host ""
Write-Host "Phone / miniprogram should use:"
Write-Host "  http://${lanIp}:${Port}/api/v1"
Write-Host ""
Write-Host "Health check from Windows:"
try {
  $h = Invoke-WebRequest -UseBasicParsing -TimeoutSec 3 "http://${lanIp}:${Port}/health"
  Write-Host "  OK $($h.Content)"
} catch {
  Write-Host "  FAIL: $_"
  Write-Host "  Ensure: ./scripts/local_dev.sh start  (in WSL) and re-run this script as Admin if needed."
}
