# Guida introduttiva

Questa guida porta OpenJobScout dall'installazione alla prima candidatura
tracciata. OpenJobScout non invia automaticamente candidature.

## 1. Requisiti

- Python 3.11 oppure 3.12
- `uv`
- connessione Internet per ricerca e verifica

Per installare `uv`, usa la
[guida ufficiale](https://docs.astral.sh/uv/getting-started/installation/).

## 2. Installazione

```powershell
git clone https://github.com/cmdr-chara/open-job-scout.git
cd open-job-scout
uv tool install .
jobscout --help
```

Per lavorare sul codice:

```powershell
uv sync --extra dev
uv run jobscout --help
```

## 3. Configurazione locale

```powershell
jobscout init
notepad "$env:USERPROFILE\.openjobscout\config.toml"
```

Percorsi predefiniti:

| Contenuto | Percorso Windows |
| --- | --- |
| Configurazione | `%USERPROFILE%\.openjobscout\config.toml` |
| Database | `%USERPROFILE%\.openjobscout\jobs.sqlite3` |
| Report | `%USERPROFILE%\.openjobscout\reports\` |

`jobscout init` non sovrascrive un file esistente. `--force` va usato soltanto
quando vuoi sostituirlo deliberatamente.

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

Per vedere tutti i dettagli di un risultato:

```powershell
jobscout show ID
```

L'ID breve viene mostrato dal comando `list`.

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

Nel database SQLite e nei report locali configurati. `.gitignore` esclude
database, report, configurazioni private e CV dal repository.
