# PioCare — arhitectură și plan

## Problema de rezolvat

Utilitarele de întreținere și protecție îmbătrânesc repede: Windows se schimbă la fiecare actualizare,
apar alte servicii, alte chei de registru și alte amenințări. Un program care are „cunoștințele” scrise
direct în cod rămâne în urmă după câteva luni.

PioCare desparte deci două lucruri:

1. **Motorul** (aplicația): știe să citească sistemul, să facă copii de siguranță, să aplice o schimbare și să o anuleze.
2. **Regulile** (pachete de definiții): spun *ce* este de curățat, ce serviciu se poate opri și în ce condiții,
   ce reguli de firewall se recomandă. Se actualizează separat, fără reinstalarea aplicației.

## Tehnologia aleasă

| Parte | Alegere | De ce |
|---|---|---|
| Aplicația | **Tauri 2 + Rust** | Aceeași bază ca PioCut, deja cunoscută; executabil mic (câțiva MB), rapid, acces direct la API-urile Windows. |
| Interfața | HTML + CSS + JavaScript simplu, fără bundler | Ușor de modificat, fără dependențe care se învechesc. |
| Acces la Windows | Doar API-uri oficiale (registru, Service Control Manager, Windows Firewall, System Restore) | Nimic ascuns: fără drivere, fără motoare terțe. |
| Reguli | Fișiere JSON versionate, semnate | Pot fi actualizate oricând; semnătura (ECDSA P-256, ca la licențele PioCut) împiedică un pachet falsificat. |

## Module

1. **Întreținere generală** — fișiere temporare, cache-uri, jurnale vechi, Coșul de reciclare, curățarea componentelor Windows (prin DISM/Storage Sense, nu prin ștergeri brute).
2. **Registru curat** — găsește cheile rămase de la programele dezinstalate (intrări „Uninstall” orfane, căi către fișiere care nu mai există, intrări de pornire automată moarte). Se afișează, nu se șterg automat.
3. **Componente și servicii** — construiește graful de dependențe al serviciilor (în ambele sensuri: de cine depinde și cine depinde de el, din SCM și `DependOnService`). Un serviciu se propune spre oprire doar dacă regulile o permit **și** nimic din ce folosește utilizatorul nu depinde de el. Profiluri: „Acasă”, „Birou”, „Gaming”.
4. **Firewall pentru siguranță** — politică implicit restrictivă la intrare, blocarea telemetriei opționale și a porturilor riscante, avertizare la programe noi care ies pe internet. Totul prin Windows Firewall, fără înlocuirea lui.
5. **Siguranță și jurnal** — istoricul tuturor schimbărilor, cu anulare pas cu pas.

## Reguli de siguranță (obligatorii, în motor)

Înainte de orice schimbare:

- se încearcă un **punct de restaurare** Windows. Atenție: Protecția sistemului poate fi oprită pe C:, iar Windows permite implicit doar un punct la 24 de ore. De aceea PioCare are **și copii proprii**:
- **registru**: exportul cheilor care urmează să fie schimbate;
- **servicii**: tipul de pornire al fiecărui serviciu atins;
- **firewall**: export complet (`netsh advfirewall export`);
- totul intră într-un **jurnal** cu buton de anulare.

Alte principii:

- modul implicit este **doar citire**; nicio schimbare fără confirmare;
- interfața rulează fără drepturi de administrator; schimbările trec printr-o etapă separată, ridicată (UAC);
- fiecare propunere spune *de ce* (ce regulă a declanșat-o) și ce risc are.

## Motorul de schimbări (0.2)

Orice schimbare este un **plan** (o listă de acțiuni elementare) trecut prin `src-tauri/src/engine.rs`:

1. se salvează starea de dinainte pentru **toți** pașii (valoarea exactă de registru, tip + octeți; cheile întregi cu `reg export`; tipul de pornire al serviciilor). Dacă o copie eșuează, nu se schimbă nimic;
2. pentru schimbările de sistem se încearcă un punct de restaurare Windows (rezultatul se scrie în jurnal);
3. se aplică pașii; dacă unul eșuează, cei deja aplicați se readuc automat;
4. „Anulează” readuce pașii în ordine inversă.

Acțiuni disponibile: setare/ștergere valoare de registru, ștergere cheie, tip de pornire serviciu (prin Service Control Manager).
Refuzuri automate: căi de registru prea generale (mai puțin de două niveluri) și drivere (servicii cu pornire boot/system).

Jurnalul: `%LOCALAPPDATA%\PioCare\journal\<id>\operation.json`, lângă fișierele de copie. Se rescrie după fiecare pas.

