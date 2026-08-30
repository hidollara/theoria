use std::ops::Range;

use crate::text::unicode::{PairedBracket, grapheme_indices, paired_bracket, script_extensions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScriptSegment {
    pub(super) range: Range<usize>,
    pub(super) script: Option<[u8; 4]>,
}

pub(super) fn script_segments(text: &str) -> Vec<ScriptSegment> {
    let graphemes: Vec<_> = grapheme_indices(text)
        .into_iter()
        .map(|(start, grapheme)| ScriptCluster {
            start,
            end: start + grapheme.len(),
            scripts: grapheme_scripts(grapheme),
            character: grapheme.chars().next(),
            bracket: grapheme.chars().next().and_then(paired_bracket),
        })
        .collect();
    let scripts = resolve_scripts(&graphemes);
    let mut segments: Vec<ScriptSegment> = Vec::new();
    for (cluster, script) in graphemes.into_iter().zip(scripts) {
        if let Some(segment) = segments.last_mut()
            && segment.script == script
        {
            segment.range.end = cluster.end;
        } else {
            segments.push(ScriptSegment {
                range: cluster.start..cluster.end,
                script,
            });
        }
    }
    segments
}

#[derive(Clone, Debug)]
struct ScriptCluster {
    start: usize,
    end: usize,
    scripts: Vec<[u8; 4]>,
    character: Option<char>,
    bracket: Option<PairedBracket>,
}

fn grapheme_scripts(grapheme: &str) -> Vec<[u8; 4]> {
    let mut candidates: Option<Vec<[u8; 4]>> = None;
    for character in grapheme.chars() {
        let extensions = script_extensions(character);
        if extensions.is_empty() {
            continue;
        }
        candidates = Some(match candidates {
            None => extensions,
            Some(current) => {
                let intersection: Vec<_> = current
                    .iter()
                    .copied()
                    .filter(|script| extensions.contains(script))
                    .collect();
                if intersection.is_empty() {
                    current
                } else {
                    intersection
                }
            }
        });
    }
    candidates.unwrap_or_default()
}

fn resolve_scripts(clusters: &[ScriptCluster]) -> Vec<Option<[u8; 4]>> {
    let mut resolved = vec![None; clusters.len()];
    let mut previous = None;
    for (index, cluster) in clusters.iter().enumerate() {
        if cluster.scripts.is_empty() {
            continue;
        }
        let script = previous
            .filter(|script| cluster.scripts.contains(script))
            .or_else(|| following_compatible_script(clusters, index, &cluster.scripts))
            .or_else(|| cluster.scripts.first().copied());
        resolved[index] = script;
        previous = script;
    }

    let mut previous = None;
    for script in &mut resolved {
        if script.is_none() {
            *script = previous;
        } else {
            previous = *script;
        }
    }
    let mut following = None;
    for index in (0..clusters.len()).rev() {
        if resolved[index].is_some() {
            following = resolved[index];
        } else {
            resolved[index] = following;
        }
    }

    resolve_paired_brackets(clusters, &mut resolved);
    resolved
}

fn following_compatible_script(
    clusters: &[ScriptCluster],
    index: usize,
    candidates: &[[u8; 4]],
) -> Option<[u8; 4]> {
    clusters[index + 1..].iter().find_map(|cluster| {
        cluster
            .scripts
            .iter()
            .copied()
            .find(|script| candidates.contains(script))
    })
}

fn resolve_paired_brackets(clusters: &[ScriptCluster], resolved: &mut [Option<[u8; 4]>]) {
    let mut stack: Vec<(char, usize)> = Vec::new();
    for (index, cluster) in clusters.iter().enumerate() {
        match cluster.bracket {
            Some(PairedBracket::Open(close)) => stack.push((close, index)),
            Some(PairedBracket::Close(_)) => {
                let Some(position) = stack
                    .iter()
                    .rposition(|(close, _)| Some(*close) == cluster.character)
                else {
                    continue;
                };
                let (_, open_index) = stack.remove(position);
                let outer = resolved[open_index];
                resolved[index] = outer;
            }
            None => {}
        }
    }
}
