package main

import (
    "database/sql"
    "encoding/json"
    "fmt"
    "os"
    "sort"
    "strings"
    "time"

    "github.com/PuerkitoBio/goquery"
    tea "github.com/charmbracelet/bubbletea"
    "github.com/charmbracelet/lipgloss"
    _ "modernc.org/sqlite"
)

type Job struct {
    ID          string  `json:"id"`
    Title       string  `json:"title"`
    Company     string  `json:"company"`
    Location    string  `json:"location"`
    WorkMode    string  `json:"work_mode"`
    Status      string  `json:"status"`
    Source      string  `json:"source"`
    Score       float64 `json:"score"`
    LastSeenAt  string  `json:"last_seen_at"`
    Description string  `json:"description"`
}

var sinkInt int
var sinkString string

func fixtureRoot() string {
    if value := os.Getenv("BENCH_FIXTURES"); value != "" {
        return value
    }
    return "bench/langdecision"
}

func loadFixture() ([]byte, []byte, []Job) {
    root := fixtureRoot()
    jsonBytes, err := os.ReadFile(root + "/jobs.json")
    if err != nil {
        panic(err)
    }
    htmlBytes, err := os.ReadFile(root + "/jobs.html")
    if err != nil {
        panic(err)
    }
    var jobs []Job
    if err := json.Unmarshal(jsonBytes, &jobs); err != nil {
        panic(err)
    }
    return jsonBytes, htmlBytes, jobs
}

func bench(iterations int, fn func()) float64 {
    fn()
    start := time.Now()
    for i := 0; i < iterations; i++ {
        fn()
    }
    return float64(time.Since(start).Nanoseconds()) / 1e6 / float64(iterations)
}

func benchJSON(data []byte) float64 {
    return bench(8, func() {
        var jobs []Job
        if err := json.Unmarshal(data, &jobs); err != nil {
            panic(err)
        }
        sinkInt = len(jobs)
    })
}

func benchFilter(jobs []Job) float64 {
    queries := []string{"python", "backend", "engineer", "fastapi", "graduate"}
    sequence := 0
    return bench(300, func() {
        query := queries[sequence%len(queries)]
        sequence++
        matches := make([]int, 0, 2048)
        for index := range jobs {
            job := &jobs[index]
            if job.Status != "new" || job.Score < 55 || job.WorkMode == "onsite" {
                continue
            }
            text := strings.ToLower(job.Title + " " + job.Company + " " + job.Description)
            if strings.Contains(text, query) {
                matches = append(matches, index)
            }
        }
        sort.Slice(matches, func(i, j int) bool {
            left := jobs[matches[i]]
            right := jobs[matches[j]]
            if left.Score == right.Score {
                return left.LastSeenAt > right.LastSeenAt
            }
            return left.Score > right.Score
        })
        sinkInt = len(matches)
        if len(matches) > 0 {
            sinkString = jobs[matches[0]].ID
        }
    })
}

func benchHTML(data []byte) float64 {
    source := string(data)
    return bench(24, func() {
        doc, err := goquery.NewDocumentFromReader(strings.NewReader(source))
        if err != nil {
            panic(err)
        }
        total := 0
        doc.Find("article.job").Each(func(_ int, selection *goquery.Selection) {
            total += len(selection.Find("h2.title").Text())
            total += len(selection.Find("span.company").Text())
            total += len(selection.Find("p.description").Text())
        })
        sinkInt = total
    })
}

func benchSQLite() float64 {
    db, err := sql.Open("sqlite", fixtureRoot()+"/jobs.sqlite3")
    if err != nil {
        panic(err)
    }
    defer db.Close()
    if err := db.Ping(); err != nil {
        panic(err)
    }
    return bench(600, func() {
        rows, err := db.Query(`
            SELECT id, title, company, score
            FROM jobs
            WHERE status=? AND work_mode<>? AND score>=? AND description LIKE ?
            ORDER BY score DESC, last_seen_at DESC
            LIMIT 100
        `, "new", "onsite", 55.0, "%python%")
        if err != nil {
            panic(err)
        }
        count := 0
        for rows.Next() {
            var id, title, company string
            var score float64
            if err := rows.Scan(&id, &title, &company, &score); err != nil {
                panic(err)
            }
            count += len(id) + len(title) + len(company) + int(score)
        }
        if err := rows.Err(); err != nil {
            panic(err)
        }
        rows.Close()
        sinkInt = count
    })
}

var (
    titleStyle = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#7D56F4"))
    mutedStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#888888"))
    selectedStyle = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#FFFFFF")).Background(lipgloss.Color("#5A45D6")).Padding(0, 1)
    panelStyle = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).Padding(1, 2)
)

type model struct {
    jobs     []Job
    selected int
}

func (m model) Init() tea.Cmd { return nil }

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
    switch msg.(type) {
    case tea.WindowSizeMsg:
        return m, nil
    }
    return m, nil
}

func (m model) View() string {
    visible := 28
    if visible > len(m.jobs) {
        visible = len(m.jobs)
    }
    var left strings.Builder
    left.WriteString(titleStyle.Render("RECOMMENDED"))
    left.WriteString("\n\n")
    for i := 0; i < visible; i++ {
        job := m.jobs[i]
        line := fmt.Sprintf("%3.0f  %-24s", job.Score, job.Title)
        if i == m.selected {
            left.WriteString(selectedStyle.Render(line))
        } else {
            left.WriteString(line)
        }
        left.WriteByte('\n')
    }
    job := m.jobs[m.selected]
    detail := titleStyle.Render(job.Title) + "\n" +
        job.Company + "\n\n" +
        mutedStyle.Render(job.Location+" · "+job.WorkMode) + "\n\n" +
        fmt.Sprintf("Score        %.0f\nStatus       %s\nSource       %s\n\n", job.Score, job.Status, job.Source) +
        titleStyle.Render("WHY IT MATCHES") + "\n" +
        "✓ Python\n✓ FastAPI\n✓ PostgreSQL\n✓ Junior-friendly\n\n" +
        mutedStyle.Render("↑↓ Navigate   / Search   O Open   N Note")
    return lipgloss.JoinHorizontal(lipgloss.Top,
        panelStyle.Width(48).Height(34).Render(left.String()),
        panelStyle.Width(68).Height(34).Render(detail),
    )
}

func benchTUI(jobs []Job) float64 {
    sample := jobs[:60]
    m := model{jobs: sample}
    return bench(1800, func() {
        m.selected = (m.selected + 1) % 28
        sinkString = m.View()
    })
}

func main() {
    if len(os.Args) > 1 && os.Args[1] == "noop" {
        return
    }
    jsonBytes, htmlBytes, jobs := loadFixture()
    result := map[string]float64{
        "json_ms":   benchJSON(jsonBytes),
        "filter_ms": benchFilter(jobs),
        "html_ms":   benchHTML(htmlBytes),
        "sqlite_ms": benchSQLite(),
        "tui_ms":    benchTUI(jobs),
    }
    output, err := json.Marshal(result)
    if err != nil {
        panic(err)
    }
    fmt.Println(string(output))
}
