use crate::{is_valid_date, Attestation, Graph, Liveness, Provenance, RootAttestation};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid edge on line {line}: {content}")]
    InvalidEdge { line: usize, content: String },
    #[error("invalid attestation on line {line}: {content}")]
    InvalidAttestation { line: usize, content: String },
}

/// Parse graph.txt format:
///
/// ```text
/// # comment lines start with # (a `# discovered: N` header is meaningful)
/// child-id -> parent-id (explicit|inferred)
/// child-id -> all-of: parent-id parent-id [parent-id ...]
/// ```
///
/// Bare `child -> parent` defaults to `(explicit)`.
pub fn parse_graph(text: &str) -> Result<Graph, ParseError> {
    let mut g = Graph::default();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            // `# discovered: N` declares how many artifacts the extractor saw,
            // so coverage can distinguish "unwired" from "audited".
            let rest = rest.trim();
            if let Some(n) = rest
                .strip_prefix("discovered:")
                .map(|v| v.trim().parse::<usize>().ok())
                .flatten()
            {
                g.discovered = Some(n);
            }
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, "->").map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(ParseError::InvalidEdge {
                line: lineno + 1,
                content: line.to_string(),
            });
        }

        let child = parts[0].to_string();
        if child.is_empty() || child.contains(char::is_whitespace) {
            return Err(ParseError::InvalidEdge {
                line: lineno + 1,
                content: line.to_string(),
            });
        }

        let rest = parts[1];

        // Conjunctive group: `child -> all-of: p1 p2 p3`
        if let Some(members_str) = rest
            .trim()
            .strip_prefix("all-of:")
            .map(str::trim)
        {
            let members: Vec<String> = members_str
                .split_whitespace()
                .map(|m| m.trim_matches(|c| c == '(' || c == ')').to_string())
                .filter(|m| !m.is_empty())
                .collect();
            if members.is_empty() {
                return Err(ParseError::InvalidEdge {
                    line: lineno + 1,
                    content: line.to_string(),
                });
            }
            g.add_group(child, members);
            continue;
        }

        // parent may be followed by (explicit) or (inferred)
        let (parent, provenance) = if let Some(idx) = rest.find('(') {
            let parent = rest[..idx].trim().to_string();
            let prov_str = rest[idx..].to_lowercase();
            let provenance = if prov_str.contains("inferred") {
                Provenance::Inferred
            } else {
                Provenance::Explicit
            };
            (parent, provenance)
        } else {
            (rest.trim().to_string(), Provenance::Explicit)
        };

        if parent.is_empty() || parent.contains(char::is_whitespace) {
            return Err(ParseError::InvalidEdge {
                line: lineno + 1,
                content: line.to_string(),
            });
        }

        g.add_edge(child, parent, provenance);
    }
    Ok(g)
}

