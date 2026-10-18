//! The saved city list on disk. It is nothing but an ordered set of
//! geonameids, so plain text — one id per line — says everything a format
//! would, and stays readable and hand-editable. Anything the file cannot
//! offer, from a missing config dir to a garbled line, degrades to fewer
//! cities rather than an error: a clock is not worth refusing to start over.
//! The one thing it must not do is mistake a file it could not read for an
//! empty one and write that back over it.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The comment the file opens with, so a curious reader knows what it holds.
const HEADER: &str = "# elsewhere saved cities, one geonameid per line, in list order";

/// Where the list lives, given the environment. Takes its variables as
/// arguments so it can be tested without touching the process environment.
/// `None` means there is nowhere sensible to write, and persistence is
/// silently skipped.
fn dir(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    // An empty variable counts as unset, per the XDG spec.
    let xdg = xdg.filter(|path| !path.is_empty());
    if let Some(xdg) = xdg {
        return Some(PathBuf::from(xdg).join("elsewhere"));
    }
    let home = home.filter(|path| !path.is_empty())?;
    Some(PathBuf::from(home).join(".config").join("elsewhere"))
}

fn path() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        // Windows has no XDG and seldom a HOME; per-user settings go in the
        // roaming app data folder.
        let appdata = env::var("APPDATA").ok().filter(|path| !path.is_empty());
        appdata.map(|appdata| PathBuf::from(appdata).join("elsewhere"))
    } else {
        let xdg = env::var("XDG_CONFIG_HOME").ok();
        let home = env::var("HOME").ok();
        dir(xdg.as_deref(), home.as_deref())
    };
    dir.map(|dir| dir.join("saved"))
}

/// The geonameids of `text`, in order. Blank lines, comments and anything
/// unparseable are dropped, as are repeats of an id already seen.
fn parse(text: &str) -> Vec<u32> {
    let mut saved: Vec<u32> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Ok(geonameid) = line.parse()
            && !saved.contains(&geonameid)
        {
            saved.push(geonameid);
        }
    }
    saved
}

fn render(saved: &[u32]) -> String {
    let mut out = String::from(HEADER);
    for geonameid in saved {
        out.push('\n');
        out.push_str(&geonameid.to_string());
    }
    out.push('\n');
    out
}