**Copie completă**: punct de restaurare + `netsh advfirewall export` + export `HKLM\SYSTEM\CurrentControlSet\Services`.

**Administrator**: interfața rulează ca utilizator. Când un plan atinge HKLM sau servicii, PioCare se repornește pe sine cu
`--elevated <cerere> <rezultat>` (fereastra UAC), execută doar acel plan și se închide.

Testat pe PC-ul de lucru doar în `HKCU\Software\PioCare` (fără risc). Partea cu administrator a fost testată pe 24 nov 2026
în mașina virtuală Hyper-V „PioCare-Test” (Windows 10 LTSC 2021): copie completă (firewall + servicii), pornirea Protecției sistemului
cu primul punct de restaurare, schimbarea tipului de pornire al unui serviciu (WSearch) și anularea ei. Script VM: `tools/vm/creeaza-vm.ps1`.

**Protecția sistemului**: pe un Windows proaspăt e adesea oprită. PioCare o detectează și oferă pornirea ei prin jurnal (cu anulare).
Limita Windows de un punct la 24 de ore este tratată ca informație, nu ca eroare.

## Pachetele de reguli

- format: `rules/*.json`, cu `schema_version` (formatul) și `version` (data pachetului);
- pachetul de bază este inclus în aplicație, ca rezervă;
- un pachet mai nou se poate pune în `%ProgramData%\PioCare\rules` (descărcat sau copiat manual) și are prioritate dacă semnătura e validă;
- dacă `schema_version` e mai mare decât știe motorul, pachetul e ignorat și se cere actualizarea aplicației.

## Structura depozitului

```
PioCare/
  src/            interfața (index.html, app.js, style.css)
  src-tauri/      motorul Rust (comenzi, acces Windows)
  rules/          pachetele de reguli JSON
  ARHITECTURA.md  acest document
```

## Etape

| Etapă | Conținut |
|---|---|
| 0.1 | Schelet: dashboard care doar citește (versiune Windows, spațiu liber, programe instalate, pachet de reguli). |
| 0.2 ✓ | Jurnal + copii de siguranță + anulare (fundația pentru orice schimbare). Vezi mai jos. |
| 0.3 ✓ | Registru curat: dezinstalări orfane, porniri automate moarte, App Paths, SharedDLLs; ștergere prin jurnal. Testat în VM: scanare → curățare → anulare. |
| 0.4 ✓ | Servicii: hartă cu dependențe în ambele sensuri, profiluri Acasă/Birou/Gaming, „ce folosești”, oprire doar fără dependenți activi, listă de servicii vitale neatinse. Testat în VM: aplicare 31 schimbări → anulare. |
| 0.5 ✓ | Firewall: 10 porți de intrare riscante + 12 unelte Windows abuzate de viruși blocate (doar reguli „PioCare-*”), export înainte, programe noi care ies pe internet + blocare. Testat în VM: 20 reguli aplicate, navigarea merge, anulare exactă (430→410). |
| 0.6 ✓ | Întreținere generală: 16 categorii (temporare, cache-uri browsere, rapoarte de erori, Windows Update, Coș, Delivery Optimization, DISM); fără legături, fișierele în uz sărite, browserele doar închise. |
| 0.7 ✓ | Limbi: română + engleză (interfață `src/i18n/*.js`, motor `i18n::t`, pachete de reguli cu texte `{ro, en}`), alegerea limbii la prima pornire (și în instalator), schimbabilă oricând; textele în fișiere separate, ca să se poată adăuga alte limbi (ca la PioCut). |
| 0.8 ✓ | „Ce este?” pentru fiecare serviciu, proces, regulă și program din listele PioCare: ce face, dacă e sigur de oprit, ce se strică. Explicațiile stau în pachetele de reguli (RO/EN), deci se actualizează odată cu ele. |
| 0.9 ✓ | Dezinstalator încorporat: lista programelor → dezinstalatorul oficial al programului → scanare imediată a resturilor (registru, foldere, scurtături, porniri automate, servicii) → ștergere doar cu copie și anulare → starea sistemului după. Componentele comune/sistem nu se ating. |
| 0.10 | Programe la pornire: tot ce pornește odată cu Windows (chei Run, foldere Startup, sarcini programate la autentificare, starea StartupApproved), cu producător, impact și „Ce este?”; dezactivare sau amânare (nu ștergere) prin jurnal, cu anulare. |
| 0.11 | Pachete de reguli semnate și actualizare separată. |
| 1.0 | Instalator semnat, manual, prezentare. |
