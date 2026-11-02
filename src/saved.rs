//! The saved city list on disk, one geonameid per line so it stays
//! hand-editable. Problems degrade to fewer cities rather than an error, but a
//! file that could not be read must never be mistaken for an empty one and
//! written over. The settings file lives beside it and is read and written
//! the same way.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const HEADER: &str = "# elsewhere saved cities, one geonameid per line, in list order";

/// Takes the variables as arguments so tests need not touch the process
/// environment. `None` silently skips persistence.
fn dir(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    // An empty variable counts as unset, per the XDG spec.
    let xdg = xdg.filter(|path| !path.is_empty());
    if let Some(xdg) = xdg {
        return Some(PathBuf::from(xdg).join("elsewhere"));
    }
    let home = home.filter(|path| !path.is_empty())?;
    Some(PathBuf::from(home).join(".config").join("elsewhere"))
}

/// `name` in the app's config directory.
pub(crate) fn config_file(name: &str) -> Option<PathBuf> {
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
    dir.map(|dir| dir.join(name))
}

/// For messages: home shortened to `~`, so the part that matters fits.
fn shown(path: &Path) -> String {
    let home = env::var("HOME").ok().filter(|home| !home.is_empty());
    match home.and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

/// io's message in the app's voice: lower case, without the os error number.
fn reason(e: &io::Error) -> String {
    let message = e.to_string();
    let message = match message.find(" (os error") {
        Some(at) => &message[..at],
        None => &message,
    };
    let mut chars = message.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A missing file is `Ok(None)`. Anything else that stops the read comes back
/// as a message naming the file, for the app to show.
pub(crate) fn read_text(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            Err(format!("can't read {}: {}", shown(path), reason(&e)))
        }
    }
}

/// Writes through a temporary file renamed into place, so an interrupted write
/// cannot leave half a file. The error names the file, for the app to show.
pub(crate) fn write_text(path: &Path, text: &str) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)
        .and_then(|()| fs::write(&temp, text))
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|e| {
            eprintln!("{}: {e}", path.display());
            format!("can't save {}: {}", shown(path), reason(&e))
        })
}

/// Drops blanks, comments, garbage and repeats.
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

/// Only a missing file counts as empty; one that exists but cannot be read
/// is an error.
fn read(path: &Path) -> Result<Vec<u32>, String> {
    Ok(read_text(path)?
        .map(|text| parse(&text))
        .unwrap_or_default())
}

/// An error when the file could not be read, in which case the session must
/// not save over it.
pub fn load() -> Result<Vec<u32>, String> {
    match config_file("saved") {
        Some(path) => read(&path),
        None => Ok(Vec::new()),
    }
}

/// Failures come back for the app to show; they must not break the running
/// session.
pub fn save(saved: &[u32]) -> Result<(), String> {
    match config_file("saved") {
        Some(path) => write_text(&path, &render(saved)),
        None => Ok(()),
    }
}

/// Moves `dragged` before `before`, or to the end. Returns whether the order
/// changed, so a no-op drop costs nothing. Unknown ids change nothing.
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

/// Earliest clock first: by UTC offset in seconds, then by longitude within
/// an offset, so a zone that reaches east of a later one still reads in clock
/// order. Stable, so ties keep their order; a NaN longitude sorts as equal
/// rather than panicking.
pub fn by_clock(mut cities: Vec<(u32, i32, f64)>) -> Vec<u32> {
    cities.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    });
    cities
        .into_iter()
        .map(|(geonameid, _, _)| geonameid)
        .collect()
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
        assert_eq!(read(&path), Ok(Vec::new()));
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_an_empty_list() {
        // A directory is there but is no file to read.
        let error = read(&env::temp_dir()).unwrap_err();
        assert!(error.starts_with("can't read "), "{error}");
    }

    #[test]
    fn io_errors_read_lower_case_without_the_os_number() {
        let e = io::Error::from_raw_os_error(13);
        assert_eq!(reason(&e), "permission denied");
    }

    #[test]
    fn a_write_into_a_missing_dir_creates_it_and_round_trips() {
        let dir = env::temp_dir().join(format!("elsewhere-saved-{}", std::process::id()));
        let path = dir.join("saved");
        let saved = [1850147, 2643743];
        assert_eq!(write_text(&path, &render(&saved)), Ok(()));
        assert_eq!(read(&path), Ok(saved.to_vec()));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_write_names_the_file() {
        // A path under a regular file cannot be created.
        let file = env::temp_dir().join(format!("elsewhere-blocker-{}", std::process::id()));
        fs::write(&file, "").unwrap();
        let error = write_text(&file.join("saved"), "").unwrap_err();
        fs::remove_file(&file).unwrap();
        assert!(
            error.starts_with("can't save ") && error.contains("saved"),
            "{error}"
        );
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
    fn orders_cities_by_offset_not_longitude() {
        // Madrid lies west of London but keeps a later clock.
        let cities = vec![
            (3117735, 3600, -3.70256),
            (2643743, 0, -0.12574),
            (5128581, -5 * 3600, -74.006),
        ];
        assert_eq!(by_clock(cities), [5128581, 2643743, 3117735]);
    }

    #[test]
    fn a_shared_offset_reads_west_to_east() {
        let cities = vec![(2, 3600, 13.405), (1, 3600, 2.3522), (3, 3600, 12.4964)];
        assert_eq!(by_clock(cities), [1, 3, 2]);
    }

    #[test]
    fn equal_keys_keep_their_given_order() {
        let cities = vec![(3, 0, 10.0), (1, 0, 10.0), (2, 0, 10.0)];
        assert_eq!(by_clock(cities), [3, 1, 2]);
    }

    #[test]
    fn ignores_an_unknown_dragged_id() {
        let mut saved = vec![1, 2, 3];
        assert!(!reorder(&mut saved, 9, Some(1)));
        assert!(!reorder(&mut saved, 1, Some(9)));
        assert_eq!(saved, [1, 2, 3]);
    }
}