/// Parse att.txt format:
///
/// ```text
/// # root-id  alive|dead  [since]  [by]  [until=YYYY-MM-DD]  [review-by=YYYY-MM-DD]
/// jira:MIG-4821   dead   2023-04-12   ci   until=2026-01-01
/// reg:SOC2-CC6    alive  2024-01-01   security   review-by=2026-06-01
/// ```
///
/// `since` and `by` keep their legacy positional slots (bare tokens after the
/// status). The v0.2 mortality fields `until=` and `review-by=` are key=value
/// tokens so they can never be confused with dates or authors.
pub fn parse_attestation(text: &str) -> Result<Attestation, ParseError> {
    let mut att = Attestation::default();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(ParseError::InvalidAttestation {
                line: lineno + 1,
                content: line.to_string(),
            });
        }

        let id = parts[0].to_string();
        let status = match parts[1].to_lowercase().as_str() {
            "alive" => Liveness::Alive,
            "dead" => Liveness::Dead,
            _ => {
                return Err(ParseError::InvalidAttestation {
                    line: lineno + 1,
                    content: line.to_string(),
                })
            }
        };

        // Remaining tokens: key=value fields, plus at most two legacy
        // positional slots (since, then by).
        let mut since: Option<String> = None;
        let mut by: Option<String> = None;
        let mut until: Option<String> = None;
        let mut review_by: Option<String> = None;
        let mut positional_used = 0;

        for tok in &parts[2..] {
            if let Some((key, value)) = tok.split_once('=') {
                match key {
                    "until" => {
                        if !is_valid_date(value) {
                            return Err(ParseError::InvalidAttestation {
                                line: lineno + 1,
                                content: format!("until={value} is not a YYYY-MM-DD date"),
                            });
                        }
                        until = Some(value.to_string());
                    }
                    "review-by" => {
                        if !is_valid_date(value) {
                            return Err(ParseError::InvalidAttestation {
                                line: lineno + 1,
                                content: format!("review-by={value} is not a YYYY-MM-DD date"),
                            });
                        }
                        review_by = Some(value.to_string());
                    }
                    "since" => since = Some(value.to_string()),
                    "by" => by = Some(value.to_string()),
                    _ => {
                        return Err(ParseError::InvalidAttestation {
                            line: lineno + 1,
                            content: format!("unknown field {tok}"),
                        })
                    }
                }
            } else {
                match positional_used {
                    0 => since = Some(tok.to_string()),
                    1 => by = Some(tok.to_string()),
                    _ => {
                        return Err(ParseError::InvalidAttestation {
                            line: lineno + 1,
                            content: line.to_string(),
                        })
                    }
                }
                positional_used += 1;
            }
        }

        att.insert(RootAttestation {
            id,
            status,
            since,
            by,
            until,
            review_by,
        });
    }
    Ok(att)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_graph() {
        let text = r#"
# comment
role:analyst -> project:active (explicit)
workflow:deploy -> project:active (inferred)
"#;
        let g = parse_graph(text).unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert!(g.edges.contains_key("role:analyst"));
        assert!(g.edges.contains_key("workflow:deploy"));
        assert!(g.discovered.is_none());
    }

    #[test]
    fn parse_discovered_header() {
        let text = "# discovered: 12\na:b -> c:d\n";
        let g = parse_graph(text).unwrap();
        assert_eq!(g.discovered, Some(12));
        assert_eq!(g.wired(), 1);
    }

    #[test]
    fn parse_all_of_group() {
        let text = "orderset:icu -> all-of: protocol:sepsis policy:active\n";
        let g = parse_graph(text).unwrap();
        assert_eq!(g.conjunctive["orderset:icu"].len(), 1);
        assert_eq!(g.conjunctive["orderset:icu"][0].len(), 2);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.wired(), 1);
    }

    #[test]
    fn parse_rejects_multiword_parent() {
        let text = "a:b -> c d\n";
        assert!(parse_graph(text).is_err());
    }

    #[test]
    fn parse_attestation_basic() {
        let text = r#"
project:active   alive  2024-01-01  casey
project:old      dead   2023-04-12  ci
"#;
        let a = parse_attestation(text).unwrap();
        assert_eq!(a.roots.len(), 2);
        assert_eq!(a.roots["project:active"].status, Liveness::Alive);
        assert_eq!(a.roots["project:old"].status, Liveness::Dead);
    }

    #[test]
    fn parse_attestation_key_value_fields() {
        let text =
            "reg:SOC2 alive 2024-01-01 security until=2026-01-01 review-by=2025-12-01\n";
        let a = parse_attestation(text).unwrap();
        let r = &a.roots["reg:SOC2"];
        assert_eq!(r.since.as_deref(), Some("2024-01-01"));
        assert_eq!(r.by.as_deref(), Some("security"));
        assert_eq!(r.until.as_deref(), Some("2026-01-01"));
        assert_eq!(r.review_by.as_deref(), Some("2025-12-01"));
    }

    #[test]
    fn parse_attestation_rejects_bad_date() {
        assert!(parse_attestation("r:x alive until=2026-13-99\n").is_err());
        assert!(parse_attestation("r:x alive review-by=soon\n").is_err());
    }

    #[test]
    fn parse_attestation_rejects_unknown_field() {
        assert!(parse_attestation("r:x alive expiry=2026-01-01\n").is_err());
    }
}
