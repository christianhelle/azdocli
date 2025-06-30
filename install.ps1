# Azure DevOps CLI (azdocli) PowerShell Installer
# This script downloads and installs the latest release of azdocli from GitHub
# 
# Usage examples:
#   .\install.ps1                                    # Install latest version
#   .\install.ps1 -Version "1.0.0"                  # Install specific version
#   .\install.ps1 -Version "v1.0.0"                 # Install specific version (v prefix auto-removed)
#   .\install.ps1 -InstallPath "C:\tools\bin"       # Install to custom directory
#   .\install.ps1 -AddToPath:$false                 # Don't add to PATH

param(
    [string]$InstallPath = "$env:USERPROFILE\.local\bin",
    [switch]$AddToPath = $true,
    [string]$Version = ""
)

# Configuration
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # Disable progress bar for faster downloads

$Repo = "christianhelle/azdocli"
$BinaryName = "azdocli.exe"

# Functions
function Write-ColorText {
    param(
        [string]$Text,
        [string]$Color = "White"
    )
    Write-Host $Text -ForegroundColor $Color
}

function Write-Info {
    param([string]$Message)
    Write-ColorText "[INFO] $Message" -Color Cyan
}

function Write-Success {
    param([string]$Message)
    Write-ColorText "[SUCCESS] $Message" -Color Green
}

function Write-Warning {
    param([string]$Message)
    Write-ColorText "[WARNING] $Message" -Color Yellow
}

function Write-Error {
    param([string]$Message)
    Write-ColorText "[ERROR] $Message" -Color Red
    exit 1
}

# Detect architecture
function Get-Platform {
    $arch = if ([Environment]::Is64BitOperatingSystem) { 
        if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
    } else { 
        "x64"  # Default to x64 even for 32-bit systems
    }
    
    return "windows-$arch"
}

# Get latest release version from GitHub or use provided version
function Get-Version {
    param([string]$ProvidedVersion)
    
    if (-not [string]::IsNullOrWhiteSpace($ProvidedVersion)) {
        # Remove 'v' prefix if present, as GitHub releases use versions without 'v'
        if ($ProvidedVersion -match '^v(.+)') {
            $ProvidedVersion = $matches[1]
        }
        Write-Info "Using specified version: $ProvidedVersion"
        return $ProvidedVersion
    }
    
    Write-Info "Fetching latest release information..."
    
    try {
        $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        return $response.tag_name
    }
    catch {
        Write-Error "Failed to get latest version information: $($_.Exception.Message)"
    }
}

# Download and install
function Install-AzDoCli {
    param(
        [string]$Platform,
        [string]$Version
    )
    
    $downloadUrl = "https://github.com/$Repo/releases/download/$Version/$Platform.zip"
    $tempDir = [System.IO.Path]::GetTempPath()
    $zipFile = Join-Path $tempDir "$Platform.zip"
    $extractDir = Join-Path $tempDir "azdocli-$Version"
    
    Write-Info "Downloading azdocli $Version for $Platform..."
    
    try {
        # Download the zip file
        Invoke-WebRequest -Uri $downloadUrl -OutFile $zipFile -UseBasicParsing
        
        if (-not (Test-Path $zipFile) -or (Get-Item $zipFile).Length -eq 0) {
            Write-Error "Downloaded file is missing or empty"
        }
        
        Write-Info "Extracting azdocli..."
        
        # Extract the zip file
        if (Test-Path $extractDir) {
            Remove-Item $extractDir -Recurse -Force
        }
        
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [System.IO.Compression.ZipFile]::ExtractToDirectory($zipFile, $extractDir)
        
        # Find the binary
        $binaryFile = Get-ChildItem -Path $extractDir -Name $BinaryName -Recurse | Select-Object -First 1
        
        if (-not $binaryFile) {
            Write-Error "Binary $BinaryName not found in the downloaded archive"
        }
        
        # Get the full path to the found binary
        $binaryFileInfo = Get-ChildItem -Path $extractDir -Filter $BinaryName -Recurse | Select-Object -First 1
        $sourceBinary = $binaryFileInfo.FullName
        
        Write-Info "Installing azdocli to $InstallPath..."
        
        # Create install directory if it doesn't exist
        if (-not (Test-Path $InstallPath)) {
            New-Item -ItemType Directory -Path $InstallPath -Force | Out-Null
        }
        
        # Install the binary
        $targetBinary = Join-Path $InstallPath $BinaryName
        
        # Remove target if it exists as a directory (from previous failed installation)
        if (Test-Path $targetBinary -PathType Container) {
            Remove-Item $targetBinary -Recurse -Force
        }
        
        Copy-Item $sourceBinary $targetBinary -Force
        
        # Cleanup
        Remove-Item $zipFile -Force -ErrorAction SilentlyContinue
        Remove-Item $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        
        Write-Success "azdocli $Version installed successfully to $targetBinary"
        
        return $targetBinary
    }
    catch {
        Write-Error "Installation failed: $($_.Exception.Message)"
    }
}

