mod app;
mod config;
mod diagnostics;
mod discovery;
mod exporting;
mod identity;
mod importing;
mod migration;
mod model;
mod providers;
mod ranking;
mod reporting;
mod safety;
mod storage;
mod theme;
mod ui;
mod verification;
mod workflows;

use std::{io, path::PathBuf, process::Command as ProcessCommand, time::Duration};

use anyhow::{Context, Result, bail};
use app::App;
use clap::{Parser, Subcommand};
use config::{load_config, resolve_database_path, selected_config_path};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use model::{ApplicationStatus, Job};
use ratatui::{Terminal, backend::CrosstermBackend};
use safety::{safe_browser_url, safe_http_url, terminal_text};
use storage::Storage;
use time::{Duration as TimeDuration, OffsetDateTime, format_description};

#[derive(Debug, Parser)]
#[command(
    name = "jobscout",
    version,
    about = "Fast, local-first job discovery and application tracking",
    long_about = None
)]
struct Cli {
    /// Override the SQLite tracker path.
    #[arg(long, global = true)]
    database: Option<PathBuf>,

    /// Read storage settings from a specific config.toml.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Open the interactive terminal application.
    Ui,
    /// Discover jobs directly from configured public ATS career APIs.
    Search {
        #[arg(long, default_value_t = 6)]
        workers: usize,
    },
    /// List tracked jobs for scripts and quick terminal checks.
    List {
        #[arg(long)]
        status: Option<ApplicationStatus>,
        #[arg(short, long)]
        query: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show one tracked job by a unique fingerprint prefix.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Mark a job with a tracker status.
    Mark {
        id: String,
        status: ApplicationStatus,
        #[arg(long)]
        note: Option<String>,
    },
    /// Append a note without changing status ownership.
    Note { id: String, text: String },
    /// Schedule or clear a local follow-up action.
    FollowUp {
        id: String,
        date: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    /// List overdue and upcoming follow-up actions.
    Due {
        #[arg(long, default_value_t = 0)]
        days: u64,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show durable tracker history for one job.
    History {
        id: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Import a JobSpy-compatible CSV through filtering, verification, ranking, and tracking.
    ImportCsv {
        path: PathBuf,
        #[arg(long)]
        no_verify: bool,
        #[arg(long, default_value_t = 6)]
        workers: usize,
    },
    /// Write a Markdown report from the current tracker.
    Report {
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Recompute transparent ranking/filter diagnostics without network access.
    Rerank,
    /// Re-verify tracked links and rerank without changing discovery timestamps.
    Recheck {
        #[arg(long, default_value_t = 6)]
        workers: usize,
    },
    /// Print tracker counts and score summary.
    Stats {
        #[arg(long)]
        json: bool,
    },
    /// Export the tracker in Python-compatible JSON or CSV fields.
    Export {
        output: Option<PathBuf>,
        #[arg(long = "output", value_name = "PATH", conflicts_with = "output")]
        output_path: Option<PathBuf>,
        #[arg(long, default_value = "json")]
        format: String,
        #[arg(long)]
        status: Option<ApplicationStatus>,
    },
    /// Inspect config/database health without mutating tracker data.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Mark automatically-managed jobs stale after N unseen days.
    Stale {
        #[arg(long, default_value_t = 30)]
        days: i64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = selected_config_path(cli.config.as_deref())?;
    let database = resolve_database_path(cli.database.as_deref(), cli.config.as_deref())?;
    if let Some(Commands::Doctor { json }) = cli.command.as_ref() {
        return command_doctor(&config_path, &database, *json);
    }

    let storage = Storage::open(database)?;
    match cli.command.unwrap_or(Commands::Ui) {
        Commands::Ui => run_ui(storage),
        Commands::Search { workers } => {
            discovery::search(&storage, &config_path, workers).map(|_| ())
        }
        Commands::List {
            status,
            query,
            limit,
            json,
        } => command_list(&storage, status, query.as_deref(), limit, json),
        Commands::Show { id, json } => command_show(&storage, &id, json),
        Commands::Mark { id, status, note } => {
            storage.mark_job(&id, status, note.as_deref())?;
            println!("{} → {}", terminal_text(&id), status.label());
            Ok(())
        }
        Commands::Note { id, text } => {
            storage.add_note(&id, &text)?;
            println!("note saved for {}", terminal_text(&id));
            Ok(())
        }
        Commands::FollowUp {
            id,
            date,
            note,
            clear,
        } => command_follow_up(&storage, &id, date.as_deref(), note.as_deref(), clear),
        Commands::Due { days, limit, json } => command_due(&storage, days, limit, json),
        Commands::History { id, limit, json } => command_history(&storage, &id, limit, json),
        Commands::ImportCsv {
            path,
            no_verify,
            workers,
        } => workflows::import_csv(&storage, &config_path, &path, !no_verify, workers).map(|_| ()),
        Commands::Report { output, limit } => {
            let report =
                workflows::report(&storage, output.as_deref(), limit.unwrap_or(usize::MAX))?;
            println!("Report: {}", terminal_text(&report.display().to_string()));
            Ok(())
        }
        Commands::Rerank => command_rerank(&storage, &config_path),
        Commands::Recheck { workers } => command_recheck(&storage, &config_path, workers),
        Commands::Stats { json } => command_stats(&storage, json),
        Commands::Export {
            output,
            output_path,
            format,
            status,
        } => {
            let Some(output) = output_path.or(output) else {
                bail!("export requires an output path (use OUTPUT or --output PATH)");
            };
            command_export(&storage, &output, &format, status)
        }
        Commands::Stale { days } => {
            let changed = storage.mark_stale_jobs(days)?;
            println!("marked {changed} job(s) stale");
            Ok(())
        }
        Commands::Doctor { .. } => unreachable!("doctor is handled before opening storage"),
    }
}

fn command_list(
    storage: &Storage,
    status: Option<ApplicationStatus>,
    query: Option<&str>,
    limit: usize,
    json: bool,
) -> Result<()> {
    if limit == 0 {
        bail!("limit must be at least 1");
    }
    let query = query.map(str::trim).filter(|value| !value.is_empty());
    let jobs = storage
        .load_jobs()?
        .into_iter()
        .filter(|job| status.is_none_or(|status| job.status == status))
        .filter(|job| {
            query.is_none_or(|query| job.search_blob().contains(&query.to_ascii_lowercase()))
        })
        .take(limit)
        .collect::<Vec<_>>();
    if jobs.is_empty() {
        if json {
            println!("[]");
            return Ok(());
        }
        println!("No jobs match the current filters.");
        return Ok(());
    }
    if json {
        let values = jobs.iter().map(exporting::job_json).collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&values)?);
        return Ok(());
    }
    for job in jobs {
        println!(
            "{:<10} {:>5.1}  {:<10}  {} — {}",
            job.short_id(),
            job.score,
            job.status.as_str(),
            terminal_text(&job.title),
            terminal_text(&job.company)
        );
    }
    Ok(())
}

fn command_show(storage: &Storage, id: &str, json: bool) -> Result<()> {
    let job = storage.find_job(id)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&exporting::job_json(&job))?
        );
    } else {
        print_job(&job);
    }
    Ok(())
}

fn action_date(value: &str) -> Result<String> {
    let format = format_description::parse_borrowed::<3>("[year]-[month]-[day]")?;
    Ok(time::Date::parse(value.trim(), &format)?.to_string())
}

fn command_follow_up(
    storage: &Storage,
    id: &str,
    date: Option<&str>,
    note: Option<&str>,
    clear: bool,
) -> Result<()> {
    if clear && date.is_some() {
        bail!("do not provide a date when using --clear");
    }
    if !clear && date.is_none() {
        bail!("provide YYYY-MM-DD or use --clear");
    }
    let normalized = date.map(action_date).transpose()?;
    let changed = storage.set_next_action(
        id,
        normalized.as_deref(),
        note.map(str::trim).filter(|value| !value.is_empty()),
    )?;
    if !changed {
        println!("That follow-up is already recorded; nothing changed.");
    } else if let Some(date) = normalized {
        println!("Scheduled follow-up for {} on {date}.", terminal_text(id));
    } else {
        println!("Cleared follow-up for {}.", terminal_text(id));
    }
    Ok(())
}

fn command_due(storage: &Storage, days: u64, limit: usize, json: bool) -> Result<()> {
    if limit == 0 {
        bail!("limit must be at least 1");
    }
    let days = i64::try_from(days).map_err(|_| anyhow::anyhow!("days is too large"))?;
    let through = (local_now() + TimeDuration::days(days)).date().to_string();
    let jobs = storage
        .due_jobs(&through)?
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    if json {
        let values = jobs.iter().map(exporting::job_json).collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&values)?);
        return Ok(());
    }
    if jobs.is_empty() {
        println!("No follow-up actions are due in this window.");
        return Ok(());
    }
    println!("Follow-up actions due through {through}:");
    for job in jobs {
        let note = job
            .next_action_note
            .as_deref()
            .map(|value| format!(" — {}", terminal_text(value)))
            .unwrap_or_default();
        println!(
            "{}  {:<10}  {} — {}{}",
            job.next_action_at.as_deref().unwrap_or("unknown"),
            job.short_id(),
            terminal_text(&job.title),
            terminal_text(&job.company),
            note,
        );
    }
    Ok(())
}

