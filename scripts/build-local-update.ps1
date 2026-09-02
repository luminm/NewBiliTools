param(
  [string]$Bundles = 'nsis'
)

$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$TauriDir = Join-Path $Root 'src-tauri'
$Cli = Join-Path $Root 'node_modules\.bin\tauri.cmd'
$ConfigPath = Join-Path $TauriDir 'tauri.conf.json'

$KeyDir = Join-Path $env:USERPROFILE '.bilitools-updater'
$KeyPath = Join-Path $KeyDir 'bilitools.key'
$PubPath = Join-Path $KeyDir 'bilitools.key.pub'
$PassPath = Join-Path $KeyDir 'bilitools.key.password'

if (-not (Test-Path -LiteralPath $KeyPath)) {
  New-Item -ItemType Directory -Force -Path $KeyDir | Out-Null
  $Bytes = New-Object byte[] 24
  [System.Security.Cryptography.RandomNumberGenerator]::Fill($Bytes)
  $Password = [Convert]::ToBase64String($Bytes)

  & $Cli signer generate --ci --password $Password -w $KeyPath
  if ($LASTEXITCODE -ne 0) {
    throw 'tauri signer generate failed'
  }

  Set-Content -LiteralPath $PassPath -Value $Password -Encoding ASCII -NoNewline
}

$Password = (Get-Content -Raw -LiteralPath $PassPath).Trim()
$ExpectedPub = (Get-Content -Raw -LiteralPath $PubPath).Trim()
$Config = Get-Content -Raw -LiteralPath $ConfigPath | ConvertFrom-Json

if ($Config.plugins.updater.pubkey.Trim() -ne $ExpectedPub) {
  throw "updater pubkey does not match $PubPath; update tauri.conf.json"
}

$env:TAURI_SIGNING_PRIVATE_KEY = $KeyPath
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $Password

Push-Location $Root
try {
  & $Cli build --ci --bundles $Bundles
  if ($LASTEXITCODE -ne 0) {
    throw 'tauri build failed'
  }
} finally {
  Pop-Location
}

Write-Host 'Local update build complete.'
Write-Host 'Start the local update server with: pnpm update:local'