# Add to PATH
function Add-ToPath {
    param([string]$Path)
    
    $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    
    if ($userPath -split ';' -contains $Path) {
        Write-Success "$Path is already in your PATH"
        return
    }
    
    Write-Info "Adding $Path to your PATH..."
    
    try {
        $newPath = if ($userPath) { "$userPath;$Path" } else { $Path }
        [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
        
        # Update current session PATH
        $env:PATH = "$env:PATH;$Path"
        
        Write-Success "Added $Path to your PATH"
        Write-Info "Please restart your PowerShell session or terminal for PATH changes to take effect"
    }
    catch {
        Write-Warning "Failed to add $Path to PATH: $($_.Exception.Message)"
        Write-Info "You can manually add $Path to your PATH environment variable"
    }
}

# Verify installation
function Test-Installation {
    param([string]$BinaryPath)
    
    if (Test-Path $BinaryPath) {
        try {
            $helpOutput = & $BinaryPath --help 2>&1 | Select-Object -First 1
            Write-Success "Installation verified: $helpOutput"
        }
        catch {
            Write-Warning "Binary installed but unable to verify: $($_.Exception.Message)"
        }
    }
    else {
        Write-Error "Installation verification failed: $BinaryPath not found"
    }
}

# Main installation process
function Main {
    Write-Host ""
    Write-ColorText "🚀 Azure DevOps CLI (azdocli) Installer" -Color Magenta
    Write-ColorText "=======================================" -Color Magenta
    Write-Host ""
    
    # Check PowerShell version
    if ($PSVersionTable.PSVersion.Major -lt 3) {
        Write-Error "PowerShell 3.0 or later is required"
    }
    
    # Detect platform
    $platform = Get-Platform
    Write-Info "Detected platform: $platform"
    
    # Get version (latest or specified)
    $version = Get-Version -ProvidedVersion $Version
    if (-not $version) {
        Write-Error "Failed to get version information"
    }
    Write-Info "Version to install: $version"
    
    # Download and install
    $installedBinary = Install-AzDoCli -Platform $platform -Version $version
    
    # Add to PATH if requested
    if ($AddToPath) {
        Add-ToPath -Path $InstallPath
    }
    
    # Verify installation
    Test-Installation -BinaryPath $installedBinary
    
    Write-Host ""
    Write-Success "Installation complete! 🎉"
    Write-Host ""
    Write-ColorText "Next steps:" -Color Yellow
    Write-Host "1. Run 'azdocli login' to authenticate with Azure DevOps"
    Write-Host "2. Run 'azdocli --help' to see available commands"
    Write-Host ""
    Write-Host "For more information, visit: https://github.com/$Repo"
    Write-Host ""
}

# Run the installer
try {
    Main
}
catch {
    Write-Error "Installation failed: $($_.Exception.Message)"
}