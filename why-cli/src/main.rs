use clap::{Parser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::ExitCode;
use why_core::*;

#[derive(Parser)]
#[command(name = "why", version, about = "Does this still deserve to exist?")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show status of every node
    Check {
        /// Path to graph.txt
        graph: PathBuf,
        /// Path to att.txt (optional)
        #[arg(long)]
        alive: Option<PathBuf>,
        /// Emit line-delimited JSON
        #[arg(long)]
        json: bool,
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
    },
    /// Show what becomes dead if this node/root dies
    Impact {
        graph: PathBuf,
        /// Node or root id to simulate killing
        id: String,
        #[arg(long)]
        alive: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// List nodes with exactly one justifying path (fragile)
    Fragile {
        graph: PathBuf,
        #[arg(long)]
        alive: Option<PathBuf>,
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
    },
    /// List current root attestations
    Roots {
        /// Path to att.txt
        att: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Extract a graph from a directory (experimental GHA support)
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
        Commands::Check { graph, alive, json } => {
            run_check(&graph, alive.as_deref(), json).map(|has_dead| {
                if has_dead {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            })
        }
        Commands::Why { graph, id, alive } => {
            run_why(&graph, &id, alive.as_deref()).map(|_| ExitCode::SUCCESS)
        }
        Commands::Impact {
            graph,
            id,
            alive,
            json,
        } => run_impact(&graph, &id, alive.as_deref(), json).map(|_| ExitCode::SUCCESS),
        Commands::Fragile { graph, alive, json } => {
            run_fragile(&graph, alive.as_deref(), json).map(|_| ExitCode::SUCCESS)
        }
        Commands::Att {
            att,
            root,
            status,
            since,
            by,
        } => run_att(&att, &root, &status, since.as_deref(), by.as_deref())
            .map(|_| ExitCode::SUCCESS),
        Commands::Roots { att, json } => {
            run_roots(&att, json).map(|_| ExitCode::SUCCESS)
        }
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

fn run_check(
    graph_path: &std::path::Path,
    att_path: Option<&std::path::Path>,
    json: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (graph, att) = load(graph_path, att_path)?;
    let eval = graph.evaluate(&att);

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
    Ok(has_dead)
}

fn run_why(
    graph_path: &std::path::Path,
    id: &str,
    att_path: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (graph, att) = load(graph_path, att_path)?;
    let eval = graph.evaluate(&att);

    let status = eval.status.get(id).copied().unwrap_or(Status::Absent);
    let chain = graph.chain(&eval, id);

    println!("{id}");
    if chain.len() == 1 {
        println!("  (no justification chain)");
    } else {
        for i in 0..chain.len() - 1 {
            let parent = &chain[i + 1];
            let root_info = att
                .roots
                .get(parent)
                .map(|r| {
                    let st = match r.status {
                        Liveness::Alive => "alive",
                        Liveness::Dead => "dead",
                    };
                    let since = r.since.as_deref().unwrap_or("?");
                    format!(" ({st}, since {since})")
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
    Ok(())
}

fn run_impact(
    graph_path: &std::path::Path,
    id: &str,
    att_path: Option<&std::path::Path>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (graph, mut att) = load(graph_path, att_path)?;
    let before = graph.evaluate(&att);

    // Simulate killing the target
    att.roots.insert(
        id.to_string(),
        RootAttestation {
            id: id.to_string(),
            status: Liveness::Dead,
            since: None,
            by: Some("impact-simulation".into()),
        },
    );
    let after = graph.evaluate(&att);

    // Nodes that were not Dead before but are Dead after
    let mut impacted: Vec<_> = after
        .status
        .iter()
        .filter(|(nid, st)| {
            if *nid == id {
                return false; // skip the target itself
            }
            **st == Status::Dead
                && before
                    .status
                    .get(*nid)
                    .map(|s| *s != Status::Dead)
                    .unwrap_or(true)
        })
        .map(|(nid, _)| nid.clone())
        .collect();

    // Dominator subtree: nodes that must go through this gate while alive.
    for n in why_core::dom_subtree(&before.idom, id) {
        if n != id && !impacted.contains(&n) {
            impacted.push(n);
        }
    }

    // Structural chains (covers already-dead targets)
    for nid in before.status.keys() {
        if nid == id {
            continue;
        }
        let chain = graph.chain(&before, nid);
        if chain.iter().any(|n| n == id) && !impacted.contains(nid) {
            impacted.push(nid.clone());
        }
    }

    impacted.sort();
    impacted.dedup();

    if impacted.is_empty() {
        if !json {
            println!("(no dependent nodes)");
        }
    } else {
        for nid in &impacted {
            if json {
                println!(r#"{{"id":"{nid}","impacted":true}}"#);
            } else {
                println!("{nid}");
            }
        }
    }
    Ok(())
}

fn run_fragile(
    graph_path: &std::path::Path,
    att_path: Option<&std::path::Path>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (graph, att) = load(graph_path, att_path)?;
    let eval = graph.evaluate(&att);

    // Fragile: Alive, not a dominator-tree root, and exactly one Alive
    // parent in the reverse (liveness-flow) graph. idom makes the gate explicit.
    let alive: HashSet<&str> = eval
        .status
        .iter()
        .filter(|(_, st)| **st == Status::Alive)
        .map(|(id, _)| id.as_str())
        .collect();

    // reverse: parent → children restricted to alive
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
            // must be dominated by something else
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
    Ok(())
}

fn run_extract(
    dir: &std::path::Path,
    out: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let workflows = dir.join(".github/workflows");
    if !workflows.is_dir() {
        return Err(format!("no .github/workflows directory under {}", dir.display()).into());
    }

    let mut edges: Vec<String> = Vec::new();
    let mut seen = HashSet::new();

    for entry in std::fs::read_dir(&workflows)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yml" && ext != "yaml" {
            continue;
        }

        let content = std::fs::read_to_string(&path)?;
        let workflow_name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Track jobs we see
        let mut current_job: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            // job key at indent 2: "  build:"
            if line.starts_with("  ")
                && !line.starts_with("   ")
                && trimmed.ends_with(':')
                && !trimmed.starts_with('#')
            {
                let job = trimmed.trim_end_matches(':').to_string();
                if !matches!(
                    job.as_str(),
                    "on" | "env" | "defaults" | "concurrency" | "permissions" | "jobs"
                ) {
                    current_job = Some(job);
                }
            }

            if trimmed.starts_with("needs:") {
                let deps = trimmed.trim_start_matches("needs:").trim();
                let cleaned = deps.trim_matches(|c| c == '[' || c == ']');
                let job_name = current_job.clone().unwrap_or_else(|| "job".into());
                for dep in cleaned.split(',') {
                    let dep = dep.trim().trim_matches('"').trim_matches('\'');
                    if !dep.is_empty() {
                        let edge = format!(
                            "workflow:{workflow_name}#{job_name} -> workflow:{workflow_name}#{dep} (explicit)"
                        );
                        if seen.insert(edge.clone()) {
                            edges.push(edge);
                        }
                    }
                }
            }

            if trimmed.starts_with("environment:") {
                let env = trimmed
                    .trim_start_matches("environment:")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !env.is_empty() {
                    let job_name = current_job.clone().unwrap_or_else(|| "job".into());
                    let edge = format!(
                        "workflow:{workflow_name}#{job_name} -> environment:{env} (explicit)"
                    );
                    if seen.insert(edge.clone()) {
                        edges.push(edge);
                    }
                }
            }

            for ticket in extract_ticket_like(trimmed) {
                let edge = format!("workflow:{workflow_name} -> ticket:{ticket} (inferred)");
                if seen.insert(edge.clone()) {
                    edges.push(edge);
                }
            }
        }
    }

    let output = if edges.is_empty() {
        "# no edges extracted\n".to_string()
    } else {
        let mut out = String::from("# extracted by why extract\n");
        out.push_str(&edges.join("\n"));
        out.push('\n');
        out
    };

    match out {
        Some(p) => std::fs::write(p, &output)?,
        None => print!("{output}"),
    }
    Ok(())
}


fn run_att(
    att_path: &std::path::Path,
    root: &str,
    status: &str,
    since: Option<&str>,
    by: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = match status.to_lowercase().as_str() {
        "alive" => Liveness::Alive,
        "dead" => Liveness::Dead,
        other => return Err(format!("status must be alive or dead, got {other}").into()),
    };

    let mut att = if att_path.exists() {
        let text = std::fs::read_to_string(att_path)?;
        parse_attestation(&text)?
    } else {
        Attestation::default()
    };

    att.roots.insert(
        root.to_string(),
        RootAttestation {
            id: root.to_string(),
            status,
            since: since.map(|s| s.to_string()),
            by: by.map(|s| s.to_string()),
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
            out.push_str(&s);
        }
        if let Some(b) = &r.by {
            // ensure since placeholder if by present but since missing
            if r.since.is_none() {
                out.push_str(" -");
            }
            out.push(' ');
            out.push_str(&b);
        }
        out.push('\n');
    }
    std::fs::write(att_path, out)?;
    println!("updated {} -> {}", root, match status { Liveness::Alive => "alive", Liveness::Dead => "dead" });
    Ok(())
}

fn run_roots(
    att_path: &std::path::Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(att_path)
        .map_err(|e| format!("reading {}: {e}", att_path.display()))?;
    let att = parse_attestation(&text)?;
    let mut ids: Vec<_> = att.roots.keys().collect();
    ids.sort();
    for id in ids {
        let r = &att.roots[id];
        let st = match r.status {
            Liveness::Alive => "alive",
            Liveness::Dead => "dead",
        };
        if json {
            println!(
                r#"{{"id":"{}","status":"{}","since":{},"by":{}}}"#,
                id,
                st,
                r.since.as_ref().map(|s| format!("\"{s}\"")).unwrap_or_else(|| "null".into()),
                r.by.as_ref().map(|s| format!("\"{s}\"")).unwrap_or_else(|| "null".into()),
            );
        } else {
            let since = r.since.as_deref().unwrap_or("-");
            let by = r.by.as_deref().unwrap_or("-");
            println!("{id:<32} {st:<6} {since:<12} {by}");
        }
    }
    Ok(())
}

fn extract_ticket_like(line: &str) -> Vec<String> {
    let mut tickets = Vec::new();
    for word in line.split_whitespace() {
        let clean: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if clean.contains('-') && clean.chars().any(|c| c.is_ascii_digit()) && clean.len() > 3 {
            tickets.push(clean);
        }
    }
    tickets
}
