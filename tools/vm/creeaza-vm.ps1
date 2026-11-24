# Creează mașina virtuală de test PioCare-Test în Hyper-V.
# Cere: utilizatorul curent în grupul Hyper-V Administrators și o imagine ISO Windows.
# Folosire: powershell -ExecutionPolicy Bypass -File creeaza-vm.ps1 -Iso "E:\ISO\Win10.iso"  (adaugă -Tpm doar pentru Windows 11)
param(
  [Parameter(Mandatory = $true)][string]$Iso,
  [string]$Name = "PioCare-Test",
  [string]$Folder = "E:\Hyper-V",
  [int]$DiskGB = 80,
  [int64]$MemoryGB = 4,
  [int]$Cpu = 2,
  [switch]$Tpm
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Iso)) { throw "Nu găsesc imaginea ISO: $Iso" }
if (Get-VM -Name $Name -ErrorAction SilentlyContinue) { throw "Mașina $Name există deja." }

New-Item -ItemType Directory -Force $Folder | Out-Null
$vhd = Join-Path $Folder "$Name.vhdx"

# Rețea: comutatorul implicit (NAT), creat automat de Windows.
$switch = (Get-VMSwitch -Name "Default Switch" -ErrorAction SilentlyContinue).Name

New-VM -Name $Name -Generation 2 -MemoryStartupBytes ($MemoryGB * 1GB) -NewVHDPath $vhd -NewVHDSizeBytes ($DiskGB * 1GB) -Path $Folder -SwitchName $switch | Out-Null
Set-VM -Name $Name -ProcessorCount $Cpu -DynamicMemory -MemoryMinimumBytes 2GB -MemoryMaximumBytes ($MemoryGB * 1GB) -AutomaticCheckpointsEnabled $false -CheckpointType Standard
if ($Tpm) {
  # Doar pentru o imagine Windows 11 (cere TPM). Windows 10 nu are nevoie.
  Set-VMKeyProtector -VMName $Name -NewLocalKeyProtector
  Enable-VMTPM -VMName $Name
}
Add-VMDvdDrive -VMName $Name -Path $Iso
$dvd = Get-VMDvdDrive -VMName $Name
Set-VMFirmware -VMName $Name -FirstBootDevice $dvd -EnableSecureBoot On -SecureBootTemplate MicrosoftWindows
Enable-VMIntegrationService -VMName $Name -Name "Guest Service Interface"

"Gata: $Name ($Cpu procesoare, până la $MemoryGB GB RAM, disc $DiskGB GB în $Folder)."
"Pornește-o din Hyper-V Manager (Connect -> Start) și instalează Windows; la cheie de produs alege: I don't have a product key."
