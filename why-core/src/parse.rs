use crate::{Attestation, Graph, Liveness, Provenance, RootAttestation};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid edge on line {line}: {content}")]
    InvalidEdge { line: usize, content: String },
    #[error("invalid attestation on line {line}: {content}")]
    InvalidAttestation { line: usize, content: String },
}

/// Parse graph.txt format:
///   child -> parent (explicit|inferred)
///   child -> parent
pub fn parse_graph(text: &str) -> Result<Graph, ParseError> {
    let mut g = Graph::default();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split on "->"
        let parts: Vec<&str> = line.splitn(2, "->").map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(ParseError::InvalidEdge {
                line: lineno + 1,
                content: line.to_string(),
            });
        }

        let child = parts[0].to_string();
        if child.is_empty() {
            return Err(ParseError::InvalidEdge {
                line: lineno + 1,
                content: line.to_string(),
            });
        }

        // parent may be followed by (explicit) or (inferred)
        let rest = parts[1];
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

        if parent.is_empty() {
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
///   root-id  alive|dead  [since]  [by]
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
        let since = parts.get(2).map(|s| s.to_string());
        let by = parts.get(3).map(|s| s.to_string());

        att.insert(RootAttestation {
            id,
            status,
            since,
            by,
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
}
