use clap::{Parser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::ExitCode;
use why_core::*;

#[derive(Parser)]
#[command(
    name = "why",
    version,
    about = "Does this still deserve to exist? (necessity-reachability engine)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show status of every node, plus coverage
    Check {
        /// Path to graph.txt
        graph: PathBuf,
        /// Path to att.txt (optional)
        #[arg(long)]
        alive: Option<PathBuf>,
        /// Emit line-delimited JSON
        #[arg(long)]
        json: bool,
        /// Evaluation date YYYY-MM-DD (default: today; controls until/review-by)
        #[arg(long)]
        as_of: Option<String>,
        /// Ignore expiry fields entirely (offline diff mode)
        #[arg(long)]
        no_clock: bool,
        /// Fail (exit 1) if coverage is below this percentage
        #[arg(long)]
        fail_under: Option<f64>,
        /// Enforce referential integrity: namespaced ids, no duplicate edges,
        /// no orphan attestations, all-of groups with >= 2 members
        #[arg(long)]
        strict: bool,
    },
    /// Show the justification chain for one node
    Why {
        /// Path to graph.txt
        graph: PathBuf,
        /// Node id to explain
        id: String,
        /// Path to att.txt (optional)
        #[arg(long)]
        alive: Option<PathBuf>,
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        no_clock: bool,
    },
    /// Show what dies or goes unknown if this node/root dies
    Impact {
        graph: PathBuf,
        /// Node or root id to simulate killing
        id: String,
        #[arg(long)]
        alive: Option<PathBuf>,
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        no_clock: bool,
        #[arg(long)]
        json: bool,
    },
    /// List nodes with exactly one justifying path (fragile)
    Fragile {
        graph: PathBuf,
        #[arg(long)]
        alive: Option<PathBuf>,
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        no_clock: bool,
        #[arg(long)]
        json: bool,
    },
    /// Update or add a root attestation (mutates att file)
    Att {
        /// Path to att.txt (created if missing)
        att: PathBuf,
        /// Root id
        root: String,
        /// alive or dead
        status: String,
        /// Optional since date
        #[arg(long)]
        since: Option<String>,
        /// Optional author
        #[arg(long)]
        by: Option<String>,
        /// Hard expiry date YYYY-MM-DD: dead after this date, no grace
        #[arg(long)]
        until: Option<String>,
        /// Soft expiry date YYYY-MM-DD: unknown after date + 30d grace
        #[arg(long)]
        review_by: Option<String>,
    },
    /// List current root attestations
    Roots {
        /// Path to att.txt
        att: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        as_of: Option<String>,
        #[arg(long)]
        no_clock: bool,
    },
    /// Extract a graph from a directory (GHA adapter)
    Extract {
        /// Directory to scan (defaults to current dir)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Write graph to this file instead of stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Check {
            graph,
            alive,
            json,
            as_of,
            no_clock,
            fail_under,
            strict,
        } => run_check(
            &graph,
            alive.as_deref(),
            json,
            as_of.as_deref(),
            no_clock,
            fail_under,
            strict,
        ),
        Commands::Why {
            graph,
            id,
            alive,
            as_of,
            no_clock,
        } => run_why(&graph, &id, alive.as_deref(), as_of.as_deref(), no_clock),
        Commands::Impact {
            graph,
            id,
            alive,
            as_of,
            no_clock,
            json,
        } => run_impact(
            &graph,
            &id,
            alive.as_deref(),
            as_of.as_deref(),
            no_clock,
            json,
        ),
        Commands::Fragile {
            graph,
            alive,
            as_of,
            no_clock,
            json,
        } => run_fragile(&graph, alive.as_deref(), as_of.as_deref(), no_clock, json),
        Commands::Att {
            att,
            root,
            status,
            since,
            by,
            until,
            review_by,
        } => run_att(
            &att,
            &root,
            &status,
            since.as_deref(),
            by.as_deref(),
            until.as_deref(),
            review_by.as_deref(),
        )
        .map(|_| ExitCode::SUCCESS),
        Commands::Roots {
            att,
            json,
            as_of,
            no_clock,
        } => run_roots(&att, json, as_of.as_deref(), no_clock).map(|_| ExitCode::SUCCESS),
        Commands::Extract { dir, out } => {
            run_extract(&dir, out.as_deref()).map(|_| ExitCode::SUCCESS)
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

// ------------------------------------------------------------------ helpers

fn load(
    graph_path: &std::path::Path,
    att_path: Option<&std::path::Path>,
) -> Result<(Graph, Attestation), Box<dyn std::error::Error>> {
    let gtext = std::fs::read_to_string(graph_path)
        .map_err(|e| format!("reading graph {}: {e}", graph_path.display()))?;
    let graph = parse_graph(&gtext)?;
    let att = match att_path {
        Some(p) => {
            let atext = std::fs::read_to_string(p)
                .map_err(|e| format!("reading attestation {}: {e}", p.display()))?;
            parse_attestation(&atext)?
        }
        None => Attestation::default(),
    };
    Ok((graph, att))
}

/// Resolve the evaluation date: explicit flag > today. None when --no-clock.
fn resolve_as_of(flag: Option<&str>, no_clock: bool) -> Option<String> {
    if no_clock {
        return None;
    }
    match flag {
        Some(d) => {
            if !is_valid_date(d) {
                // validated here so every verb shares the behavior
                eprintln!("warning: --as-of {d} is not YYYY-MM-DD; expiry fields ignored");
                None
            } else {
                Some(d.to_string())
            }
        }
        None => today(),
    }
}

/// Referential-integrity problems for --strict. Empty = clean.
fn strict_problems(graph: &Graph, att: &Attestation) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: HashSet<(String, String, bool)> = HashSet::new();

    for (child, parents) in &graph.edges {
        if !child.contains(':') {
            problems.push(format!("id not namespaced (expected prefix:id): {child}"));
        }
        for (parent, prov) in parents {
            if !parent.contains(':') {
                problems.push(format!("id not namespaced (expected prefix:id): {parent}"));
            }
            let key = (
                child.clone(),
                parent.clone(),
                matches!(prov, Provenance::Explicit),
            );
            if !seen.insert(key) {
                problems.push(format!("duplicate edge: {child} -> {parent}"));
            }
        }
    }
    for (child, groups) in &graph.conjunctive {
        if !child.contains(':') {
            problems.push(format!("id not namespaced (expected prefix:id): {child}"));
        }
        for group in groups {
            if group.len() < 2 {
                problems.push(format!("all-of group with fewer than 2 members on: {child}"));
            }
            for m in group {
                if !m.contains(':') {
                    problems.push(format!("id not namespaced (expected prefix:id): {m}"));
                }
            }
        }
    }
    for id in att.roots.keys() {
        if !id.contains(':') {
            problems.push(format!("attested id not namespaced: {id}"));
        }
        if !graph.nodes.contains(id) {
            problems.push(format!("attestation for node absent from graph: {id}"));
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

fn effective_as_of(flag: Option<&str>, no_clock: bool) -> Option<String> {
    resolve_as_of(flag, no_clock)
}

// ------------------------------------------------------------------ check

#[allow(clippy::too_many_arguments)]
fn run_check(
    graph_path: &std::path::Path,
    att_path: Option<&std::path::Path>,
    json: bool,
    as_of_flag: Option<&str>,
    no_clock: bool,
    fail_under: Option<f64>,
    strict: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (graph, att) = load(graph_path, att_path)?;

    if strict {
        let problems = strict_problems(&graph, &att);
        if !problems.is_empty() {
            for p in &problems {
                eprintln!("strict: {p}");
            }
            eprintln!("strict mode: {} problem(s) found", problems.len());
            return Ok(ExitCode::from(2));
        }
    }

    let as_of = effective_as_of(as_of_flag, no_clock);
    let eval = graph.evaluate(&att, as_of.as_deref());

    let mut has_dead = false;
    let mut ids: Vec<_> = eval.status.keys().collect();
    ids.sort();

    for id in ids {
        let st = eval.status[id];
        if st == Status::Dead {
            has_dead = true;
        }
        if json {
            println!(r#"{{"id":"{}","status":"{}"}}"#, id, st);
        } else {
            println!("{:<8} {}", st, id);
        }
    }

    let mut cov_fail = false;
    if let Some(cov) = &eval.coverage {
        if json {
            println!(
                r#"{{"coverage":{{"wired":{},"discovered":{},"pct":{}}}}}"#,
                cov.wired,
                cov.discovered,
                (cov.pct() * 10.0).round() / 10.0
            );
        } else {
            println!(
                "coverage: {}/{} discovered artifacts wired ({:.0}%)",
                cov.wired,
                cov.discovered,
                cov.pct()
            );
        }
        if let Some(threshold) = fail_under {
            if cov.pct() < threshold {
                cov_fail = true;
                if !json {
                    println!(
                        "coverage {:.0}% is below the required {:.0}% — unwired is not healthy",
                        cov.pct(),
                        threshold
                    );
                }
            }
        }
    } else if fail_under.is_some() {
        eprintln!(
            "strict: --fail-under needs a `# discovered: N` header in graph.txt (why extract writes one)"
        );
        return Ok(ExitCode::from(2));
    }

    if !json {
        for r in &eval.stale_roots {
            println!("stale:   {r} (review-by expired; unknown until re-attested)");
        }
        for r in &eval.expired_roots {
            println!("expired: {r} (until date passed)");
        }
    }

    if has_dead || cov_fail {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

// ------------------------------------------------------------------ why

fn run_why(
    graph_path: &std::path::Path,
    id: &str,
    att_path: Option<&std::path::Path>,
    as_of_flag: Option<&str>,
    no_clock: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (graph, att) = load(graph_path, att_path)?;
    let as_of = effective_as_of(as_of_flag, no_clock);
    let eval = graph.evaluate(&att, as_of.as_deref());

    let status = eval.status.get(id).copied().unwrap_or(Status::Absent);
    let chain = graph.chain(&eval, id);

    println!("{id}");
    if let Some(groups) = graph.conjunctive.get(id) {
        for group in groups {
            let members = group.join(" AND ");
            println!("  needs all of  {members}");
        }
    }
    if chain.len() == 1 && !graph.conjunctive.contains_key(id) {
        println!("  (no justification chain)");
    } else {
        for i in 0..chain.len() - 1 {
            let parent = &chain[i + 1];
            let root_info = att
                .roots
                .get(parent)
                .map(|r| {
                    let st = match r.effective(as_of.as_deref()) {
                        RootStatus::Alive => {
                            format!("alive, since {}", r.since.as_deref().unwrap_or("?"))
                        }
                        RootStatus::Dead => {
                            if matches!(r.status, Liveness::Alive) {
                                format!("expired (until {})", r.until.clone().unwrap_or_default())
                            } else {
                                format!("dead, since {}", r.since.as_deref().unwrap_or("?"))
                            }
                        }
                        RootStatus::Stale => format!(
                            "stale (review-by {} passed; unknown until re-attested)",
                            r.review_by.clone().unwrap_or_default()
                        ),
                    };
                    format!(" ({st})")
                })
                .unwrap_or_default();
            println!("  because  {parent}{root_info}");
        }
    }
    if let Some(d) = eval.idom.get(id) {
        if d != id {
            println!("  dominated by  {d}");
        }
    }
    println!("status: {status}");
    Ok(ExitCode::SUCCESS)
}

// ------------------------------------------------------------------ impact

fn run_impact(
    graph_path: &std::path::Path,
    id: &str,
    att_path: Option<&std::path::Path>,
    as_of_flag: Option<&str>,
    no_clock: bool,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (graph, mut att) = load(graph_path, att_path)?;
    let as_of = effective_as_of(as_of_flag, no_clock);
    let before = graph.evaluate(&att, as_of.as_deref());

    // Simulate killing the target
    att.roots.insert(
        id.to_string(),
        RootAttestation {
            id: id.to_string(),
            status: Liveness::Dead,
            since: None,
            by: Some("impact-simulation".into()),
            until: None,
            review_by: None,
        },
    );
    let after = graph.evaluate(&att, as_of.as_deref());

    // Impacted = newly dead OR newly unknown (a lost reason is an impact
    // even when the verdict is "investigate" rather than "delete").
    let mut impacted: Vec<(String, Status)> = after
        .status
        .iter()
        .filter(|(nid, st)| {
            if nid.as_str() == id {
                return false;
            }
            let worse = matches!(**st, Status::Dead | Status::Unknown);
            let was_bad = before
                .status
                .get(*nid)
                .map(|s| matches!(s, Status::Dead | Status::Unknown))
                .unwrap_or(true);
            worse && !was_bad
        })
        .map(|(nid, st)| (nid.clone(), *st))
        .collect();

    // Dominator subtree: nodes that must go through this gate while alive.
    for n in dom_subtree(&before.idom, id) {
        if n != id && !impacted.iter().any(|(nid, _)| nid == &n) {
            impacted.push((n.clone(), after.status.get(&n).copied().unwrap_or(Status::Unknown)));
        }
    }

    // Structural chains (covers already-dead targets).
    for nid in before.status.keys() {
        if nid == id {
            continue;
        }
        let chain = graph.chain(&before, nid);
        if chain.iter().any(|n| n == id) && !impacted.iter().any(|(i, _)| i == nid) {
            impacted.push((nid.clone(), after.status.get(nid).copied().unwrap_or(Status::Unknown)));
        }
    }

    impacted.sort();
    impacted.dedup();

    if impacted.is_empty() {
        if !json {
            println!("(no dependent nodes)");
        }
    } else {
        for (nid, st) in &impacted {
            if json {
                println!(r#"{{"id":"{nid}","impacted":true,"status":"{st}"}}"#);
            } else {
                println!("{nid}  ({st})");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

// ------------------------------------------------------------------ fragile

fn run_fragile(
    graph_path: &std::path::Path,
    att_path: Option<&std::path::Path>,
    as_of_flag: Option<&str>,
    no_clock: bool,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (graph, att) = load(graph_path, att_path)?;
    let as_of = effective_as_of(as_of_flag, no_clock);
    let eval = graph.evaluate(&att, as_of.as_deref());

    let alive: HashSet<&str> = eval
        .status
        .iter()
        .filter(|(_, st)| **st == Status::Alive)
        .map(|(id, _)| id.as_str())
        .collect();

    let mut alive_parents: HashMap<&str, usize> = HashMap::new();
    for (child, parents) in &graph.edges {
        if !alive.contains(child.as_str()) {
            continue;
        }
        let count = parents
            .iter()
            .filter(|(p, _)| alive.contains(p.as_str()))
            .count();
        alive_parents.insert(child.as_str(), count);
    }

    let mut fragile: Vec<_> = eval
        .status
        .iter()
        .filter(|(id, st)| {
            if **st != Status::Alive {
                return false;
            }
            let idom = eval.idom.get(*id);
            let dominated = idom.map(|d| d != *id).unwrap_or(false);
            let one_parent = alive_parents.get(id.as_str()).copied().unwrap_or(0) == 1;
            dominated && one_parent
        })
        .map(|(id, _)| id.clone())
        .collect();
    fragile.sort();

    for id in &fragile {
        let gate = eval.idom.get(id).map(|s| s.as_str()).unwrap_or("?");
        if json {
            println!(r#"{{"id":"{id}","fragile":true,"idom":"{gate}"}}"#);
        } else {
            println!("{id}  (idom: {gate})");
        }
    }
    if fragile.is_empty() && !json {
        println!("(no fragile nodes)");
    }
    Ok(ExitCode::SUCCESS)
}

// ------------------------------------------------------------------ att

fn run_att(
    att_path: &std::path::Path,
    root: &str,
    status: &str,
    since: Option<&str>,
    by: Option<&str>,
    until: Option<&str>,
    review_by: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = match status.to_lowercase().as_str() {
        "alive" => Liveness::Alive,
        "dead" => Liveness::Dead,
        other => return Err(format!("status must be alive or dead, got {other}").into()),
    };
    if let Some(d) = until {
        if !is_valid_date(d) {
            return Err(format!("--until {d} is not a YYYY-MM-DD date").into());
        }
    }
    if let Some(d) = review_by {
        if !is_valid_date(d) {
            return Err(format!("--review-by {d} is not a YYYY-MM-DD date").into());
        }
    }

    let mut att = if att_path.exists() {
        let text = std::fs::read_to_string(att_path)?;
        parse_attestation(&text)?
    } else {
        Attestation::default()
    };

    // Preserve existing mortality fields unless explicitly overridden.
    let (prev_until, prev_review) = att
        .roots
        .get(root)
        .map(|r| (r.until.clone(), r.review_by.clone()))
        .unwrap_or((None, None));

    att.roots.insert(
        root.to_string(),
        RootAttestation {
            id: root.to_string(),
            status,
            since: since.map(|s| s.to_string()).or_else(|| {
                att.roots
                    .get(root)
                    .and_then(|r| r.since.clone())
            }),
            by: by
                .map(|s| s.to_string())
                .or_else(|| att.roots.get(root).and_then(|r| r.by.clone())),
            until: until
                .map(|s| s.to_string())
                .or(prev_until),
            review_by: review_by.map(|s| s.to_string()).or(prev_review),
        },
    );

    // Rewrite file stably sorted
    let mut ids: Vec<_> = att.roots.keys().cloned().collect();
    ids.sort();
    let mut out = String::new();
    for id in ids {
        let r = &att.roots[&id];
        let st = match r.status {
            Liveness::Alive => "alive",
            Liveness::Dead => "dead",
        };
        out.push_str(&id);
        out.push(' ');
        out.push_str(st);
        if let Some(s) = &r.since {
            out.push(' ');
            out.push_str(s);
        }
        if let Some(b) = &r.by {
            if r.since.is_none() {
                out.push_str(" -");
            }
            out.push(' ');
            out.push_str(b);
        }
        if let Some(u) = &r.until {
            out.push_str(&format!(" until={u}"));
        }
        if let Some(rb) = &r.review_by {
            out.push_str(&format!(" review-by={rb}"));
        }
        out.push('\n');
    }
    std::fs::write(att_path, out)?;
    println!(
        "updated {root} -> {}",
        match status {
            Liveness::Alive => "alive",
            Liveness::Dead => "dead",
        }
    );
    Ok(())
}

// ------------------------------------------------------------------ roots

fn run_roots(
    att_path: &std::path::Path,
    json: bool,
    as_of_flag: Option<&str>,
    no_clock: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(att_path)
        .map_err(|e| format!("reading {}: {e}", att_path.display()))?;
    let att = parse_attestation(&text)?;
    let as_of = effective_as_of(as_of_flag, no_clock);

    let mut ids: Vec<_> = att.roots.keys().collect();
    ids.sort();
    for id in ids {
        let r = &att.roots[id];
        let st = match r.status {
            Liveness::Alive => "alive",
            Liveness::Dead => "dead",
        };
        let eff = match r.effective(as_of.as_deref()) {
            RootStatus::Alive => "alive".to_string(),
            RootStatus::Dead => {
                if matches!(r.status, Liveness::Alive) {
                    "EXPIRED".to_string()
                } else {
                    "dead".to_string()
                }
            }
            RootStatus::Stale => "STALE".to_string(),
        };
        if json {
            println!(
                r#"{{"id":"{}","status":"{}","effective":"{}","since":{},"by":{},"until":{},"review_by":{}}}"#,
                id,
                st,
                eff,
                r.since.as_ref().map(|s| format!("\"{s}\"")).unwrap_or_else(|| "null".into()),
                r.by.as_ref().map(|s| format!("\"{s}\"")).unwrap_or_else(|| "null".into()),
                r.until.as_ref().map(|s| format!("\"{s}\"")).unwrap_or_else(|| "null".into()),
                r.review_by.as_ref().map(|s| format!("\"{s}\"")).unwrap_or_else(|| "null".into()),
            );
        } else {
            let since = r.since.as_deref().unwrap_or("-");
            let by = r.by.as_deref().unwrap_or("-");
            let until = r.until.as_deref().unwrap_or("-");
            let review = r.review_by.as_deref().unwrap_or("-");
            println!("{id:<32} {st:<6} -> {eff:<8} since:{since:<12} by:{by:<10} until:{until:<12} review-by:{review}");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ extract

/// An anchored ticket id: 2+ uppercase alphanumeric prefix (starting with a
/// letter), a dash, then digits — the classic tracker key shape
/// (MIG-4821, BILL-99, GH-7). Anything else (ubuntu-22, v2, Python-3) is
/// not a ticket and must never become an inferred edge.
fn is_anchored_ticket(tok: &str) -> bool {
    let Some((prefix, num)) = tok.split_once('-') else {
        return false;
    };
    if prefix.len() < 2 || num.is_empty() {
        return false;
    }
    let mut chars = prefix.chars();
    let first = chars.next().unwrap();
    first.is_ascii_uppercase()
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && num.chars().all(|c| c.is_ascii_digit())
}

fn run_extract(
    dir: &std::path::Path,
    out: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let workflows = dir.join(".github/workflows");
    if !workflows.is_dir() {
        return Err(format!("no .github/workflows directory under {}", dir.display()).into());
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&workflows)? {
        let path = entry?.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "yml" || ext == "yaml" {
            files.push(path);
        }
    }
    files.sort();

    let mut edges: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let mut discovered: usize = 0;

    let push_edge = |edges: &mut Vec<String>, seen: &mut HashSet<String>, e: String| {
        if seen.insert(e.clone()) {
            edges.push(e);
        }
    };

    for path in &files {
        let content = std::fs::read_to_string(path)?;
        let workflow_name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut current_job: Option<String> = None;
        let mut pending_needs = false;
        // The top-level `on:` block's keys (push:, pull_request:, ...) sit at
        // indent 2 and must never be mistaken for jobs.
        let mut in_on_block = false;

        for line in content.lines() {
            let trimmed = line.trim();

            let is_top_level = !line.starts_with(' ')
                && !line.starts_with('\t')
                && !trimmed.is_empty()
                && !trimmed.starts_with('#');
            if is_top_level {
                in_on_block = trimmed == "on:" || trimmed.starts_with("on:");
            }

            // job key at indent 2: "  build:"
            if line.starts_with("  ")
                && !line.starts_with("   ")
                && trimmed.ends_with(':')
                && !trimmed.starts_with('#')
                && !in_on_block
            {
                let job = trimmed.trim_end_matches(':');
                if !matches!(
                    job,
                    "on" | "env" | "defaults" | "concurrency" | "permissions" | "jobs"
                ) {
                    current_job = Some(job.to_string());
                    discovered += 1;
                    pending_needs = false;
                }
            }

            let job_id = |job: &Option<String>| {
                job.clone()
                    .map(|j| format!("workflow:{workflow_name}#{j}"))
                    .unwrap_or_else(|| format!("workflow:{workflow_name}"))
            };

            // needs: inline list or block list on following lines
            if let Some(rest) = trimmed.strip_prefix("needs:") {
                let deps = rest.trim();
                if deps.is_empty() {
                    pending_needs = true;
                } else {
                    pending_needs = false;
                    let cleaned = deps.trim_matches(|c| c == '[' || c == ']');
                    for dep in cleaned.split(',') {
                        let dep = dep.trim().trim_matches('"').trim_matches('\'');
                        if !dep.is_empty() {
                            if let Some(j) = &current_job {
                                push_edge(
                                    &mut edges,
                                    &mut seen,
                                    format!("workflow:{workflow_name}#{j} -> workflow:{workflow_name}#{dep} (explicit)"),
                                );
                            }
                        }
                    }
                }
            } else if pending_needs {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    let dep = item.trim().trim_matches('"').trim_matches('\'');
                    if !dep.is_empty() {
                        if let Some(j) = &current_job {
                            push_edge(
                                &mut edges,
                                &mut seen,
                                format!("workflow:{workflow_name}#{j} -> workflow:{workflow_name}#{dep} (explicit)"),
                            );
                        }
                    }
                } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    pending_needs = false;
                }
            }

            // environment: <name> — explicit edge to an environment root
            if let Some(env) = trimmed.strip_prefix("environment:") {
                let env = env.trim().trim_matches('"').trim_matches('\'');
                if !env.is_empty() {
                    if let Some(j) = &current_job {
                        push_edge(
                            &mut edges,
                            &mut seen,
                            format!("workflow:{workflow_name}#{j} -> environment:{env} (explicit)"),
                        );
                    }
                }
            }

            // Inline because comment: "# because: jira:MIG-4821 @ 2023-04-12 by ci"
            if let Some(rest) = trimmed.strip_prefix("# because:") {
                let mut toks = rest.trim().split_whitespace();
                let parent = toks.next().unwrap_or_default().to_string();
                if !parent.is_empty() {
                    let from = job_id(&current_job);
                    push_edge(
                        &mut edges,
                        &mut seen,
                        format!("{from} -> {parent} (explicit)"),
                    );
                }
                continue; // don't ticket-scan the because line
            }

            // Anchored tickets anywhere in the line become candidate edges.
            for tok in trimmed.split_whitespace() {
                if is_anchored_ticket(tok) {
                    let from = job_id(&current_job);
                    push_edge(
                        &mut edges,
                        &mut seen,
                        format!("{from} -> ticket:{tok} (inferred)"),
                    );
                }
            }
        }
    }

    edges.sort();
    edges.dedup();

    let mut output = String::from("# extracted by why extract\n");
    output.push_str(&format!("# discovered: {discovered}\n"));
    if edges.is_empty() {
        output.push_str("# no edges extracted\n");
    } else {
        output.push_str(&edges.join("\n"));
        output.push('\n');
    }

    match out {
        Some(p) => std::fs::write(p, &output)?,
        None => print!("{output}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchored_tickets() {
        assert!(is_anchored_ticket("MIG-4821"));
        assert!(is_anchored_ticket("BILL-99"));
        assert!(is_anchored_ticket("GH-7"));
        assert!(is_anchored_ticket("PROJ-12345"));
        assert!(!is_anchored_ticket("ubuntu-22")); // lowercase prefix
        assert!(!is_anchored_ticket("v2")); // no dash
        assert!(!is_anchored_ticket("Python-3")); // mixed case
        assert!(!is_anchored_ticket("3-11")); // starts with a digit
        assert!(!is_anchored_ticket("A-1")); // 1-char prefix
        assert!(!is_anchored_ticket("node-18")); // lowercase
        assert!(!is_anchored_ticket("2023-04-12")); // all digits
    }

    #[test]
    fn strict_problems_detected() {
        let g = parse_graph("build -> deploy\nbuild -> deploy\nx:y -> all-of: a:b\n").unwrap();
        let a = parse_attestation("orphan:root alive 2024-01-01 ci\n").unwrap();
        let problems = strict_problems(&g, &a);
        assert!(problems.iter().any(|p| p.contains("not namespaced") && p.contains("build")));
        assert!(problems.iter().any(|p| p.contains("duplicate edge")));
        assert!(problems.iter().any(|p| p.contains("fewer than 2 members")));
        assert!(problems.iter().any(|p| p.contains("absent from graph")));
    }

    #[test]
    fn strict_clean_graph_passes() {
        let g = parse_graph("workflow:ci -> project:alpha (explicit)\n").unwrap();
        let a = parse_attestation("project:alpha alive 2024-01-01 casey\n").unwrap();
        assert!(strict_problems(&g, &a).is_empty());
    }
}
