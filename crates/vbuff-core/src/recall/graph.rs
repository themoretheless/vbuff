use std::collections::{HashMap, HashSet, VecDeque};

use vbuff_types::ClipId;

const MAX_GRAPH_NODES: usize = 10_000;
const MAX_GRAPH_EDGES: usize = 40_000;
const MAX_RELATED_RESULTS: usize = 100;
const MAX_GRAPH_DEPTH: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipRelation {
    CopiedFrom,
    TransformedFrom,
    PastedAfter,
    SyncedFrom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Edge {
    target: ClipId,
    relation: ClipRelation,
    timestamp_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelatedClip {
    pub clip_id: ClipId,
    pub relation: ClipRelation,
    pub distance: u8,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ClipRelationshipGraph {
    edges: HashMap<ClipId, Vec<Edge>>,
    nodes: HashSet<ClipId>,
    edge_count: usize,
}

impl ClipRelationshipGraph {
    pub fn link(
        &mut self,
        from: ClipId,
        to: ClipId,
        relation: ClipRelation,
        timestamp_ms: u64,
    ) -> bool {
        let added_nodes =
            usize::from(!self.nodes.contains(&from)) + usize::from(!self.nodes.contains(&to));
        if from == to
            || self.edge_count >= MAX_GRAPH_EDGES
            || self.nodes.len().saturating_add(added_nodes) > MAX_GRAPH_NODES
        {
            return false;
        }
        let edges = self.edges.entry(from).or_default();
        if edges
            .iter()
            .any(|edge| edge.target == to && edge.relation == relation)
        {
            return false;
        }
        edges.push(Edge {
            target: to,
            relation,
            timestamp_ms,
        });
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.edge_count += 1;
        true
    }

    pub fn related(&self, root: ClipId, maximum_depth: u8, limit: usize) -> Vec<RelatedClip> {
        let maximum_depth = maximum_depth.min(MAX_GRAPH_DEPTH);
        let limit = limit.min(MAX_RELATED_RESULTS);
        if maximum_depth == 0 || limit == 0 {
            return Vec::new();
        }
        let mut queue = VecDeque::from([(root, 0_u8)]);
        let mut visited = HashSet::from([root]);
        let mut output = Vec::new();
        while let Some((source, distance)) = queue.pop_front() {
            if distance >= maximum_depth {
                continue;
            }
            let Some(edges) = self.edges.get(&source) else {
                continue;
            };
            let mut ordered = edges.iter().collect::<Vec<_>>();
            ordered.sort_by(|left, right| {
                right.timestamp_ms.cmp(&left.timestamp_ms).then_with(|| {
                    left.target
                        .to_string_repr()
                        .cmp(&right.target.to_string_repr())
                })
            });
            for edge in ordered {
                if !visited.insert(edge.target) {
                    continue;
                }
                let next_distance = distance.saturating_add(1);
                output.push(RelatedClip {
                    clip_id: edge.target,
                    relation: edge.relation,
                    distance: next_distance,
                    timestamp_ms: edge.timestamp_ms,
                });
                if output.len() == limit {
                    return output;
                }
                queue.push_back((edge.target, next_distance));
            }
        }
        output
    }

    pub const fn edge_count(&self) -> usize {
        self.edge_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationship_graph_is_bounded_deterministic_and_cycle_safe() {
        let a = ClipId::new();
        let b = ClipId::new();
        let c = ClipId::new();
        let mut graph = ClipRelationshipGraph::default();
        assert!(graph.link(a, b, ClipRelation::CopiedFrom, 10));
        assert!(graph.link(b, c, ClipRelation::TransformedFrom, 20));
        assert!(graph.link(c, a, ClipRelation::PastedAfter, 30));
        assert!(!graph.link(a, a, ClipRelation::CopiedFrom, 40));
        assert!(!graph.link(a, b, ClipRelation::CopiedFrom, 50));
        let related = graph.related(a, 4, 10);
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].clip_id, b);
        assert_eq!(related[1].clip_id, c);
        assert_eq!(related[1].distance, 2);
    }
}
