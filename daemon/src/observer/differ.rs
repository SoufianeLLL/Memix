use tree_sitter::{Tree, Language};
use crate::observer::parser::{AstParser, AstNodeFeature};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub file: String,
    pub nodes_added: Vec<AstNodeFeature>,
    pub nodes_removed: Vec<AstNodeFeature>,
    pub nodes_modified: Vec<AstNodeFeature>,
}

pub struct AstDiffer;

impl AstDiffer {
    pub fn compute_diff(
        file_path: &str,
        parser: &AstParser,
        old_tree: Option<&(Tree, Language)>,
        new_tree: &(Tree, Language),
        old_source: &[u8],
        new_source: &[u8],
        extension: &str,
    ) -> SemanticDiff {
        // Evaluate true structural features.
        let mut old_features_map = HashMap::new();

        if let Some((ot, lang)) = old_tree {
            let features = parser.extract_features(ot, lang.clone(), old_source, extension);
            for f in features {
                old_features_map.insert(f.name.clone(), f);
            }
        }

        let new_features = parser.extract_features(&new_tree.0, new_tree.1.clone(), new_source, extension);
        
        let mut nodes_added = vec![];
        let mut nodes_removed = vec![];
        let mut nodes_modified = vec![];

        // 1. Check for Additions or Modifications dynamically in O(N).
        for nf in new_features {
            match old_features_map.remove(&nf.name) {
                Some(old_feature) => {
                    // It exists in both. Did the body mutate?
                    if old_feature.body != nf.body {
                        nodes_modified.push(nf);
                    }
                }
                None => {
                    // It didn't exist in the old tree. It's new.
                    nodes_added.push(nf);
                }
            }
        }

        // 2. Anything remaining in the old map was removed in the new AST.
        for (_, rf) in old_features_map {
            nodes_removed.push(rf);
        }

        SemanticDiff {
            file: file_path.to_string(),
            nodes_added,
            nodes_removed,
            nodes_modified,
        }
    }
}
