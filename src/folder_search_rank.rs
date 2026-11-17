//! Deterministic, local ranking over candidates obtained from Everything.
//! This function does not acquire candidates or make completeness claims.
use crate::everything::EverythingFolder;

fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ac) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(previous + usize::from(ac != *bc));
            previous = old;
        }
    }
    row[b.len()]
}

fn score(query: &str, path: &str) -> i64 {
    let parts: Vec<&str> = path
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    let name = parts.last().copied().unwrap_or("");
    let needle = compact(query);
    if needle.is_empty() {
        return 0;
    }
    let norm = compact(name);
    let n = needle.chars().count();
    let m = norm.chars().count();
    let mut value = if norm == needle {
        10000
    } else if norm.starts_with(&needle) {
        8500
    } else if norm.contains(&needle) {
        7500
    } else if n >= 4 && m >= 4 && n.abs_diff(m) <= 2 && distance(&needle, &norm) <= 2 {
        7000 - distance(&needle, &norm) as i64 * 250
    } else if compact(path).contains(&needle) {
        2500
    } else {
        0
    };
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(compact)
        .collect();
    // Match separate query words only at name-token boundaries, in order.
    // Arbitrary fragments within unrelated words are not meaningful matches.
    if words.len() > 1 {
        let name_words: Vec<String> = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(compact)
            .collect();
        let mut next = 0;
        for token in &name_words {
            if next < words.len() && token.starts_with(&words[next]) {
                next += 1;
            }
        }
        if next == words.len() {
            value = value.max(8000);
        }
    }
    let technical = |part: &&str| {
        matches!(
            part.to_ascii_lowercase().as_str(),
            ".git"
                | ".svn"
                | "node_modules"
                | "target"
                | "build"
                | "dist"
                | "cache"
                | ".cache"
                | "__pycache__"
                | "vendor"
        )
    };
    let penalty = parts.iter().filter(|part| technical(part)).count() as i64 * 650;
    // Explicit name matches remain discoverable even under technical parents.
    value - penalty - parts.len() as i64
}

pub(crate) fn rank_folders(query: &str, items: &mut [EverythingFolder]) {
    items.sort_by(|a, b| {
        let a_path = a.path.to_string_lossy();
        let b_path = b.path.to_string_lossy();
        score(query, &b_path)
            .cmp(&score(query, &a_path))
            .then_with(|| a_path.to_lowercase().cmp(&b_path.to_lowercase()))
            .then_with(|| a_path.cmp(&b_path))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    #[test]
    fn names_outrank_technical_descendants_and_near_name_works() {
        let mut items: Vec<_> = [
            r"C:\work\Katran\target\build",
            r"C:\work\Katran",
            r"C:\work\other\cat run notes",
            r"C:\work\Katran\.git",
        ]
        .into_iter()
        .map(|path| EverythingFolder {
            path: PathBuf::from(path),
        })
        .collect();
        rank_folders("Katran", &mut items);
        assert!(items[0].path.to_string_lossy().ends_with("Katran"));
        rank_folders("cat run", &mut items);
        assert!(
            items
                .iter()
                .position(|i| i.path.to_string_lossy().ends_with("Katran"))
                .unwrap()
                < items
                    .iter()
                    .position(|i| i.path.to_string_lossy().ends_with("build"))
                    .unwrap()
        );
        rank_folders(".git", &mut items);
        assert!(items[0].path.to_string_lossy().ends_with(".git"));
    }

    #[test]
    fn separated_words_do_not_promote_fragments_inside_other_words() {
        let mut items: Vec<_> = [
            r"C:\work\Application.Runtime",
            r"C:\work\Katran",
            r"C:\work\cat run notes",
            r"C:\work\cat runner",
        ]
        .into_iter()
        .map(|path| EverythingFolder {
            path: PathBuf::from(path),
        })
        .collect();
        rank_folders("cat run", &mut items);
        let position = |name: &str| {
            items
                .iter()
                .position(|item| item.path.to_string_lossy().ends_with(name))
                .unwrap()
        };
        assert!(position("cat run notes") < position("Katran"));
        assert!(position("cat runner") < position("Katran"));
        assert!(position("Katran") < position("Application.Runtime"));
    }
}
