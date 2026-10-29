#Requires -Version 5.1
<#
.SYNOPSIS
Connects GitHub to Sliplane so that publishing a GitHub Release builds the image and deploys it.

.DESCRIPTION
CI already builds and pushes ghcr.io/nielsbosma/opensign:X.Y.Z, :X.Y and :latest for every published
release. This script stores the Sliplane deploy hook as the SLIPLANE_DEPLOY_HOOK repository secret,
which CI calls after the push. The hook never touches disk or the console.

.EXAMPLE
./deploy/Connect-Github.ps1
./deploy/Connect-Github.ps1 -Disconnect    # remove the secret, releases only build the image
#>
[CmdletBinding()]
param(
    [string]$Repo = 'nielsbosma/OpenSign',
    [switch]$Disconnect
)

$ErrorActionPreference = 'Continue'
$SecretName = 'SLIPLANE_DEPLOY_HOOK'
$CredFile = Join-Path $env:APPDATA 'OpenSign\credentials.xml'

function Step($text) { Write-Host "`n== $text" -ForegroundColor Cyan }
function Info($text) { Write-Host "   $text" }
function Ok($text) { Write-Host "   $text" -ForegroundColor Green }
function Warn($text) { Write-Host "   $text" -ForegroundColor Yellow }

function Confirm-Step($question, [bool]$default = $true) {
    $hint = if ($default) { '[Y/n]' } else { '[y/N]' }
    $answer = Read-Host "   $question $hint"
    if ([string]::IsNullOrWhiteSpace($answer)) { return $default }
    return $answer -match '^(y|yes|j|ja)$'
}

function Get-Plain([securestring]$secure) {
    [Net.NetworkCredential]::new('', $secure).Password
}

Step 'Checking the GitHub CLI'
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    Warn 'The GitHub CLI is missing. Install it with: winget install --id GitHub.cli'
    throw 'Install gh, open a new terminal and run this script again.'
}
gh auth status *> $null
if ($LASTEXITCODE -ne 0) {
    Info 'Signing in to GitHub (a browser window opens).'
    gh auth login --hostname github.com --git-protocol https --web
    if ($LASTEXITCODE -ne 0) { throw 'GitHub sign-in failed.' }
}
$permission = gh api "repos/$Repo" --jq '.permissions.admin' 2>$null
if ($permission -ne 'true') { throw "Your GitHub account needs admin access to $Repo to set secrets." }
Ok "Signed in with admin access to $Repo."

$existing = (gh secret list --repo $Repo --json name 2>$null | ConvertFrom-Json).name -contains $SecretName

if ($Disconnect) {
    if (-not $existing) { Info "$SecretName is not set. Releases already only build the image."; return }
    gh secret delete $SecretName --repo $Repo
    if ($LASTEXITCODE -ne 0) { throw 'Removing the secret failed.' }
    Ok "Removed $SecretName. Releases now build and push the image without deploying."
    return
}

if ($existing) {
    Warn "$SecretName is already set, so releases already deploy."
    if (-not (Confirm-Step 'Replace it with a new deploy hook?' $false)) { return }
}

Step 'Sliplane deploy hook'
Info 'The service must run the :latest image (or a series such as :0.1), because the hook redeploys'
Info 'whatever tag the service has. Check it in Sliplane -> OpenSign service -> Settings -> Image.'
Info 'Then copy the deploy hook from the same page (Settings -> Deploy hook).'

$hook = $null
if (Test-Path $CredFile) {
    $saved = Import-Clixml -ErrorAction SilentlyContinue $CredFile
    if ($saved.DeployHook -and (Confirm-Step 'Use the deploy hook saved by release.ps1?')) { $hook = $saved.DeployHook }
}
while (-not $hook) {
    $secure = Read-Host '   Paste the deploy hook URL (input is hidden)' -AsSecureString
    $plain = Get-Plain $secure
    if ([string]::IsNullOrWhiteSpace($plain)) { Info 'Nothing changed.'; return }
    if ($plain -match '^https://\S+$') { $hook = $secure }
    else { Warn 'A deploy hook is an https:// URL. Try again, or press Enter to stop.' }
}

Step "Saving $SecretName in GitHub"
Get-Plain $hook | gh secret set $SecretName --repo $Repo
if ($LASTEXITCODE -ne 0) { throw 'Saving the secret failed.' }
Ok "Saved. GitHub encrypts it and only shows it to the CI workflow."

if (Confirm-Step 'Call the hook once now to check it works? This redeploys the running service.' $false) {
    try {
        Invoke-RestMethod -ErrorAction Stop -Uri (Get-Plain $hook) -Method Get | Out-Null
        Ok 'Sliplane accepted the hook and is redeploying.'
    } catch {
        Warn "Sliplane refused the hook: $($_.Exception.Message). Copy it again and rerun this script."
    }
}

Step 'Done'
Info 'From now on: GitHub -> Releases -> Draft a new release -> tag vX.Y.Z on main -> Publish.'
Info 'CI runs every check, pushes the image and calls the deploy hook. Migrations run when it starts.'
Info "Follow it at https://github.com/$Repo/actions"