fn command_history(storage: &Storage, id: &str, limit: usize, json: bool) -> Result<()> {
    let job = storage.find_job(id)?;
    let events = storage.events(id, limit)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&events)?);
        return Ok(());
    }
    println!(
        "{} — {} ({})",
        terminal_text(&job.title),
        terminal_text(&job.company),
        job.short_id()
    );
    if events.is_empty() {
        println!("No history events recorded.");
        return Ok(());
    }
    for event in events {
        let transition = match (event.old_value.as_deref(), event.new_value.as_deref()) {
            (Some(old), Some(new)) => {
                format!(" {} → {}", terminal_text(old), terminal_text(new))
            }
            (_, Some(new)) => format!(" {}", terminal_text(new)),
            _ => String::new(),
        };
        let note = event
            .note
            .as_deref()
            .filter(|note| !note.is_empty())
            .map(|note| format!(" · {}", terminal_text(note)))
            .unwrap_or_default();
        println!(
            "{}  {:<12}{}{}",
            terminal_text(&event.created_at),
            terminal_text(&event.event_type),
            transition,
            note
        );
    }
    Ok(())
}

fn command_rerank(storage: &Storage, config_path: &std::path::Path) -> Result<()> {
    let config = load_config(config_path)?;
    println!(
        "Configured search location: {}",
        terminal_text(&config.search.location)
    );
    let mut jobs = storage.load_jobs()?;
    let mut pass_filters = 0;
    for job in &mut jobs {
        let mut probe = job.clone();
        if ranking::filter_job(&mut probe, &config).allowed {
            pass_filters += 1;
        }
        ranking::rank_job(job, &config);
    }
    let refreshed = storage.refresh_jobs(&jobs)?;
    println!("Reranked: {refreshed}");
    println!("Pass current discovery filters: {pass_filters}/{refreshed}");
    println!("Discovery timestamps were not changed.");
    Ok(())
}

