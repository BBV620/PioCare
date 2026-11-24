// Manualul PioCare în română (pagina „Ajutor”).
(window.PIOCARE_HELP = window.PIOCARE_HELP || {}).ro = `
<h2>Ce face PioCare</h2>
<p>PioCare readuce calculatorul la performanța lui reală și îl face mai sigur pe internet, fără să ascundă ce schimbă. Este gratuit, independent și lucrează doar cu uneltele oficiale ale Windows.</p>

<h2>Regula de aur: totul se poate anula</h2>
<p>Înainte de orice schimbare, PioCare salvează exact cum era: valorile de registru, tipul de pornire al serviciilor, firewall-ul complet. Când e posibil face și un punct de restaurare Windows. Fiecare schimbare apare în <b>Jurnal și anulare</b>, cu butonul <b>Anulează</b>. Singura excepție este curățarea fișierelor temporare: acolo nu e nevoie de anulare, pentru că Windows și programele le refac singure.</p>
<p>Când o schimbare atinge sistemul (nu doar contul tău), Windows îți cere confirmarea de administrator. Dacă refuzi, nu se schimbă nimic.</p>

<h2>Panou</h2>
<p>Starea pe scurt: versiunea Windows, spațiul liber, numărul de programe instalate, ultima copie completă și pachetele de reguli.</p>

<h2>Întreținere</h2>
<p>Apasă <b>Scanează</b> ca să vezi cât spațiu se poate elibera (scanarea doar citește). Bifează ce vrei și apasă <b>Curăță selectate</b>. Documentele, pozele, parolele și istoricul browserelor nu sunt atinse. Cache-ul unui browser se curăță doar când browserul e închis complet.</p>

<h2>Pornire</h2>
<p>Tot ce pornește singur odată cu Windows. <b>Dezactivează la pornire</b> funcționează ca în Task Manager: programul rămâne instalat și îl poți porni oricând manual. Folosește butonul <b>ⓘ Ce este?</b> dacă nu știi ce e un element.</p>

<h2>Dezinstalare</h2>
<p>Alege programul și apasă <b>Dezinstalează</b>. PioCare pornește dezinstalatorul oficial al programului, apoi caută resturile: chei de registru, foldere, scurtături, porniri automate. Folderele nu se șterg, ci se mută în carantină. Le poți readuce din jurnal, iar după 7 zile le poți șterge definitiv din Întreținere. Componentele comune (Visual C++, .NET, drivere) sunt marcate: dezinstalează-le doar dacă știi sigur.</p>

<h2>Registru curat</h2>
<p>Caută resturi rămase în registru de la programe dezinstalate. O intrare apare doar dacă fișierul către care trimite <b>sigur</b> nu mai există. Ștergerea se face cu copie.</p>

<h2>Componente și servicii</h2>
<p>Alege profilul (Acasă, Birou, Gaming) și bifează ce folosești (imprimantă, Bluetooth, Xbox…). PioCare propune doar servicii de care nu ai nevoie. <b>Bifat înseamnă că se schimbă</b>, iar debifat că rămâne neatins. Un serviciu de care depinde altul activ nu e oprit niciodată, ci lăsat pe „manual”. Serviciile vitale ale Windows nu se ating deloc. Schimbările au efect după repornire.</p>

<h2>Firewall</h2>
<p>PioCare lucrează peste Windows Firewall și doar adaugă blocări: porți de intrare folosite de atacatori și unelte Windows prin care virușii descarcă alți viruși. Navigarea, actualizările și jocurile nu sunt atinse. Jos vezi programele care ies acum pe internet; cele noi sunt marcate „nou” și pot fi blocate cu un clic.</p>

<h2>Jurnal și anulare</h2>
<p>Istoricul tuturor schimbărilor. <b>Copie completă acum</b> face un punct de restaurare și salvează firewall-ul și configurația serviciilor. Dacă Protecția sistemului e oprită, PioCare îți propune să o pornești.</p>

<h2>Pachete de reguli</h2>
<p>Cunoștințele PioCare (ce se poate opri, ce resturi se caută, explicațiile) stau separat de program, ca să poată fi actualizate fără reinstalare. Un pachet nou se pune în <code>C:\\ProgramData\\PioCare\\rules</code> și este folosit doar dacă e semnat digital de Piogrup și e mai nou decât cel inclus.</p>

<h2>Instalare: avertismentul Windows</h2>
<p>Instalatorul PioCare nu are (încă) semnătură comercială de cod, așa că Windows SmartScreen poate afișa „Windows a protejat PC-ul”. Apasă <b>Mai multe informații</b>, apoi <b>Rulare oricum</b>. Ca să fii sigur că fișierul e cel original, compară codul SHA-256 publicat pe GitHub cu cel calculat de comanda <code>Get-FileHash PioCare_1.0.0_x64-setup.exe</code> în PowerShell.</p>

<h2>Unde își ține PioCare datele</h2>
<ul>
<li>Jurnalul și copiile: <code>%LOCALAPPDATA%\\PioCare\\journal</code></li>
<li>Carantina: <code>%LOCALAPPDATA%\\PioCare\\carantina</code>, <code>C:\\ProgramData\\PioCare\\carantina</code></li>
<li>Setări (limba): <code>%LOCALAPPDATA%\\PioCare\\settings.json</code></li>
</ul>
<p>PioCare nu trimite nimic pe internet.</p>
`;
