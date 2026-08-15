# Guida introduttiva

Questa guida porta OpenJobScout dall'installazione alla prima candidatura
tracciata. OpenJobScout non invia automaticamente candidature.

Gli esempi usano PowerShell su Windows. In macOS o Linux la sintassi della shell
cambia, ma la cartella dati predefinita resta `~/.openjobscout/`.

## 1. Requisiti

- Python 3.11 oppure 3.12
- `uv`
- connessione Internet per ricerca e verifica

Per installare `uv`, usa la
[guida ufficiale](https://docs.astral.sh/uv/getting-started/installation/).

## 2. Installazione

Installa il comando direttamente dal repository pubblico:

```powershell
uv tool install git+https://github.com/cmdr-chara/open-job-scout.git@v0.1.0
jobscout --help
```

Per lavorare sul codice:

```powershell
git clone https://github.com/cmdr-chara/open-job-scout.git
cd open-job-scout
uv sync --extra dev
uv run jobscout --help
```

## 3. Configurazione locale

```powershell
jobscout init
notepad "$env:USERPROFILE\.openjobscout\config.toml"
```

Percorsi predefiniti:

| Contenuto      | Percorso Windows                           |
| -------------- | ------------------------------------------ |
| Configurazione | `%USERPROFILE%\.openjobscout\config.toml`  |
| Database       | `%USERPROFILE%\.openjobscout\jobs.sqlite3` |
| Report         | `%USERPROFILE%\.openjobscout\reports\`     |

In macOS o Linux i percorsi equivalenti sono `~/.openjobscout/config.toml`,
`~/.openjobscout/jobs.sqlite3` e `~/.openjobscout/reports/`. I nuovi file di
configurazione e database usano permessi `0600` sui sistemi Unix-like, così non
sono leggibili dagli altri utenti locali per impostazione predefinita.

`jobscout init` non sovrascrive un file esistente. `--force` va usato soltanto
quando vuoi sostituirlo deliberatamente.

Il [template completo di configurazione](../examples/config.example.toml)
mostra tutte le sezioni disponibili.

## 4. Fonti e ricerche

```toml
[search]
terms = [
  "junior backend developer",
  "graduate software engineer",
]
sites = ["linkedin", "google"]
location = "Italy"
country_indeed = "Italy"
results_per_term = 20
max_age_days = 14
```

Inizia con poche query e volumi contenuti. Per Google, OpenJobScout passa
automaticamente ogni termine al motore di discovery JobSpy.
Indeed è temporaneamente disabilitato perché l'adapter upstream attuale non
verifica i certificati TLS; trovi i dettagli in
[SECURITY.md](../SECURITY.md).

## 5. Filtri e priorità

```toml
[filters]
require_remote = true
allowed_employment_types = ["fulltime", "internship", "contract", ""]
blocked_title_terms = ["senior", "staff", "principal", "director"]
blocked_description_terms = ["mandatory relocation"]
max_required_years = 3

[ranking]
preferred_title_terms = ["software engineer", "backend", "python"]
preferred_skills = ["python", "django", "fastapi", "postgresql", "docker"]
junior_signals = ["junior", "graduate", "entry level", "new grad"]
concern_signals = ["unpaid", "on-site only"]
```

```toml
[salary]
minimum_annual = 0
preferred_annual = 50000
unknown_policy = "allow"
unknown_penalty = 0
preferred_bonus = 10
```

Il punteggio ordina la coda da esaminare. Non è un punteggio ATS e non predice
la decisione dell'azienda.

## 6. Prima ricerca

```powershell
jobscout search
jobscout list --status new
```

Per evitare la verifica online degli URL:

```powershell
jobscout search --no-verify
```

Ogni ricerca e ogni importazione CSV crea anche un'istantanea Markdown con data
e ora nella cartella dei report configurata.

Per vedere tutti i dettagli di un risultato:

```powershell
jobscout show ID
```

L'ID breve viene mostrato dal comando `list`.
`show` stampa il record locale completo in JSON: non apre il browser.

La coda può essere filtrata per stato, modalità di lavoro, fonte, punteggio
minimo e testo libero, e ordinata per punteggio oppure per ultima scoperta:

```powershell
jobscout list --status new --work-mode remote --min-score 60 --query python
jobscout list --source linkedin --sort newest
```

## 7. Controllo manuale

Prima di candidarti, apri sempre il collegamento ufficiale e controlla:

- che l'offerta accetti ancora candidature;
- che azienda e ruolo siano legittimi;
- località, remoto e paesi ammessi;
- requisiti realmente obbligatori;
- RAL e forma contrattuale.

Google può mostrare risultati scaduti. Se la pagina ufficiale restituisce
`404`, `410`, mostra “Job not found” oppure l'offerta non esiste più nell'ATS,
OpenJobScout conserva lo storico con stato `closed`.

## 8. Stato della candidatura e cronologia

```powershell
jobscout mark ID reviewed
jobscout mark ID applied --note "Candidatura sul sito ufficiale"
jobscout mark ID interview --note "Colloquio tecnico venerdì"
jobscout mark ID rejected
jobscout mark ID offer
```

Stati disponibili:

```text
new
reviewed
applied
interview
rejected
offer
closed
stale
```

Una ricerca successiva non sovrascrive gli stati importanti come `applied`,
`interview`, `rejected` oppure `offer`.

Lo schema v3 conserva anche una cronologia persistente per ogni offerta:

```powershell
jobscout history ID
jobscout history ID --json
```

La cronologia registra scoperta, cambiamenti di verifica, transizioni
automatiche, modifiche manuali dello stato, note e uno snapshot iniziale quando
un database creato con una versione precedente viene migrato.

## 9. Riverifica senza rifare la ricerca

`recheck` serve a controllare nuovamente gli URL già presenti nel tracker senza
eseguire un'altra ricerca sui job board:

```powershell
jobscout recheck ID
jobscout recheck ID_ALTRO
```

Puoi anche riverificare una parte filtrata della coda:

```powershell
jobscout recheck --status new --work-mode remote --min-score 60
jobscout recheck --status closed --limit 20
```

Il limite predefinito è 50 per evitare raffiche involontarie di richieste.
`--workers` controlla il parallelismo della verifica.

La riverifica aggiorna stato di verifica, dati ATS e ranking, ma non modifica
`last_seen_at`: l'offerta non è stata riscoperta da una fonte. Un'offerta
`closed` automaticamente può tornare `new` se risulta di nuovo attiva. Gli
stati impostati manualmente restano invariati, e `stale` resta `stale` finché
una ricerca successiva non ritrova davvero l'offerta.

## 10. Report, export e statistiche

```powershell
jobscout report
jobscout report --status applied
jobscout report --status interview --output colloqui.md
jobscout export --status applied --format csv
jobscout export --work-mode remote --min-score 70 --format json
jobscout stats
```

Report Markdown ed export CSV/JSON accettano gli stessi filtri della coda.
`stats` riepiloga stati, fonti, modalità di lavoro, copertura salariale,
punteggio medio e migliori offerte nuove.

Il database SQLite resta la fonte principale.

## 11. Importazione CSV

```powershell
jobscout import-csv .\jobs.csv
jobscout import-csv .\jobs.csv --no-verify
```

Sono riconosciuti campi JobSpy come `title`, `company`, `job_url`,
`job_url_direct`, `location`, `is_remote`, `job_type`, `description`,
`date_posted`, `min_amount`, `max_amount` e `currency`.

Un CSV importato può contenere note personali o lo storico delle ricerche.
Tienilo fuori da Git: la cartella `data/` è già esclusa da `.gitignore` ed è un
buon posto locale se lavori dalla cartella del progetto.

## 12. Diagnostica locale

Prima di investigare manualmente un problema di ricerca o database, esegui:

```powershell
jobscout doctor
jobscout doctor --json
```

`doctor` controlla:

- validità della configurazione;
- fonti disabilitate come l'attuale adapter Indeed;
- compatibilità dello schema SQLite e `PRAGMA quick_check`;
- permessi dei file config/database sui sistemi Unix-like;
- possibilità di scrivere nella cartella report;
- disponibilità di JobSpy.

Non avvia una ricerca reale sui job board.

## Problemi comuni

### `jobscout` non viene riconosciuto

Riavvia il terminale oppure esegui il comando dalla cartella del progetto:

```powershell
uv run jobscout --help
```

### Una fonte non restituisce risultati

- Esegui prima `jobscout doctor`.
- Prova una sola query e una sola fonte.
- Riduci `results_per_term`.
- Controlla la località.
- Dopo un errore `429`, attendi prima di riprovare.
- Usa un'altra fonte oppure importa un CSV.

### Google mostra `Job not found`

È un risultato rimasto nell'indice. L'offerta deve restare `closed`; non usare
mirror sospetti per inviare dati personali. In seguito puoi usare
`jobscout recheck ID` per verificare se la pagina ufficiale è tornata attiva
senza alterare la data di ultima scoperta.

### Dove vengono salvati i dati personali?

Configurazione, database SQLite, report, note, cronologia eventi e CSV importati
restano sul tuo computer. Il programma effettua però richieste ai job board
configurati e agli URL pubblici dell'offerta o dell'ATS durante ricerca e
verifica. `recheck` esegue soltanto la parte di verifica degli URL pubblici.
OpenJobScout non carica il CV e non invia candidature. `.gitignore` esclude i
dati locali e `data/` dal repository.
