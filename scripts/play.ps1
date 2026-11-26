# Starts the title with the settings this fork is played with (README.md, "How to run it").
#   scripts\play.ps1 -Exe <path of your own copy's executable>
# -Safe         without the newer, faster ways (the first thing to try when something is wrong)
# -RayTracing   the title's own ray-traced indirect light: lit interiors, about a third slower
# -Build        target\diag (the default, the build all measurements are of) or target\release
# -Console      auto (the default: Series S on a PC with 32 GB of RAM or less, else Series X), series-x or series-s
# -Output       the title's output size: 1280x720, 1664x936, 1920x1080, 2560x1440 or 3840x2160.
#               Series X profile only: the Series S profile runs at 1920x1080 (rendering 1280x720) and
#               crashes with another size, so the program leaves it out there.
# -Ports        the renderer patches 1, 2, 5, 6 and 7 of the README, all on (off by default while they are
#               tested; together 57-75 ms became about 45 ms a frame on the PC they were measured on)
# -Set          other settings, as NAME=VALUE with commas between: -Set VICTORY_PANEL=1,VICTORY_D3D12_DEBUG=1
#               (this is the form that works through play.cmd)
# -More         the same as a table, for calls of this script from PowerShell itself: -More @{VICTORY_PANEL='1'}
#               (through play.cmd a table arrives as text and is refused)
# Run it from a normal window, not as administrator: the driver then keeps its shader cache elsewhere.
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Safe,
    [switch]$RayTracing,
    [switch]$Ports,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [ValidateSet('auto', 'series-x', 'series-s')][string]$Console = 'auto',
    [string]$Output = '',
    [string[]]$Set = @(),
    [hashtable]$More = @{}
)
# (From play.cmd "-Set A=1,B=2" arrives as one text, from PowerShell as a list.)
foreach ($pair in ($Set -join ',').Split(',', [StringSplitOptions]::RemoveEmptyEntries)) {
    $name, $value = $pair.Trim().Split('=', 2)
    if (-not $value -or $name -notmatch '^VICTORY_[A-Z0-9_]+$') {
        Write-Host "-Set takes NAME=VALUE with commas between, the names starting with VICTORY_: '$pair' is not one."
        exit 1
    }
    $More[$name] = $value
}
$settings = @{
    VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_RELAY = '2'; VICTORY_WIDE_BACK = '1'; VICTORY_CARD_REHASH_RUNS = '1'
    VICTORY_JOIN_COPIES = '1'; VICTORY_INDIRECT_WINDOW = '1'; VICTORY_RELAY_COPY = '1'; VICTORY_CARD_STEPS = '2'
    VICTORY_INDIRECT_BATCH = '2'; VICTORY_MAP_THREADS = '1'; VICTORY_TITLE_PRIORITIES = '0'; VICTORY_CARD_AWAKE = '1'
    VICTORY_SHOW_AFTER = '1'; VICTORY_AMBIENT = '0.8'; VICTORY_D3D12_DRED = 'light'
    # The pipelines the title creates are written down for "victory warm", which makes them ahead of a run.
    VICTORY_PIPELINE_BOOK = '1'
}
if ($Safe) {
    $settings.VICTORY_UNDER_WIDE = '0'; $settings.VICTORY_LAND_ECHO = '0'
} else {
    $settings.VICTORY_THIN = '1'; $settings.VICTORY_JOIN_PARTS = '8'; $settings.VICTORY_UPLOAD_AHEAD = '1'
}
if ($RayTracing) { $settings.VICTORY_BVH_BUILDER = '1' }
if ($Output) { $settings.VICTORY_OUTPUT = $Output }
if ($Ports) {
    $settings.VICTORY_PORT_CONSTANTS = '1'; $settings.VICTORY_PORT_DESCRIPTORS = '1'; $settings.VICTORY_PORT_CBV = '1'
    $settings.VICTORY_PORT_GUARD = '1'; $settings.VICTORY_PORT_JOURNAL = '1'
}
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }
# (Without --console the program chooses by this PC's memory.)
$consoleArgs = @()
if ($Console -ne 'auto') { $consoleArgs = @('--console', $Console) }

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; exit 1 }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; exit 1 }
# (Other settings left over in this window go; where your files are stays.)
$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR'
$inherited = @(Get-ChildItem Env:VICTORY_* | Where-Object { $_.Name -notin $paths })
foreach ($entry in $inherited) { Remove-Item "Env:$($entry.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$discarded = $inherited | Where-Object {
    $_.Name -notin $More.Keys -and -not $_.Value.Equals([Environment]::GetEnvironmentVariable($_.Name))
} | Sort-Object Name
if ($discarded) {
    Write-Host "Discarded inherited values for $($discarded.Name -join ', '). Use -Set NAME=VALUE to pass them explicitly."
}
$Exe = (Resolve-Path $Exe).Path
Push-Location (Join-Path $PSScriptRoot '..')   # victory.toml is looked for here
try { & $victory run $Exe --execute @consoleArgs } finally { Pop-Location }
