param(
    [Parameter(Mandatory = $true)][int]$RootProcessId,
    [ValidateRange(1, 3600)][int]$Seconds = 60,
    [Parameter(Mandatory = $true)][string]$OutputPath
)
$ErrorActionPreference = 'Stop'
$rootProcess = Get-Process -Id $RootProcessId
$rootStarted = $rootProcess.StartTime
$knownProcesses = @{}
$samples = [System.Collections.Generic.List[object]]::new()
$watch = [System.Diagnostics.Stopwatch]::StartNew()
for ($iteration = 0; $iteration -lt $Seconds; $iteration++) {
    $rootProcess = Get-Process -Id $RootProcessId -ErrorAction SilentlyContinue
    if (!$rootProcess -or $rootProcess.StartTime -ne $rootStarted) { break }
    $tree = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootProcessId)
    do {
        $added = $false
        foreach ($item in $tree) {
            if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) { $added = $true }
        }
    } while ($added)
    $cpuDelta = 0.0
    $privateBytes = 0L
    $workingBytes = 0L
    foreach ($processIdValue in $ids) {
        $process = Get-Process -Id $processIdValue -ErrorAction SilentlyContinue
        if (!$process) { continue }
        $key = "$processIdValue/$($process.StartTime.Ticks)"
        $cpu = $process.TotalProcessorTime.TotalSeconds
        if ($knownProcesses.ContainsKey($key)) { $cpuDelta += [Math]::Max(0, $cpu - $knownProcesses[$key]) }
        $knownProcesses[$key] = $cpu
        $privateBytes += $process.PrivateMemorySize64
        $workingBytes += $process.WorkingSet64
    }
    $samples.Add([pscustomobject]@{ elapsed_seconds = $watch.Elapsed.TotalSeconds; cpu_delta_seconds = $cpuDelta; private_bytes = $privateBytes; working_set_bytes = $workingBytes; process_count = $ids.Count })
    Start-Sleep -Milliseconds 1000
}
$samples | ConvertTo-Json | Set-Content -LiteralPath $OutputPath -Encoding utf8
