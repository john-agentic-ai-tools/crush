#!/usr/bin/env pwsh

# Consolidated prerequisite checking script (PowerShell)
#
# This script provides unified prerequisite checking for Spec-Driven Development workflow.
# It replaces the functionality previously spread across multiple scripts.
#
# Usage: ./check-prerequisites.ps1 [OPTIONS]
#
# OPTIONS:
#   -Json               Output in JSON format
#   -RequireTasks       Require tasks.md to exist (for implementation phase)
#   -IncludeTasks       Include tasks.md in AVAILABLE_DOCS list
#   -PathsOnly          Only output path variables (no validation)
#   -Help, -h           Show help message

[CmdletBinding()]
param(
    [switch]$Json,
    [switch]$RequireTasks,
    [switch]$IncludeTasks,
    [switch]$PathsOnly,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

# Show help if requested
if ($Help) {
    Write-Output @"
Usage: check-prerequisites.ps1 [OPTIONS]

Consolidated prerequisite checking for Spec-Driven Development workflow.

OPTIONS:
  -Json               Output in JSON format
  -RequireTasks       Require tasks.md to exist (for implementation phase)
  -IncludeTasks       Include tasks.md in AVAILABLE_DOCS list
  -PathsOnly          Only output path variables (no prerequisite validation)
  -Help, -h           Show this help message

EXAMPLES:
  # Check task prerequisites (plan.md required)
  .\check-prerequisites.ps1 -Json
  
  # Check implementation prerequisites (plan.md + tasks.md required)
  .\check-prerequisites.ps1 -Json -RequireTasks -IncludeTasks
  
  # Get feature paths only (no validation)
  .\check-prerequisites.ps1 -PathsOnly

"@
    exit 0
}

# Source common functions
. "$PSScriptRoot/common.ps1"

# Centralized output object
$outputObject = [PSCustomObject]@{
    Success = $true
    Messages = @()
    REPO_ROOT = $null
    BRANCH = $null
    HAS_GIT = $null
    FEATURE_DIR = $null
    FEATURE_SPEC = $null
    IMPL_PLAN = $null
    TASKS = $null
    RESEARCH = $null # Initialize optional paths to null
    DATA_MODEL = $null
    QUICKSTART = $null
    CONTRACTS_DIR = $null # Only if it exists and has files
    AVAILABLE_DOCS = @()
}

# Custom function for logging errors/warnings into the JSON messages array or console
function Log-Message {
    param (
        [string]$Type, # 'Error', 'Warning', 'Info'
        [string]$Message
    )
    if ($Json) {
        $script:outputObject.Messages += [PSCustomObject]@{ Type = $Type; Text = $Message }
        if ($Type -eq 'Error') { $script:outputObject.Success = $false }
    } else {
        switch ($Type) {
            'Error'   { Write-SpecifyError $Message }
            'Warning' { Write-SpecifyWarning $Message }
            'Info'    { Write-SpecifyInfo $Message }
            default   { Write-Host $Message } # Still use Write-Host for default
        }
    }
}

# Get feature paths and validate branch
$paths = Get-FeaturePathsEnv

$outputObject.REPO_ROOT = $paths.REPO_ROOT
$outputObject.BRANCH = $paths.CURRENT_BRANCH
$outputObject.HAS_GIT = $paths.HAS_GIT
$outputObject.FEATURE_DIR = $paths.FEATURE_DIR
$outputObject.FEATURE_SPEC = $paths.FEATURE_SPEC
$outputObject.IMPL_PLAN = $paths.IMPL_PLAN
$outputObject.TASKS = $paths.TASKS

if (-not (Test-FeatureBranch -Branch $paths.CURRENT_BRANCH -HasGit:$paths.HAS_GIT)) { 
    Log-Message -Type 'Error' -Message "Branch validation failed for $($paths.CURRENT_BRANCH)."
    if ($Json) { ConvertTo-Json -InputObject $outputObject -Compress; exit 1 } else { exit 1 }
}



# Validate required directories and files
if (-not (Test-Path $paths.FEATURE_DIR -PathType Container)) {
    Log-Message -Type 'Error' -Message "Feature directory not found: $($paths.FEATURE_DIR)"
    Log-Message -Type 'Info' -Message "Run /speckit.specify first to create the feature structure."
    if ($Json) { ConvertTo-Json -InputObject $outputObject -Compress; exit 1 } else { exit 1 }
}

if (-not (Test-Path $paths.IMPL_PLAN -PathType Leaf)) {
    Log-Message -Type 'Error' -Message "plan.md not found in $($paths.FEATURE_DIR)"
    Log-Message -Type 'Info' -Message "Run /speckit.plan first to create the implementation plan."
    if ($Json) { ConvertTo-Json -InputObject $outputObject -Compress; exit 1 } else { exit 1 }
}

# Check for tasks.md if required
if ($RequireTasks -and -not (Test-Path $paths.TASKS -PathType Leaf)) {
    Log-Message -Type 'Error' -Message "tasks.md not found in $($paths.FEATURE_DIR)"
    Log-Message -Type 'Info' -Message "Run /speckit.tasks first to create the task list."
    if ($Json) { ConvertTo-Json -InputObject $outputObject -Compress; exit 1 } else { exit 1 }
}

# Populate available document paths in outputObject
if (Test-Path $paths.RESEARCH) { 
    $outputObject.RESEARCH = $paths.RESEARCH;
    $outputObject.AVAILABLE_DOCS += $paths.RESEARCH
}
if (Test-Path $paths.DATA_MODEL) { 
    $outputObject.DATA_MODEL = $paths.DATA_MODEL;
    $outputObject.AVAILABLE_DOCS += $paths.DATA_MODEL
}
# Check contracts directory (only if it exists and has files)
if ((Test-Path $paths.CONTRACTS_DIR) -and (Get-ChildItem -Path $paths.CONTRACTS_DIR -ErrorAction SilentlyContinue | Select-Object -First 1)) { 
    $outputObject.CONTRACTS_DIR = $paths.CONTRACTS_DIR;
    $outputObject.AVAILABLE_DOCS += $paths.CONTRACTS_DIR 
}
if (Test-Path $paths.QUICKSTART) { 
    $outputObject.QUICKSTART = $paths.QUICKSTART;
    $outputObject.AVAILABLE_DOCS += $paths.QUICKSTART 
}
# Include tasks.md if requested and it exists
if ($IncludeTasks -and (Test-Path $paths.TASKS)) { 
    $outputObject.AVAILABLE_DOCS += $paths.TASKS 
}

# Output results
if ($Json) {
    ConvertTo-Json -InputObject $outputObject -Compress
} else {
    # Original text output logic, or simplified version
    Write-Output "REPO_ROOT: $($outputObject.REPO_ROOT)"
    Write-Output "BRANCH: $($outputObject.BRANCH)"
    Write-Output "FEATURE_DIR: $($outputObject.FEATURE_DIR)"
    Write-Output "FEATURE_SPEC: $($outputObject.FEATURE_SPEC)"
    Write-Output "IMPL_PLAN: $($outputObject.IMPL_PLAN)"
    Write-Output "TASKS: $($outputObject.TASKS)"
    # Iterate and print messages
    foreach ($msg in $outputObject.Messages) {
        # Need to use the newly named functions here
        switch ($msg.Type) {
            'Error'   { Write-SpecifyError $msg.Text }
            'Warning' { Write-SpecifyWarning $msg.Text }
            'Info'    { Write-SpecifyInfo $msg.Text }
            default   { Write-Host $msg.Text }
        }
    }
    Write-Output "AVAILABLE_DOCS:"
    foreach ($doc in $outputObject.AVAILABLE_DOCS) {
        Write-Output "  - $doc"
    }
}