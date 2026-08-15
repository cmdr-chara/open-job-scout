use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};
use rusqlite::{params, Connection};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::json;
use std::{
    env,
    fs,
    hint::black_box,
    path::PathBuf,
    time::Instant,
};

#[derive(Clone, Deserialize)]
struct Job {
    id: String,
    title: String,
    company: String,
    location: String,
    work_mode: String,
    status: String,
    source: String,
    score: f64,
    last_seen_at: String,
    description: String,
}

fn fixture_root() -> PathBuf {
    env::var("BENCH_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("bench/langdecision"))
}

fn bench<F: FnMut()>(iterations: usize, mut f: F) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iterations as f64
}

fn bench_json(data: &[u8]) -> f64 {
    bench(8, || {
        let jobs: Vec<Job> = serde_json::from_slice(data).expect("parse jobs json");
        black_box(jobs.len());
    })
}

fn bench_filter(jobs: &[Job]) -> f64 {
    let queries = ["python", "backend", "engineer", "fastapi", "graduate"];
    let mut sequence = 0usize;
    bench(300, || {
        let query = queries[sequence % queries.len()];
        sequence += 1;
        let mut matches: Vec<usize> = Vec::with_capacity(2048);
        for (index, job) in jobs.iter().enumerate() {
            if job.status != "new" || job.score < 55.0 || job.work_mode == "onsite" {
                continue;
            }
            let text = format!("{} {} {}", job.title, job.company, job.description).to_lowercase();
            if text.contains(query) {
                matches.push(index);
            }
        }
        matches.sort_unstable_by(|left, right| {
            let l = &jobs[*left];
            let r = &jobs[*right];
            r.score
                .total_cmp(&l.score)
                .then_with(|| r.last_seen_at.cmp(&l.last_seen_at))
        });
        if let Some(index) = matches.first() {
            black_box(&jobs[*index].id);
        }
        black_box(matches.len());
    })
}

fn bench_html(data: &[u8]) -> f64 {
    let source = std::str::from_utf8(data).expect("html utf8");
    bench(24, || {
        let document = Html::parse_document(source);
        let article = Selector::parse("article.job").unwrap();
        let title = Selector::parse("h2.title").unwrap();
        let company = Selector::parse("span.company").unwrap();
        let description = Selector::parse("p.description").unwrap();
        let mut total = 0usize;
        for node in document.select(&article) {
            total += node.select(&title).flat_map(|item| item.text()).map(str::len).sum::<usize>();
            total += node
                .select(&company)
                .flat_map(|item| item.text())
                .map(str::len)
                .sum::<usize>();
            total += node
                .select(&description)
                .flat_map(|item| item.text())
                .map(str::len)
                .sum::<usize>();
        }
        black_box(total);
    })
}

fn bench_sqlite() -> f64 {
    let connection = Connection::open(fixture_root().join("jobs.sqlite3")).expect("open sqlite");
    bench(600, || {
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, title, company, score
                FROM jobs
                WHERE status=?1 AND work_mode<>?2 AND score>=?3 AND description LIKE ?4
                ORDER BY score DESC, last_seen_at DESC
                LIMIT 100
                "#,
            )
            .expect("prepare query");
        let mut rows = statement
            .query(params!["new", "onsite", 55.0f64, "%python%"])
            .expect("query sqlite");
        let mut count = 0usize;
        while let Some(row) = rows.next().expect("next sqlite row") {
            let id: String = row.get(0).unwrap();
            let title: String = row.get(1).unwrap();
            let company: String = row.get(2).unwrap();
            let score: f64 = row.get(3).unwrap();
            count += id.len() + title.len() + company.len() + score as usize;
        }
        black_box(count);
    })
}

fn render_screen(jobs: &[Job], selected: usize) -> Buffer {
    let area = Rect::new(0, 0, 120, 38);
    let left = Rect::new(0, 0, 49, 38);
    let right = Rect::new(49, 0, 71, 38);
    let mut buffer = Buffer::empty(area);

    let items: Vec<ListItem<'_>> = jobs
        .iter()
        .take(28)
        .enumerate()
        .map(|(index, job)| {
            let style = if index == selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:>3.0} ", job.score), Style::default().fg(Color::Magenta)),
                Span::styled(format!("{:<25}", job.title), style),
            ]))
        })
        .collect();
    List::new(items)
        .block(Block::default().title(" RECOMMENDED ").borders(Borders::ALL))
        .render(left, &mut buffer);

    let job = &jobs[selected];
    let detail = vec![
        Line::from(Span::styled(
            job.title.clone(),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )),
        Line::from(job.company.clone()),
        Line::from(""),
        Line::from(Span::styled(
            format!("{} · {}", job.location, job.work_mode),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(format!("Score        {:.0}", job.score)),
        Line::from(format!("Status       {}", job.status)),
        Line::from(format!("Source       {}", job.source)),
        Line::from(""),
        Line::from(Span::styled(
            "WHY IT MATCHES",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )),
        Line::from("✓ Python"),
        Line::from("✓ FastAPI"),
        Line::from("✓ PostgreSQL"),
        Line::from("✓ Junior-friendly"),
        Line::from(""),
        Line::from(Span::styled(
            "↑↓ Navigate   / Search   O Open   N Note",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    Paragraph::new(detail)
        .block(Block::default().title(" JOB DETAILS ").borders(Borders::ALL))
        .render(right, &mut buffer);
    buffer
}

fn bench_tui(jobs: &[Job]) -> f64 {
    let sample = &jobs[..60];
    let mut selected = 0usize;
    bench(1800, || {
        selected = (selected + 1) % 28;
        let buffer = render_screen(sample, selected);
        black_box(buffer.content.len());
    })
}

fn main() {
    if env::args().nth(1).as_deref() == Some("noop") {
        return;
    }
    let root = fixture_root();
    let json_bytes = fs::read(root.join("jobs.json")).expect("read jobs json");
    let html_bytes = fs::read(root.join("jobs.html")).expect("read jobs html");
    let jobs: Vec<Job> = serde_json::from_slice(&json_bytes).expect("load jobs");

    let result = json!({
        "json_ms": bench_json(&json_bytes),
        "filter_ms": bench_filter(&jobs),
        "html_ms": bench_html(&html_bytes),
        "sqlite_ms": bench_sqlite(),
        "tui_ms": bench_tui(&jobs),
    });
    println!("{}", serde_json::to_string(&result).unwrap());
}
