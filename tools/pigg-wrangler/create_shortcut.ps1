# Creates a desktop shortcut for Pigg Wrangler
$WshShell = New-Object -ComObject WScript.Shell
$Desktop = $WshShell.SpecialFolders("Desktop")
$Shortcut = $WshShell.CreateShortcut("$Desktop\Pigg Wrangler.lnk")
$Shortcut.TargetPath = "$PSScriptRoot\PiggWrangler.bat"
$Shortcut.WorkingDirectory = $PSScriptRoot
$Shortcut.IconLocation = "$PSScriptRoot\piggwrangler.ico,0"
$Shortcut.Description = "Browse and extract City of Heroes PIGG archives"
$Shortcut.WindowStyle = 1
$Shortcut.Save()
Write-Host "Desktop shortcut created: $Desktop\Pigg Wrangler.lnk"
