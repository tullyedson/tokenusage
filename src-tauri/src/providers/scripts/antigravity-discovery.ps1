$ErrorActionPreference = 'Stop'
# Only inspect the installed Antigravity service belonging to this Windows user.
# Output goes to the native reader's private pipe, never to HTML or a log file.
$ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$roots = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Antigravity'),
    (Join-Path $env:ProgramFiles 'Antigravity')
)
$servers = @()
foreach ($process in @(Get-CimInstance Win32_Process -Filter "Name='language_server.exe'")) {
    $validPath = $false
    foreach ($root in $roots) {
        if ($process.ExecutablePath -eq (Join-Path $root 'resources\bin\language_server.exe')) { $validPath = $true }
    }
    if (-not $validPath) { continue }
    $owner = Invoke-CimMethod -InputObject $process -MethodName GetOwnerSid
    if ($owner.ReturnValue -ne 0 -or $owner.Sid -ne $ownerSid) { continue }
    $match = [regex]::Match($process.CommandLine, '--csrf_token(?:=|\s+)"?([^"\s]+)')
    if (-not $match.Success -or $match.Groups[1].Value.Length -gt 512) { continue }
    $ports = @(Get-NetTCPConnection -State Listen -OwningProcess $process.ProcessId -ErrorAction SilentlyContinue |
        Where-Object LocalAddress -eq '127.0.0.1' | Select-Object -ExpandProperty LocalPort -Unique | Sort-Object | Select-Object -First 4)
    if ($ports.Count -gt 0) { $servers += @{ csrf = $match.Groups[1].Value; ports = $ports } }
    if ($servers.Count -ge 4) { break }
}
ConvertTo-Json -InputObject @{ servers = $servers } -Depth 4 -Compress
