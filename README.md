# PioCare

**RO** · [EN](#english)

Program **gratuit** de întreținere și siguranță pentru Windows 10 și 11. Readuce calculatorul la performanța lui reală și îl face mai sigur pe internet, **fără să ascundă ce schimbă și cu posibilitatea de a anula orice**.

## Ce face

- **Întreținere**: fișiere temporare, cache-uri de browser, rapoarte de erori, descărcări vechi Windows Update, Coșul de reciclare, curățarea componentelor Windows (DISM).
- **Pornire**: tot ce pornește odată cu Windows (registru, folderul Startup, sarcini programate); dezactivare ca în Task Manager.
- **Dezinstalare**: rulează dezinstalatorul oficial al programului, apoi caută și curăță resturile (registru, foldere, scurtături, porniri automate).
- **Registru curat**: resturi rămase de la programe dezinstalate, doar când fișierul țintă sigur nu mai există.
- **Componente și servicii**: hartă cu dependențele serviciilor; propuneri pe profil (Acasă / Birou / Gaming) și după ce folosești. Un serviciu de care depinde altul activ nu este oprit niciodată, iar serviciile vitale nu se ating.
- **Firewall pentru siguranță**: blochează porți de intrare folosite de atacatori și unelte Windows abuzate de viruși; arată programele care ies pe internet.
- **„Ce este?”**: explicație pe înțelesul oricui pentru fiecare serviciu, program sau regulă.
- **Română și engleză**, cu alegerea limbii la prima pornire.

## Siguranța înainte de toate

- Înainte de orice schimbare se salvează starea de dinainte (registru, servicii, firewall complet) și, când se poate, un punct de restaurare Windows.
- Fiecare schimbare apare în **Jurnal și anulare** și se poate anula. Folderele rămase de la programe nu se șterg, ci se mută în carantină.
- Doar API-uri și unelte oficiale Windows. Fără drivere, fără servicii proprii în fundal, **fără date trimise pe internet**.
- „Cunoștințele” (ce se poate opri, ce resturi se caută, explicațiile) stau în pachete de reguli separate, **semnate digital**, care se pot actualiza fără reinstalare.

## Instalare

1. Descarcă `PioCare_1.0.3_x64-setup.exe` din [Releases](../../releases).
2. Instalatorul nu are (încă) o semnătură comercială de cod, așa că Windows SmartScreen poate afișa „Windows a protejat PC-ul”: apasă **Mai multe informații** → **Rulare oricum**.
3. Verifică, dacă vrei, că fișierul e cel original, în PowerShell:

   ```powershell
   Get-FileHash .\PioCare_1.0.3_x64-setup.exe -Algorithm SHA256
   ```

   Rezultatul trebuie să fie identic cu codul SHA-256 publicat la versiunea respectivă din Releases.

Manualul complet e în aplicație, la **Ajutor**.

## Pentru dezvoltatori

Tauri 2 + Rust, interfață HTML/JS simplă. Arhitectura și etapele: [ARHITECTURA.md](ARHITECTURA.md).

```
npm install
npm run dev      # fereastra de dezvoltare
npm run build    # instalatorul NSIS
cd src-tauri && cargo test
```

Pe discuri exFAT, compilatorul Rust poate eșua: `tools/tauri.cjs` mută folderul de build în `%LOCALAPPDATA%\PioCare-build`.

## Licență

Gratuit. © Piogrup. Vezi [LICENSE](LICENSE).

---

<a id="english"></a>
## English

**Free** maintenance and safety tool for Windows 10 and 11. It brings your PC back to its real performance and makes it safer online, **without hiding what it changes and with everything undoable**.

**Features:** maintenance (temp files, browser caches, error reports, old Windows Update downloads, Recycle Bin, DISM component cleanup) · startup programs (disable like Task Manager) · uninstaller with leftover cleanup · registry cleanup (only provably dead entries) · services with a dependency map and profile-based proposals (vital services never touched) · safety firewall rules and live list of programs going online · plain-language “What is this?” for every item · Romanian and English.

**Safety first:** the previous state is saved before every change (registry, services, full firewall export, restore point when possible), every change is listed in *Journal & undo* and can be reverted, leftover folders are quarantined rather than deleted, only official Windows APIs are used, nothing is sent over the internet, and the knowledge lives in **digitally signed** rule packs that update without reinstalling.

**Install:** download `PioCare_1.0.3_x64-setup.exe` from [Releases](../../releases). The installer has no commercial code signature yet, so SmartScreen may say “Windows protected your PC”: click **More info** → **Run anyway**. Check the file with `Get-FileHash .\PioCare_1.0.3_x64-setup.exe -Algorithm SHA256` against the SHA-256 published with the release. The full manual is in the app under **Help**.

**License:** free. © Piogrup. See [LICENSE](LICENSE).
