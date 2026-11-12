pub fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}
pub fn in_folder(path: &str, folder: &str) -> bool {
    if folder.is_empty() {
        return true;
    }
    let path = normalize(path);
    path.rsplit_once('/')
        .map(|(p, _)| p == normalize(folder))
        .unwrap_or(false)
}
pub fn browse(indices: &[usize], selected: usize, step: isize) -> Option<usize> {
    if indices.is_empty() {
        return None;
    }
    let pos = indices.iter().position(|i| *i == selected).unwrap_or(0);
    Some(indices[(pos as isize + step).clamp(0, indices.len() as isize - 1) as usize])
}
/// Move spatially through the fixed 4x4 sampler, including empty assignment slots.
pub fn pad_step(selected: usize, dx: isize, dy: isize) -> usize {
    let selected = selected.min(15);
    let col = (selected % 4) as isize;
    let row = (selected / 4) as isize;
    ((row + dy).clamp(0, 3) * 4 + (col + dx).clamp(0, 3)) as usize
}
pub fn folders(paths: &[String], roots: &[String]) -> Vec<String> {
    let mut bases = roots.to_vec();
    if bases.is_empty() && !paths.is_empty() {
        let mut parts: Vec<String> = paths[0]
            .replace('\\', "/")
            .split('/')
            .map(String::from)
            .collect();
        parts.pop();
        for path in paths {
            let path = path.replace('\\', "/");
            let mut other: Vec<&str> = path.split('/').collect();
            other.pop();
            while !parts.is_empty()
                && !parts
                    .iter()
                    .enumerate()
                    .all(|(i, p)| other.get(i).is_some_and(|v| v.eq_ignore_ascii_case(p)))
            {
                parts.pop();
            }
        }
        if !parts.is_empty() {
            bases.push(parts.join("/"));
        }
    }
    let roots = &bases;
    let mut out = std::collections::BTreeMap::new();
    out.insert(String::new(), String::new());
    for path in paths {
        let normalized = path.replace('\\', "/");
        if let Some((parent, _)) = normalized.rsplit_once('/') {
            let root = roots.iter().find(|r| {
                normalize(parent).starts_with(&(normalize(r) + "/"))
                    || normalize(parent) == normalize(r)
            });
            let minimum = root.map(|r| normalize(r).len()).unwrap_or(parent.len());
            let mut current = parent;
            loop {
                out.entry(normalize(current))
                    .or_insert_with(|| current.to_string());
                if current.len() <= minimum {
                    break;
                }
                if let Some((p, _)) = current.rsplit_once('/') {
                    current = p;
                } else {
                    break;
                }
            }
        }
    }
    for root in roots {
        out.entry(normalize(root))
            .or_insert_with(|| root.replace('\\', "/"));
    }
    out.into_values().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampler_moves_by_rows_and_columns_without_wrapping() {
        assert_eq!(pad_step(1, 0, 1), 5);
        assert_eq!(pad_step(5, 1, 0), 6);
        assert_eq!(pad_step(6, -1, 0), 5);
        assert_eq!(pad_step(5, 0, -1), 1);
        assert_eq!(pad_step(15, 1, 1), 15);
        assert_eq!(pad_step(0, -1, -1), 0);
    }
    #[test]
    fn folder_navigation_includes_ancestors_and_excludes_siblings() {
        let paths = vec![
            r"C:\Music\House\Deep\a.wav".into(),
            r"C:\Music\Jazz\b.wav".into(),
        ];
        let folders = folders(&paths, &[r"C:\Music".into()]);
        assert_eq!(
            folders,
            vec![
                "",
                "C:/Music",
                "C:/Music/House",
                "C:/Music/House/Deep",
                "C:/Music/Jazz"
            ]
        );
    }
    #[test]
    fn windows_folders_are_exact() {
        assert!(in_folder(r"C:\Music\House\Song.mp3", "c:/music/house/"));
        assert!(!in_folder(
            r"C:\Music\House\Deep\Song.mp3",
            "c:/music/house"
        ));
        assert!(!in_folder(r"C:\Music\House2\Song.mp3", "c:/music/house"));
    }
    #[test]
    fn skips_unavailable_rows() {
        assert_eq!(browse(&[1, 5, 9], 5, 1), Some(9));
        assert_eq!(browse(&[1, 5, 9], 5, -1), Some(1));
        assert_eq!(browse(&[], 0, 1), None);
    }
}
