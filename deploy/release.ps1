#Requires -Version 5.1
<#
.SYNOPSIS
Publishes an OpenSign release (which builds and pushes the Docker image) and deploys it to Sliplane.

.DESCRIPTION
Walks through each step and asks before anything that cannot be undone. The Sliplane API token and
deploy hook are saved encrypted for your Windows account (DPAPI) in %APPDATA%\OpenSign, so later runs
only ask once.

.EXAMPLE
./deploy/release.ps1
./deploy/release.ps1 -SkipRelease          # only deploy
./deploy/release.ps1 -ForgetCredentials    # delete the saved token and hook
#>
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$SkipRelease,
    [switch]$SkipDeploy,
    [switch]$ForgetCredentials
)

$ErrorActionPreference = 'Continue'
$Repo = 'nielsbosma/OpenSign'
$Image = 'ghcr.io/nielsbosma/opensign'
$Root = Split-Path $PSScriptRoot -Parent
$CredDir = Join-Path $env:APPDATA 'OpenSign'
$CredFile = Join-Path $CredDir 'credentials.xml'

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

function Read-Credentials {
    if (Test-Path $CredFile) { return Import-Clixml -ErrorAction Stop $CredFile }
    return @{}
}

function Save-Credentials($creds) {
    New-Item -ItemType Directory -Force -Path $CredDir | Out-Null
    $creds | Export-Clixml -Path $CredFile
    Ok "Saved encrypted to $CredFile (only your Windows account can read it)."
}

function Read-Secret($name, $prompt, $pattern) {
    while ($true) {
        $secure = Read-Host "   $prompt" -AsSecureString
        $plain = Get-Plain $secure
        if ([string]::IsNullOrWhiteSpace($plain)) { return $null }
        if ($plain -match $pattern) { return $secure }
        Warn "That does not look like a $name. Try again, or press Enter to skip."
    }
}

function Test-ImageTag($tag) {
    try {
        $token = (Invoke-RestMethod -ErrorAction Stop "https://ghcr.io/token?scope=repository:nielsbosma/opensign:pull").token
        $tags = Invoke-RestMethod -ErrorAction Stop 'https://ghcr.io/v2/nielsbosma/opensign/tags/list' -Headers @{ Authorization = "Bearer $token" }
        return $tags.tags -contains $tag
    } catch { return $false }
}

if ($ForgetCredentials) {
    if (Test-Path $CredFile) { Remove-Item $CredFile; Ok 'Deleted the saved Sliplane token and deploy hook.' }
    else { Info 'Nothing saved.' }
    return
}

if ($PSVersionTable.PSVersion.Major -ge 6 -and -not $IsWindows) {
    throw 'Saving credentials uses Windows DPAPI. On macOS or Linux, run deploy/sliplane.sh instead.'
}

Step 'Checking tools'
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
Ok 'GitHub CLI is signed in.'

if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
$Tag = "v$Version"

if (-not $SkipRelease) {
    Step "Release $Tag"

    $existing = gh release view $Tag --repo $Repo --json tagName 2>$null
    if ($LASTEXITCODE -eq 0 -and $existing) {
        Warn "$Tag is already released. Bump version in Cargo.toml to release again, or rerun with -SkipRelease."
        if (-not (Confirm-Step 'Skip the release and continue to deploy?')) { return }
    } else {
        $run = gh run list --repo $Repo --workflow CI --branch main --event push --limit 1 --json status,conclusion,displayTitle,url | ConvertFrom-Json | Select-Object -First 1
        if (-not $run) {
            Warn 'No CI run on main found.'
        } elseif ($run.status -ne 'completed') {
            Info "CI on main is still running: $($run.displayTitle)"
            if (Confirm-Step 'Wait for it to finish?') {
                gh run watch --repo $Repo ($run.url -split '/')[-1] --exit-status *> $null
                $run = gh run list --repo $Repo --workflow CI --branch main --event push --limit 1 --json status,conclusion,displayTitle,url | ConvertFrom-Json | Select-Object -First 1
            }
        }
        if ($run -and $run.conclusion -ne 'success') {
            Warn "The latest CI run on main is not green ($($run.conclusion)): $($run.url)"
            Warn 'A release from a red main builds an image from code that failed its checks.'
            if (-not (Confirm-Step 'Release anyway?' $false)) { return }
        } elseif ($run) {
            Ok 'CI on main is green.'
        }

        $changelog = Get-Content (Join-Path $Root 'CHANGELOG.md') -Raw
        $section = [regex]::Match($changelog, "(?ms)^## \[$([regex]::Escape($Version))\][^\n]*\n(.*?)(?=^## \[|\z)")
        if (-not $section.Success) {
            $section = [regex]::Match($changelog, '(?ms)^## \[Unreleased\][^\n]*\n(.*?)(?=^## \[|\z)')
            Warn "CHANGELOG.md has no [$Version] section, so the notes come from [Unreleased]."
        }
        $notesFile = New-TemporaryFile
        [IO.File]::WriteAllText($notesFile.FullName, $section.Groups[1].Value.Trim(), [Text.UTF8Encoding]::new($false))

        Info "This publishes $Tag from main on GitHub. CI then pushes $Image`:$Version, :$($Version -replace '\.\d+$','') and :latest."
        if (-not (Confirm-Step "Publish $Tag now?")) { Remove-Item $notesFile; return }
        gh release create $Tag --repo $Repo --target main --title $Tag --notes-file $notesFile.FullName
        $created = $LASTEXITCODE
        Remove-Item $notesFile
        if ($created -ne 0) { throw 'Creating the release failed.' }
        Ok "Released $Tag."

        if (Confirm-Step 'Wait for CI to build and push the image (about 15 minutes)?') {
            Start-Sleep -Seconds 15
            $releaseRun = gh run list --repo $Repo --workflow CI --event release --limit 1 --json databaseId,url | ConvertFrom-Json | Select-Object -First 1
            if ($releaseRun) {
                Info "Following $($releaseRun.url)"
                gh run watch --repo $Repo $releaseRun.databaseId --exit-status
                if ($LASTEXITCODE -ne 0) { throw "The release build failed: $($releaseRun.url)" }
            }
            if (Test-ImageTag $Version) { Ok "$Image`:$Version is published." }
            else { Warn "$Image`:$Version is not visible yet. Check the run before deploying." }
        }
    }
}

