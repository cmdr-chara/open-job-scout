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
uv tool install git+https://github.com/cmdr-chara/open-job-scout.git
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
`~/.openjobscout/jobs.sqlite3` e `~/.openjobscout/reports/`.

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

## 8. Stato della candidatura

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

## 9. Report

```powershell
jobscout report
jobscout report --status applied
jobscout report --status interview --output colloqui.md
```

I report sono istantanee Markdown. Il database SQLite resta la fonte principale.

## 10. Importazione CSV

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

## Problemi comuni

### `jobscout` non viene riconosciuto

Riavvia il terminale oppure esegui il comando dalla cartella del progetto:

```powershell
uv run jobscout --help
```

### Una fonte non restituisce risultati

- Prova una sola query e una sola fonte.
- Riduci `results_per_term`.
- Controlla la località.
- Dopo un errore `429`, attendi prima di riprovare.
- Usa un'altra fonte oppure importa un CSV.

### Google mostra `Job not found`

È un risultato rimasto nell'indice. L'offerta deve restare `closed`; non usare
mirror sospetti per inviare dati personali.

### Dove vengono salvati i dati personali?

Configurazione, database SQLite, report, note e CSV importati restano sul tuo
computer. Il programma effettua però richieste ai job board configurati e agli
URL pubblici dell'offerta o dell'ATS durante ricerca e verifica. Non carica il
CV e non invia candidature. `.gitignore` esclude i dati locali e `data/` dal
repository.
