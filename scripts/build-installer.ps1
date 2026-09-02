# Builds the Windows NSIS installer with the current frontend dist.
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$TauriDir = Join-Path $Root 'src-tauri'
$Cli = Join-Path $Root 'node_modules\.bin\tauri.cmd'
$VueTsc = Join-Path $Root 'node_modules\.bin\vue-tsc.cmd'
$Vite = Join-Path $Root 'node_modules\.bin\vite.cmd'

foreach ($tool in @($Cli, $VueTsc, $Vite)) {
  if (-not (Test-Path -LiteralPath $tool)) {
    throw "Missing frontend tool: $tool. Run pnpm install first."
  }
}

$env:RUSTUP_HOME = Join-Path $env:USERPROFILE '.rustup'
$env:CARGO_HOME = Join-Path $env:USERPROFILE '.cargo'
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

Push-Location $Root
try {
  & $VueTsc --noEmit
  if ($LASTEXITCODE -ne 0) {
    throw 'vue-tsc --noEmit failed'
  }

  & $Vite build
  if ($LASTEXITCODE -ne 0) {
    throw 'vite build failed'
  }

  $config = Join-Path $env:TEMP "bilitools-tauri-$PID.json"
  $configJson = '{"build":{"beforeBuildCommand":""},"bundle":{"createUpdaterArtifacts":false}}'
  Set-Content -LiteralPath $config -Value $configJson -Encoding ASCII
  try {
    & $Cli build --ci --bundles nsis --config $config
    if ($LASTEXITCODE -ne 0) {
      throw 'tauri build failed'
    }
  } finally {
    Remove-Item -LiteralPath $config -Force -ErrorAction SilentlyContinue
  }
} finally {
  Pop-Location
}

$installer = Get-ChildItem `
  -Path (Join-Path $TauriDir 'target\release\bundle\nsis') `
  -Filter '*setup.exe' `
  -File -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $installer) {
  throw 'Installer not found after build'
}

Write-Host "Installer: $($installer.FullName)"