fn command_recheck(storage: &Storage, config_path: &std::path::Path, workers: usize) -> Result<()> {
    if workers == 0 {
        bail!("workers must be at least 1");
    }
    let config = load_config(config_path)?;
    let jobs = storage.load_jobs()?;
    if jobs.is_empty() {
        println!("No tracked jobs to recheck.");
        return Ok(());
    }
    println!("Rechecking {} tracked job(s)…", jobs.len());
    let mut refreshed = verification::verify_jobs(jobs, workers);
    for job in &mut refreshed {
        ranking::rank_job(job, &config);
    }
    let closed = refreshed
        .iter()
        .filter(|job| job.verification == "closed")
        .count();
    let unreachable = refreshed
        .iter()
        .filter(|job| job.verification == "unreachable")
        .count();
    let count = storage.refresh_jobs(&refreshed)?;
    println!("Rechecked: {count}");
    println!("Closed: {closed}");
    println!("Unreachable: {unreachable}");
    println!("Discovery timestamps were not changed.");
    Ok(())
}

fn command_stats(storage: &Storage, json: bool) -> Result<()> {
    let jobs = storage.load_jobs()?;
    if json {
        let total = jobs.len();
        let average = if total == 0 {
            0.0
        } else {
            jobs.iter().map(|job| job.score).sum::<f64>() / total as f64
        };
        let statuses = jobs.iter().fold(
            std::collections::BTreeMap::<String, usize>::new(),
            |mut counts, job| {
                *counts.entry(job.status.as_str().to_string()).or_default() += 1;
                counts
            },
        );
        let verification = jobs.iter().fold(
            std::collections::BTreeMap::<String, usize>::new(),
            |mut counts, job| {
                *counts.entry(job.verification.clone()).or_default() += 1;
                counts
            },
        );
        let work_modes = jobs.iter().fold(
            std::collections::BTreeMap::<String, usize>::new(),
            |mut counts, job| {
                *counts
                    .entry(job.work_mode.as_str().to_string())
                    .or_default() += 1;
                counts
            },
        );
        let sources = jobs.iter().fold(
            std::collections::BTreeMap::<String, usize>::new(),
            |mut counts, job| {
                *counts.entry(job.source.clone()).or_default() += 1;
                counts
            },
        );
        let salary_published = jobs
            .iter()
            .filter(|job| job.salary_min.is_some() || job.salary_max.is_some())
            .count();
        let seen_last_7_days = jobs
            .iter()
            .filter(|job| {
                ranking::age_days(&job.last_seen).is_some_and(|days| (0..=7).contains(&days))
            })
            .count();
        let posted_last_7_days = jobs
            .iter()
            .filter(|job| {
                ranking::age_days(&job.posted).is_some_and(|days| (0..=7).contains(&days))
            })
            .count();
        let (overdue_follow_ups, due_today, due_next_7_days) = follow_up_counts(&jobs)?;
        let recorded_status_transitions = jobs
            .iter()
            .map(|job| storage.events(&job.id, 10_000))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|event| event.event_type == "status")
            .count();
        let top_new = jobs
            .iter()
            .filter(|job| job.status == ApplicationStatus::New)
            .take(5)
            .map(|job| {
                serde_json::json!({
                    "fingerprint": job.id,
                    "title": job.title,
                    "company": job.company,
                    "score": job.score,
                })
            })
            .collect::<Vec<_>>();
        let applied = statuses.get("applied").copied().unwrap_or_default()
            + statuses.get("interview").copied().unwrap_or_default()
            + statuses.get("offer").copied().unwrap_or_default();
        let interviews = statuses.get("interview").copied().unwrap_or_default()
            + statuses.get("offer").copied().unwrap_or_default();
        let offers = statuses.get("offer").copied().unwrap_or_default();
        let reviewed = statuses.get("reviewed").copied().unwrap_or_default()
            + statuses.get("rejected").copied().unwrap_or_default()
            + applied;
        let mut action_items = Vec::new();
        if let Some(count) = statuses.get("new").filter(|count| **count > 0) {
            action_items.push(serde_json::json!({
                "type": "review",
                "count": count,
                "message": format!("Review {count} new job(s)"),
            }));
        }
        let uncertain = verification.get("unverified").copied().unwrap_or_default()
            + verification.get("unreachable").copied().unwrap_or_default();
        if uncertain > 0 {
            action_items.push(serde_json::json!({
                "type": "verify",
                "count": uncertain,
                "message": format!("Recheck {uncertain} job(s) with uncertain verification"),
            }));
        }
        if let Some(count) = statuses.get("stale").filter(|count| **count > 0) {
            action_items.push(serde_json::json!({
                "type": "stale",
                "count": count,
                "message": format!("Refresh {count} stale job(s) or archive them"),
            }));
        }
        if overdue_follow_ups > 0 {
            action_items.push(serde_json::json!({
                "type": "follow_up_overdue",
                "count": overdue_follow_ups,
                "message": format!("Follow up on {overdue_follow_ups} overdue action(s)"),
            }));
        }
        if due_today > 0 {
            action_items.push(serde_json::json!({
                "type": "follow_up_today",
                "count": due_today,
                "message": format!("Complete {due_today} follow-up action(s) today"),
            }));
        }
        let percentage = |numerator: usize, denominator: usize| {
            if denominator == 0 {
                0.0
            } else {
                (numerator as f64 / denominator as f64 * 100.0 * 10.0).round() / 10.0
            }
        };
        let payload = serde_json::json!({
            "summary": {
                "total": total,
                "average_score": (average * 10.0).round() / 10.0,
                "salary_published": salary_published,
                "statuses": statuses.clone(),
                "work_modes": work_modes.clone(),
                "sources": sources.clone(),
                "top_new": top_new,
            },
            "insights": {
                "total": total,
                "average_score": (average * 10.0).round() / 10.0,
                "statuses": statuses,
                "verification": verification,
                "work_modes": work_modes,
                "sources": sources,
                "freshness": {
                    "seen_last_7_days": seen_last_7_days,
                    "posted_last_7_days": posted_last_7_days,
                },
                "follow_ups": {
                    "overdue": overdue_follow_ups,
                    "due_today": due_today,
                    "due_next_7_days": due_next_7_days,
                },
                "funnel": {
                    "reviewed_or_beyond": reviewed,
                    "applied_or_beyond": applied,
                    "interview_or_beyond": interviews,
                    "offers": offers,
                    "review_to_application_pct": percentage(applied, reviewed),
                    "application_to_interview_pct": percentage(interviews, applied),
                    "interview_to_offer_pct": percentage(offers, interviews),
                    "recorded_status_transitions": recorded_status_transitions,
                },
                "action_items": action_items,
            },
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    println!("Tracked: {}", jobs.len());
    if jobs.is_empty() {
        return Ok(());
    }
    for status in ApplicationStatus::ALL {
        let count = jobs.iter().filter(|job| job.status == status).count();
        println!("{:<10} {}", format!("{}:", status.label()), count);
    }
    let average = jobs.iter().map(|job| job.score).sum::<f64>() / jobs.len() as f64;
    let best = jobs.iter().map(|job| job.score).fold(0.0_f64, f64::max);
    let salary_published = jobs
        .iter()
        .filter(|job| job.salary_min.is_some() || job.salary_max.is_some())
        .count();
    println!("Average score: {average:.1}");
    println!("Best score:    {best:.1}");
    println!("Salary published: {salary_published}/{}", jobs.len());
    let seen_last_7_days = jobs
        .iter()
        .filter(|job| ranking::age_days(&job.last_seen).is_some_and(|days| (0..=7).contains(&days)))
        .count();
    let posted_last_7_days = jobs
        .iter()
        .filter(|job| ranking::age_days(&job.posted).is_some_and(|days| (0..=7).contains(&days)))
        .count();
    let (overdue_follow_ups, due_today, due_next_7_days) = follow_up_counts(&jobs)?;
    let count_status =
        |status: ApplicationStatus| jobs.iter().filter(|job| job.status == status).count();
    let applied = count_status(ApplicationStatus::Applied)
        + count_status(ApplicationStatus::Interview)
        + count_status(ApplicationStatus::Offer);
    let interviews =
        count_status(ApplicationStatus::Interview) + count_status(ApplicationStatus::Offer);
    let offers = count_status(ApplicationStatus::Offer);
    let reviewed = count_status(ApplicationStatus::Reviewed)
        + count_status(ApplicationStatus::Rejected)
        + applied;
    let percentage = |numerator: usize, denominator: usize| {
        if denominator == 0 {
            0.0
        } else {
            numerator as f64 / denominator as f64 * 100.0
        }
    };
    println!(
        "Freshness: seen_7d={}, posted_7d={}",
        seen_last_7_days, posted_last_7_days
    );
    println!(
        "Funnel: review→apply={:.1}%, apply→interview={:.1}%, interview→offer={:.1}%",
        percentage(applied, reviewed),
        percentage(interviews, applied),
        percentage(offers, interviews)
    );
    println!(
        "Follow-ups: overdue={}, today={}, next_7d={}",
        overdue_follow_ups, due_today, due_next_7_days
    );
    Ok(())
}

fn follow_up_counts(jobs: &[Job]) -> Result<(usize, usize, usize)> {
    let format = format_description::parse_borrowed::<3>("[year]-[month]-[day]")?;
    let today = local_now().date();
    let next_week = today + TimeDuration::days(7);
    let mut overdue = 0;
    let mut due_today = 0;
    let mut due_next_7_days = 0;
    for job in jobs {
        let Some(value) = job.next_action_at.as_deref() else {
            continue;
        };
        let Ok(action_date) = time::Date::parse(value, &format) else {
            continue;
        };
        if action_date < today {
            overdue += 1;
        } else if action_date == today {
            due_today += 1;
        } else if action_date <= next_week {
            due_next_7_days += 1;
        }
    }
    Ok((overdue, due_today, due_next_7_days))
}

fn local_now() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

fn command_export(
    storage: &Storage,
    output: &std::path::Path,
    format: &str,
    status: Option<ApplicationStatus>,
) -> Result<()> {
    let jobs = storage
        .load_jobs()?
        .into_iter()
        .filter(|job| status.is_none_or(|status| job.status == status))
        .collect::<Vec<_>>();
    exporting::export_jobs(&jobs, output, &format.to_ascii_lowercase())?;
    println!(
        "exported {} job(s) to {}",
        jobs.len(),
        terminal_text(&output.display().to_string())
    );
    Ok(())
}

fn command_doctor(
    config_path: &std::path::Path,
    database_path: &std::path::Path,
    json: bool,
) -> Result<()> {
    let checks = diagnostics::run(config_path, database_path);
    if json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        for check in &checks {
            println!(
                "{:<5} {:<22} {}",
                check.level.to_uppercase(),
                terminal_text(check.check),
                terminal_text(&check.message)
            );
        }
    }
    let failed = checks.iter().any(|check| check.level == "error");
    if failed {
        bail!("one or more diagnostics failed");
    }
    Ok(())
}

fn print_job(job: &Job) {
    println!(
        "{} — {}",
        terminal_text(&job.title),
        terminal_text(&job.company)
    );
    println!("ID:           {}", job.short_id());
    println!("Score:        {:.1}/100", job.score);
    println!("Status:       {}", job.status.as_str());
    println!("Work mode:    {}", job.work_mode.as_str());
    println!("Verification: {}", terminal_text(&job.verification));
    if let Some(source) = job
        .verification_source
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        println!("Verified via: {}", terminal_text(source));
    }
    println!("Location:     {}", fallback(&job.location));
    println!(
        "Employment:   {}",
        terminal_text(job.employment_type.as_deref().unwrap_or("not provided"))
    );
    println!("Salary:       {}", terminal_text(&job.salary_label()));
    println!("Posted:       {}", fallback(&job.posted));
    println!("Source:       {}", fallback(&job.source));
    println!("First seen:   {}", fallback(&job.first_seen));
    println!("Last seen:    {}", fallback(&job.last_seen));
    if let Some(updated) = job
        .status_updated_at
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        println!("Status at:    {}", terminal_text(updated));
    }
    println!("URL:          {}", fallback(job.preferred_url()));
    if let Some(url) = job
        .replacement_url
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let title = job
            .replacement_title
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("Suggested replacement");
        println!("Replacement:  {}", terminal_text(title));
        println!("               {}", terminal_text(url));
    }
    println!();
    println!("Why it ranked:");
    if job.reasons.is_empty() {
        println!("  none recorded");
    } else {
        for reason in &job.reasons {
            println!("  + {}", terminal_text(reason));
        }
    }
    println!();
    println!("Concerns:");
    if job.concerns.is_empty() {
        println!("  none recorded");
    } else {
        for concern in &job.concerns {
            println!("  - {}", terminal_text(concern));
        }
    }
    if !job.notes.trim().is_empty() {
        println!();
        println!("Notes:");
        for line in job.notes.lines() {
            println!("  {}", terminal_text(line));
        }
    }
    println!();
    println!("Description:");
    println!("{}", fallback(&job.description));
}

