// PioCare manual in English (“Help” page).
(window.PIOCARE_HELP = window.PIOCARE_HELP || {}).en = `
<h2>What PioCare does</h2>
<p>PioCare brings your computer back to its real performance and makes it safer online, without hiding what it changes. It is free, independent and uses only official Windows tools.</p>

<h2>The golden rule: everything can be undone</h2>
<p>Before any change, PioCare saves exactly how things were: registry values, service startup types, the complete firewall. When possible it also makes a Windows restore point. Every change is listed in <b>Journal &amp; undo</b>, with an <b>Undo</b> button. The only exception is cleaning temporary files: no undo is needed there, because Windows and programs recreate them.</p>
<p>When a change affects the system (not just your account), Windows asks for administrator confirmation. If you decline, nothing changes.</p>

<h2>Dashboard</h2>
<p>The status at a glance: Windows version, free space, number of installed programs, last full backup and rule packs.</p>

<h2>Maintenance</h2>
<p>Press <b>Scan</b> to see how much space can be freed (scanning only reads). Tick what you want and press <b>Clean selected</b>. Documents, photos, passwords and browser history are not touched. A browser's cache is cleaned only when the browser is fully closed.</p>

<h2>Startup</h2>
<p>Everything that starts on its own with Windows. <b>Disable at startup</b> works like Task Manager: the program stays installed and you can still start it manually. Use <b>ⓘ What is this?</b> if you don't know an item.</p>

<h2>Uninstall</h2>
<p>Choose the program and press <b>Uninstall</b>. PioCare starts the program's official uninstaller, then looks for leftovers: registry keys, folders, shortcuts, startup entries. Folders are not deleted but moved to quarantine: you can restore them from the journal, and after 7 days delete them for good from Maintenance. Shared components (Visual C++, .NET, drivers) are flagged: only uninstall them if you are sure.</p>

<h2>Registry cleanup</h2>
<p>Looks for registry leftovers of uninstalled programs. An entry is shown only if the file it points to is <b>certainly</b> gone. Deletion is done with a backup.</p>

<h2>Components &amp; services</h2>
<p>Pick a profile (Home, Office, Gaming) and tick what you use (printer, Bluetooth, Xbox…). PioCare only proposes services you don't need. <b>Ticked means it will be changed</b>; unticked means it is left alone. A service another active one depends on is never disabled, only set to “manual”. Vital Windows services are never touched. Changes take effect after a restart.</p>

<h2>Firewall</h2>
<p>PioCare works on top of Windows Firewall and only adds blocks: entry points used by attackers and Windows tools malware uses to download more malware. Browsing, updates and games are not affected. Below you see the programs online right now; new ones are marked “new” and can be blocked with one click.</p>

<h2>Journal &amp; undo</h2>
<p>The history of all changes. <b>Full backup now</b> makes a restore point and saves the firewall and service configuration. If System Protection is off, PioCare offers to turn it on.</p>

<h2>Rule packs</h2>
<p>PioCare's knowledge (what can be turned off, which leftovers to look for, the explanations) is kept separate from the program so it can be updated without reinstalling. A new pack goes into <code>C:\\ProgramData\\PioCare\\rules</code> and is used only if it is digitally signed by Piogrup and newer than the built-in one.</p>

<h2>Installing: the Windows warning</h2>
<p>The PioCare installer does not (yet) carry a commercial code signature, so Windows SmartScreen may show “Windows protected your PC”. Click <b>More info</b>, then <b>Run anyway</b>. To make sure the file is the original, compare the SHA-256 published on GitHub with the one from <code>Get-FileHash PioCare_1.0.0_x64-setup.exe</code> in PowerShell.</p>

<h2>Where PioCare keeps its data</h2>
<ul>
<li>Journal and backups: <code>%LOCALAPPDATA%\\PioCare\\journal</code></li>
<li>Quarantine: <code>%LOCALAPPDATA%\\PioCare\\carantina</code>, <code>C:\\ProgramData\\PioCare\\carantina</code></li>
<li>Settings (language): <code>%LOCALAPPDATA%\\PioCare\\settings.json</code></li>
</ul>
<p>PioCare sends nothing over the internet.</p>
`;