if ($SkipDeploy) { return }

Step 'Deploy to Sliplane'
$creds = Read-Credentials
Info '1) First install: creates the database, bucket, Gotenberg and OpenSign through the Sliplane API.'
Info '2) Update: calls the service''s deploy hook, which pulls the newest image for its tag.'
Info '3) Skip.'
$choice = Read-Host '   Choose 1, 2 or 3'

switch ($choice) {
    '1' {
        if (-not $creds.SliplaneToken) {
            Info 'Create an API token: Sliplane dashboard -> team settings -> API tokens.'
            Info 'It needs read and write for projects, servers, services, postgres and buckets, plus secrets:read.'
            Start-Process 'https://sliplane.io/app'
            $secure = Read-Secret 'Sliplane API token' 'Paste the token (input is hidden)' '^api_'
            if (-not $secure) { Warn 'No token, skipping the deploy.'; return }
            $creds.SliplaneToken = $secure
            Save-Credentials $creds
        } else {
            Ok 'Using the saved Sliplane token.'
        }

        $bash = @("$env:ProgramFiles\Git\bin\bash.exe", "${env:ProgramFiles(x86)}\Git\bin\bash.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
        if (-not $bash) { throw 'deploy/sliplane.sh needs Git Bash. Install it with: winget install --id Git.Git' }
        & $bash -lc 'command -v jq' *> $null
        if ($LASTEXITCODE -ne 0) { throw 'deploy/sliplane.sh needs jq. Install it with: winget install --id jqlang.jq, then open a new terminal.' }

        $env:SLIPLANE_TOKEN = Get-Plain $creds.SliplaneToken
        try {
            Push-Location $Root
            & $bash ./deploy/sliplane.sh
            if ($LASTEXITCODE -ne 0) { throw 'The Sliplane deploy failed. See the output above.' }
        } finally {
            Pop-Location
            Remove-Item Env:SLIPLANE_TOKEN -ErrorAction SilentlyContinue
        }

        Info 'Next, for one-click updates: Sliplane -> OpenSign service -> Settings -> Deploy hook.'
        Info 'Paste it below to save it here, and also into OpenSign -> Settings -> Updates.'
        $hook = Read-Secret 'Sliplane deploy hook URL' 'Deploy hook URL (Enter to skip)' '^https://'
        if ($hook) { $creds.DeployHook = $hook; Save-Credentials $creds }
    }
    '2' {
        if (-not $creds.DeployHook) {
            Info 'Copy the deploy hook: Sliplane -> OpenSign service -> Settings -> Deploy hook.'
            $hook = Read-Secret 'Sliplane deploy hook URL' 'Paste the deploy hook URL (input is hidden)' '^https://'
            if (-not $hook) { Warn 'No deploy hook, skipping the deploy.'; return }
            $creds.DeployHook = $hook
            Save-Credentials $creds
        } else {
            Ok 'Using the saved deploy hook.'
        }
        Invoke-RestMethod -ErrorAction Stop -Uri (Get-Plain $creds.DeployHook) -Method Get | Out-Null
        Ok 'Sliplane is redeploying. Migrations run when the new version starts.'
        Info 'The hook redeploys the service''s current tag, so the service should run :latest or a series such as :0.1.'
    }
    default { Info 'Skipped the deploy.' }
}

Step 'Done'