/// The list in the file at `path`. Only a file that is not there counts as
/// empty; one that is there but cannot be read, whether for permissions or for
/// not being text, is reported and comes back `None`.
fn read(path: &Path) -> Option<Vec<u32>> {
    match fs::read_to_string(path) {
        Ok(text) => Some(parse(&text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(Vec::new()),
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            None
        }
    }
}

/// The saved list, or `None` when the file could not be read. Then the session
/// must not save: it would replace whatever the file holds with its own list.
pub fn load() -> Option<Vec<u32>> {
    match path() {
        Some(path) => read(&path),
        None => Some(Vec::new()),
    }
}

/// Rewrites the whole list, through a temporary file renamed into place so an
/// interrupted write cannot leave half a list behind. A failure is reported
/// and shrugged off: losing the order across restarts must never break the
/// running session.
pub fn save(saved: &[u32]) {
    let Some(path) = path() else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    let temp = path.with_extension("tmp");
    if let Err(e) = fs::create_dir_all(dir)
        .and_then(|()| fs::write(&temp, render(saved)))
        .and_then(|()| fs::rename(&temp, &path))
    {
        eprintln!("{}: {e}", path.display());
    }
}

/// Moves `dragged` to sit just before `before`, or to the end when there is no
/// row to sit before. Returns whether the order actually changed, so a drop
/// that lands where the row already was costs nothing. An id the list does not
/// hold, or a row dropped on itself, leaves it untouched.
pub fn reorder(saved: &mut Vec<u32>, dragged: u32, before: Option<u32>) -> bool {
    if before == Some(dragged) {
        return false;
    }
    let Some(from) = saved.iter().position(|id| *id == dragged) else {
        return false;
    };
    let mut reordered = saved.clone();
    reordered.remove(from);
    let to = match before {
        Some(before) => match reordered.iter().position(|id| *id == before) {
            Some(at) => at,
            None => return false,
        },
        None => reordered.len(),
    };
    reordered.insert(to, dragged);

    if reordered == *saved {
        return false;
    }
    *saved = reordered;
    true
}

/// The given cities, westmost first. The sort is stable, so cities sharing a
/// longitude keep the order they came in; a longitude that is somehow not a
/// number sorts as equal rather than panicking.
pub fn westward(mut cities: Vec<(u32, f64)>) -> Vec<u32> {
    cities.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    cities.into_iter().map(|(geonameid, _)| geonameid).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_geonameid_per_line() {
        assert_eq!(
            parse("1850147\n2643743\n5128581\n"),
            [1850147, 2643743, 5128581]
        );
    }

    #[test]
    fn skips_comments_blanks_and_garbage_lines() {
        let text = "# elsewhere saved cities\n1850147\n\nTokyo\n-1\n2643743\n";
        assert_eq!(parse(text), [1850147, 2643743]);
    }

    #[test]
    fn drops_duplicates_keeping_the_first() {
        assert_eq!(parse("1850147\n2643743\n1850147\n"), [1850147, 2643743]);
    }

    #[test]
    fn render_round_trips_through_parse() {
        let saved = [1850147, 2643743, 5128581];
        assert_eq!(parse(&render(&saved)), saved);
    }

    #[test]
    fn a_missing_file_is_an_empty_list() {
        let path = env::temp_dir().join("elsewhere-saved-that-does-not-exist");
        assert_eq!(read(&path), Some(Vec::new()));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_an_empty_list() {
        // A directory is there but is no file to read.
        assert_eq!(read(&env::temp_dir()), None);
    }

    #[test]
    fn resolves_the_dir_from_xdg_config_home() {
        assert_eq!(
            dir(Some("/xdg"), Some("/home/someone")),
            Some(PathBuf::from("/xdg/elsewhere"))
        );
    }

    #[test]
    fn an_empty_xdg_variable_falls_back_to_home() {
        assert_eq!(
            dir(Some(""), Some("/home/someone")),
            Some(PathBuf::from("/home/someone/.config/elsewhere"))
        );
    }

    #[test]
    fn yields_no_dir_without_home_or_xdg() {
        assert_eq!(dir(None, None), None);
    }

    #[test]
    fn moves_a_row_before_another() {
        let mut saved = vec![1, 2, 3];
        assert!(reorder(&mut saved, 3, Some(1)));
        assert_eq!(saved, [3, 1, 2]);
    }

    #[test]
    fn moves_a_row_to_the_end() {
        let mut saved = vec![1, 2, 3];
        assert!(reorder(&mut saved, 1, None));
        assert_eq!(saved, [2, 3, 1]);
    }

    #[test]
    fn dropping_a_row_where_it_already_sits_changes_nothing() {
        let mut saved = vec![1, 2, 3];
        assert!(!reorder(&mut saved, 2, Some(2)));
        assert!(!reorder(&mut saved, 2, Some(3)));
        assert!(!reorder(&mut saved, 3, None));
        assert_eq!(saved, [1, 2, 3]);
    }

    #[test]
    fn orders_cities_west_to_east() {
        let cities = vec![
            (1850147, 139.69171),
            (5128581, -74.006),
            (2643743, -0.12574),
        ];
        assert_eq!(westward(cities), [5128581, 2643743, 1850147]);
    }

    #[test]
    fn equal_longitudes_keep_their_given_order() {
        let cities = vec![(3, 10.0), (1, 10.0), (2, 10.0)];
        assert_eq!(westward(cities), [3, 1, 2]);
    }

    #[test]
    fn ignores_an_unknown_dragged_id() {
        let mut saved = vec![1, 2, 3];
        assert!(!reorder(&mut saved, 9, Some(1)));
        assert!(!reorder(&mut saved, 1, Some(9)));
        assert_eq!(saved, [1, 2, 3]);
    }
}
