//! Causal Graph Diffusion & Personalized PageRank.
//!
//! Provides Random Walk with Restart (RWR) / Personalized PageRank over the
//! Causal Fact Knowledge Graph to solve root-cause attribution:
//!
//! p^{(k+1)} = (1 - c) * W^T * p^{(k)} + c * e_s
//!
//! Resolves queries like: `k "cause of latest.error"` or `k "causal.latest"`
//! by diffusing influence backward through temporal and causal edges.

use crate::db::GraphDb;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalAttribution {
    pub node_id: String,
    pub node_type: String,
    pub label: String,
    pub score: f64,
    pub explanation: String,
}

/// Pluggable trait for graph diffusion & causal inference
pub trait CausalDiffusion: Send + Sync {
    /// Diffuses causal influence outward from `start_node_id`, ranking the top-k most influential antecedent nodes
    fn diffuse(&self, db: &GraphDb, start_node_id: &str, top_k: usize) -> Result<Vec<CausalAttribution>>;
}

pub struct PageRankDiffusion {
    /// Restart probability c (default 0.15)
    pub restart_prob: f64,
    /// Maximum power iterations (default 20)
    pub max_iterations: usize,
    /// Neighborhood expansion depth (default 2 hops)
    pub max_hops: usize,
}

impl Default for PageRankDiffusion {
    fn default() -> Self {
        Self {
            restart_prob: 0.15,
            max_iterations: 20,
            max_hops: 2,
        }
    }
}

impl PageRankDiffusion {
    pub fn new(restart_prob: f64, max_iterations: usize, max_hops: usize) -> Self {
        Self {
            restart_prob,
            max_iterations,
            max_hops,
        }
    }
}