fn fallback(value: &str) -> String {
    if value.trim().is_empty() {
        "not provided".into()
    } else {
        terminal_text(value)
    }
}

fn run_ui(storage: Storage) -> Result<()> {
    let app = App::from_storage(storage)?;
    let mut terminal = setup_terminal()?;
    let result = run_event_loop(&mut terminal, app);
    let cleanup = restore_terminal(&mut terminal);
    result?;
    cleanup?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                app.handle_key(key);
                if let Some(url) = app.take_open_url() {
                    match open_in_browser(&url) {
                        Ok(()) => app.notice = Some("Opened employer listing".into()),
                        Err(error) => app.notice = Some(format!("Could not open listing: {error}")),
                    }
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                }
                MouseEventKind::ScrollUp => {
                    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
                }
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

fn open_in_browser(url: &str) -> Result<()> {
    let url = safe_browser_url(url).context("refusing to open an unsafe URL")?;
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = ProcessCommand::new("explorer.exe");
        command.arg(&url);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = ProcessCommand::new("open");
        command.arg(&url);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = ProcessCommand::new("xdg-open");
        command.arg(&url);
        command
    };
    command
        .spawn()
        .with_context(|| format!("failed to launch browser for {url}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cli_command_is_ui() {
        let cli = Cli::try_parse_from(["jobscout"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn tracker_commands_parse() {
        let cli = Cli::try_parse_from(["jobscout", "mark", "abc123", "applied"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Mark {
                status: ApplicationStatus::Applied,
                ..
            })
        ));
        let cli = Cli::try_parse_from(["jobscout", "note", "abc123", "follow up"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Note { .. })));
    }

    #[test]
    fn operational_commands_parse() {
        let search = Cli::try_parse_from(["jobscout", "search", "--workers", "4"]).unwrap();
        assert!(matches!(
            search.command,
            Some(Commands::Search { workers: 4 })
        ));
        let stats = Cli::try_parse_from(["jobscout", "stats", "--json"]).unwrap();
        assert!(matches!(
            stats.command,
            Some(Commands::Stats { json: true })
        ));
        let rerank = Cli::try_parse_from(["jobscout", "rerank"]).unwrap();
        assert!(matches!(rerank.command, Some(Commands::Rerank)));
        let recheck = Cli::try_parse_from(["jobscout", "recheck", "--workers", "4"]).unwrap();
        assert!(matches!(
            recheck.command,
            Some(Commands::Recheck { workers: 4 })
        ));
        let export =
            Cli::try_parse_from(["jobscout", "export", "jobs.json", "--status", "new"]).unwrap();
        assert!(matches!(export.command, Some(Commands::Export { .. })));
        let export_flag =
            Cli::try_parse_from(["jobscout", "export", "--output", "jobs.json"]).unwrap();
        assert!(matches!(export_flag.command, Some(Commands::Export { .. })));
        let show = Cli::try_parse_from(["jobscout", "show", "abc123", "--json"]).unwrap();
        assert!(matches!(
            show.command,
            Some(Commands::Show { json: true, .. })
        ));
        let history = Cli::try_parse_from(["jobscout", "history", "abc123", "--json"]).unwrap();
        assert!(matches!(
            history.command,
            Some(Commands::History { json: true, .. })
        ));
        let doctor = Cli::try_parse_from(["jobscout", "doctor", "--json"]).unwrap();
        assert!(matches!(
            doctor.command,
            Some(Commands::Doctor { json: true })
        ));
    }

    #[test]
    fn global_database_override_parses() {
        let cli = Cli::try_parse_from(["jobscout", "--database", "tracker.db", "list"]).unwrap();
        assert_eq!(
            cli.database.as_deref(),
            Some(std::path::Path::new("tracker.db"))
        );
    }
}