impl CausalDiffusion for PageRankDiffusion {
    fn diffuse(&self, db: &GraphDb, start_node_id: &str, top_k: usize) -> Result<Vec<CausalAttribution>> {
        // 1. Verify start node exists
        let _root_node = match db.get_node(start_node_id)? {
            Some(n) => n,
            None => return Ok(Vec::new()),
        };

        // 2. Discover local subgraph via bidirectional BFS (hops <= max_hops)
        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        visited_nodes.insert(start_node_id.to_string());
        queue.push_back((start_node_id.to_string(), 0));

        // Adjacency list: u -> Vec<(v, weight, relation)>
        let mut adj: HashMap<String, Vec<(String, f64, String)>> = HashMap::new();

        while let Some((curr, hop)) = queue.pop_front() {
            if hop >= self.max_hops {
                continue;
            }

            // Incoming edges (causes leading to curr)
            if let Ok(incoming) = db.get_incoming(&curr, None) {
                for edge in incoming {
                    if let (Some(source_id), Some(rel), Some(w)) = (
                        edge.get("source_id").and_then(|v| v.as_str()),
                        edge.get("relation").and_then(|v| v.as_str()),
                        edge.get("weight").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))),
                    ) {
                        let weight = w.max(1.0);
                        adj.entry(curr.clone())
                            .or_default()
                            .push((source_id.to_string(), weight, rel.to_string()));
                        adj.entry(source_id.to_string())
                            .or_default()
                            .push((curr.clone(), weight, rel.to_string()));

                        if !visited_nodes.contains(source_id) {
                            visited_nodes.insert(source_id.to_string());
                            queue.push_back((source_id.to_string(), hop + 1));
                        }
                    }
                }
            }

            // Outgoing edges
            if let Ok(outgoing) = db.get_related(&curr, None) {
                for edge in outgoing {
                    if let (Some(target_id), Some(rel), Some(w)) = (
                        edge.get("target_id").and_then(|v| v.as_str()),
                        edge.get("relation").and_then(|v| v.as_str()),
                        edge.get("weight").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))),
                    ) {
                        let weight = w.max(1.0);
                        adj.entry(curr.clone())
                            .or_default()
                            .push((target_id.to_string(), weight, rel.to_string()));
                        adj.entry(target_id.to_string())
                            .or_default()
                            .push((curr.clone(), weight, rel.to_string()));

                        if !visited_nodes.contains(target_id) {
                            visited_nodes.insert(target_id.to_string());
                            queue.push_back((target_id.to_string(), hop + 1));
                        }
                    }
                }
            }
        }

        let node_list: Vec<String> = visited_nodes.into_iter().collect();
        if node_list.len() <= 1 {
            return Ok(Vec::new());
        }

        let n = node_list.len();
        let mut node_to_idx: HashMap<String, usize> = HashMap::with_capacity(n);
        for (idx, id) in node_list.iter().enumerate() {
            node_to_idx.insert(id.clone(), idx);
        }

        let start_idx = match node_to_idx.get(start_node_id) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()),
        };

        // 3. Build Column-Normalized Transition Matrix W
        // P[u][v] is probability of jumping from u to v
        let mut p_curr = vec![0.0f64; n];
        p_curr[start_idx] = 1.0;

        let mut transition: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for (u_id, neighbors) in &adj {
            if let Some(&u_idx) = node_to_idx.get(u_id) {
                let total_weight: f64 = neighbors.iter().map(|(_, w, _)| *w).sum();
                if total_weight > 0.0 {
                    for (v_id, w, _) in neighbors {
                        if let Some(&v_idx) = node_to_idx.get(v_id) {
                            transition[u_idx].push((v_idx, *w / total_weight));
                        }
                    }
                }
            }
        }

        // 4. Power Iterations: p^{(k+1)} = (1 - c) * P^T * p^{(k)} + c * e_s
        let c = self.restart_prob;
        for _ in 0..self.max_iterations {
            let mut p_next = vec![0.0f64; n];

            // Distribute flow from each node u
            for u in 0..n {
                let p_u = p_curr[u];
                if p_u <= 0.0 {
                    continue;
                }
                let edges = &transition[u];
                if edges.is_empty() {
                    // Dangling node: flows directly back to restart vector
                    p_next[start_idx] += (1.0 - c) * p_u;
                } else {
                    for &(v, prob) in edges {
                        p_next[v] += (1.0 - c) * p_u * prob;
                    }
                }
            }

            // Add restart teleportation
            p_next[start_idx] += c;

            // Normalize L1
            let sum: f64 = p_next.iter().sum();
            if sum > 0.0 {
                for val in &mut p_next {
                    *val /= sum;
                }
            }

            p_curr = p_next;
        }

        // 5. Gather & rank antecedent nodes (excluding seed itself)
        let mut attributions = Vec::new();
        for (idx, score) in p_curr.iter().enumerate() {
            if idx == start_idx || *score <= 0.001 {
                continue;
            }
            let node_id = &node_list[idx];
            if let Ok(Some(node_data)) = db.get_node(node_id) {
                let node_type = node_data
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let label = node_data
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let explanation = format!(
                    "Causal diffusion rank: {}% attribution to antecedent {}",
                    (score * 100.0).round(),
                    node_type
                );

                attributions.push(CausalAttribution {
                    node_id: node_id.clone(),
                    node_type,
                    label,
                    score: *score,
                    explanation,
                });
            }
        }

        // Sort descending by score
        attributions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        attributions.truncate(top_k);

        Ok(attributions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_pagerank_diffusion_root_cause() {
        let temp = NamedTempFile::new().unwrap();
        let db = GraphDb::new(temp.path()).unwrap();

        // Setup mini causal DAG:
        // Command "cargo build" -> Error "E0559"
        // Directory "my-project" linked to Command and Error
        let cmd_id = "cmd:01234567";
        let err_id = "error:abcdef01";
        let dir_id = "dir:/workspace";

        db.upsert_node(cmd_id, "Command", "cargo build", None).unwrap();
        db.upsert_node(err_id, "Error", "RustCompile", None).unwrap();
        db.upsert_node(dir_id, "Directory", "/workspace", None).unwrap();

        // Edges: Error occurred in Directory, Command executed in Directory
        db.upsert_edge(err_id, dir_id, "OCCURRED_IN", 1, 1.0).unwrap();
        db.upsert_edge(cmd_id, dir_id, "EXECUTED_IN", 1, 1.0).unwrap();
        db.upsert_edge(err_id, cmd_id, "PRECEDED_BY", 5, 1.0).unwrap();

        let model = PageRankDiffusion::default();
        let attributions = model.diffuse(&db, err_id, 5).unwrap();

        assert!(!attributions.is_empty());
        // The preceding command has weight 5, should have high attribution score
        let top = &attributions[0];
        assert_eq!(top.node_id, cmd_id);
        assert!(top.score > 0.10);
    }
}
