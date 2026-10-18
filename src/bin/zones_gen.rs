//! Turns timezone-boundary-builder's "now" releases in data/raw/ into the
//! zone borders the map draws, written to data/zones.tsv.
//!
//! The "now" releases merge zones that keep the same clock from now on, so
//! every border is a real change of time for at least part of the year.
//! Borders come from the release with oceans, for its nautical zones; the
//! land-only release marks which are just the rim of a country's waters.
//! Those would outline every coast, so they are dropped unless a nautical
//! line detours along them round waters that bulge across it.

mod geojson;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use geojson::{Point, Result, Value, distance_to_segment, simplify};

const OCEANS_PATH: &str = "data/raw/combined-with-oceans-now.json";
const LAND_PATH: &str = "data/raw/combined-now.json";
const FETCH: &str = "scripts/fetch-timezones.sh";
const OUT_PATH: &str = "data/zones.tsv";

/// The map's crop, as in src/geo.rs.
const NORTH: f64 = 84.0;
const SOUTH: f64 = -56.0;

/// Douglas-Peucker tolerance in degrees. Finer than land.tsv's, since a
/// border is drawn as a bare line rather than the rim of a fill.
const TOLERANCE: f64 = 0.1;

/// How far apart, in degrees, a rim's end and a nautical line's loose end can
/// be and still meet: the polygons leave the odd sliver rather than share
/// the point.
const SNAP: f64 = 0.1;

/// How far, in degrees, a rim can stray from the straight line between its
/// ends and still be a detour. In the 2026e release the widest, round Peru's
/// waters off 82.5° W and South Georgia, stray just over one; a coast the
/// line merely meets at both ends, as Brazil's at 37.5° W, strays two or more.
const DETOUR: f64 = 1.5;

const OUT_HEADER: &str =
    "#the zones a border parts, comma separated, then its lon,lat pairs space separated";

/// Bit-exact, as the data repeats a shared vertex bit for bit, across both
/// releases too.
type Key = (u64, u64);
/// Ends sorted, so it is the same edge either way round.
type Edge = (Key, Key);
/// A ring, outer or hole, with its zone's index into the names.
type Ring = (usize, Vec<Point>);

/// What an edge of the ocean release is in the land-only one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shore {
    /// Missing from it: open sea on both sides.
    Sea,
    /// Only one zone has it: where that zone's waters end.
    Rim,
    /// Two or more zones share it.
    Inland,
}

/// `zones` index into the feature names.
#[derive(Debug, PartialEq)]
struct Border {
    zones: Vec<usize>,
    points: Vec<Point>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // One release at a time, as each parses to over a gigabyte.
    let (_, land) = zones(LAND_PATH)?;
    let shores = shores(&land);
    drop(land);
    let (names, rings) = zones(OCEANS_PATH)?;
    let before: usize = rings.iter().map(|(_, ring)| ring.len()).sum();

    let borders: Vec<Border> = borders(&rings, &shores)
        .into_iter()
        .map(|border| Border {
            points: simplify(&border.points, TOLERANCE),
            ..border
        })
        .collect();
    let after: usize = borders.iter().map(|border| border.points.len()).sum();

    let out = render(&borders, &names);
    fs::write(OUT_PATH, &out)?;

    println!("zones             {}", names.len());
    println!("rings             {}", rings.len());
    println!("borders           {}", borders.len());
    println!("points before     {before}");
    println!("points after      {after}");
    println!("bytes             {}", out.len());
    println!("\nemitted {} borders to {OUT_PATH}", borders.len());

    Ok(())
}

fn zones(path: &str) -> Result<(Vec<String>, Vec<Ring>)> {
    let text = geojson::read(Path::new(path), FETCH)?;
    let features = geojson::features(&text)?;
    drop(text);

    let names: Vec<String> = features
        .iter()
        .enumerate()
        .map(|(i, feature)| tzid(&feature.properties, i + 1))
        .collect::<Result<_>>()?;
    let rings = features
        .into_iter()
        .enumerate()
        .flat_map(|(i, feature)| {
            feature
                .polygons
                .into_iter()
                .flatten()
                .map(move |ring| (i, ring))
        })
        .collect();
    Ok((names, rings))
}

/// The IANA name the release files the feature's group of zones under.
fn tzid(properties: &Value, feature: usize) -> Result<String> {
    match properties.get("tzid") {
        Some(Value::String(tzid)) => Ok(tzid.clone()),
        _ => Err(format!("feature {feature}: no \"tzid\" property").into()),
    }
}

fn key(point: Point) -> Key {
    (point.lon.to_bits(), point.lat.to_bits())
}

fn edge(a: Key, b: Key) -> Edge {
    (a.min(b), a.max(b))
}

/// The point behind each key, and the zones that have each edge, sorted.
fn owners(rings: &[Ring]) -> (HashMap<Key, Point>, HashMap<Edge, Vec<usize>>) {
    let mut points: HashMap<Key, Point> = HashMap::new();
    let mut owners: HashMap<Edge, Vec<usize>> = HashMap::new();
    for (zone, ring) in rings {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            if a == b || beyond_crop(a, b) {
                continue;
            }
            points.insert(key(a), a);
            points.insert(key(b), b);
            let zones = owners.entry(edge(key(a), key(b))).or_default();
            if !zones.contains(zone) {
                zones.push(*zone);
            }
        }
    }
    for zones in owners.values_mut() {
        zones.sort_unstable();
    }
    (points, owners)
}

/// For the land-only release. Besides where a zone's waters end, its rims
/// include the frame and the inside rims of disputed ground.
fn shores(rings: &[Ring]) -> HashMap<Edge, Shore> {
    let (_, owners) = owners(rings);
    owners
        .into_iter()
        .map(|(edge, zones)| {
            let shore = if zones.len() == 1 {
                Shore::Rim
            } else {
                Shore::Inland
            };
            (edge, shore)
        })
        .collect()
}

/// Runs of edge two or more zones share. Edges only one zone has are the
/// frame or the inside rim of disputed ground, not a change of clock.
///
/// A run stops wherever its zones change, three ways meet, or it passes onto
/// or off a rim, so each has one set of zones and one kind of edge. A run
/// along a rim is a coast seen from the sea and is dropped, unless it is a
/// detour: a nautical line between the same zones meets it at both ends and
/// it strays no more than `DETOUR` from the straight line between them.
fn borders(rings: &[Ring], shores: &HashMap<Edge, Shore>) -> Vec<Border> {
    let (points, mut owners) = owners(rings);
    owners.retain(|_, zones| zones.len() >= 2);
    let shore = |edge: Edge| shores.get(&edge).copied().unwrap_or(Shore::Sea);
    let rim = |edge: Edge| shore(edge) == Shore::Rim;

    let mut next: HashMap<Key, Vec<Key>> = HashMap::new();
    for &(a, b) in owners.keys() {
        next.entry(a).or_default().push(b);
        next.entry(b).or_default().push(a);
    }
    // Where a run has to stop: anywhere but the middle of a plain stretch.
    let stops: HashSet<Key> = next
        .iter()
        .filter(|(here, neighbours)| match neighbours.as_slice() {
            [a, b] => {
                let (a, b) = (edge(**here, *a), edge(**here, *b));
                owners[&a] != owners[&b] || rim(a) != rim(b)
            }
            _ => true,
        })
        .map(|(here, _)| *here)
        .collect();

    let mut walked: HashSet<Edge> = HashSet::new();
    let mut runs: Vec<Vec<Key>> = Vec::new();
    // Runs from every stop, then the loops that meet none, such as an
    // enclave. Sorted so the output is reproducible.
    let mut starts: Vec<Key> = stops.iter().copied().collect();
    starts.sort_unstable();
    let mut loops: Vec<Key> = next.keys().copied().collect();
    loops.sort_unstable();
    for start in starts.into_iter().chain(loops) {
        let mut ways = next[&start].clone();
        ways.sort_unstable();
        for way in ways {
            if walked.contains(&edge(start, way)) {
                continue;
            }
            let mut run = vec![start];
            let (mut from, mut here) = (start, way);
            loop {
                walked.insert(edge(from, here));
                run.push(here);
                if here == start || stops.contains(&here) {
                    break;
                }
                let Some(&onward) = next[&here]
                    .iter()
                    .find(|&&n| n != from && !walked.contains(&edge(here, n)))
                else {
                    break;
                };
                (from, here) = (here, onward);
            }
            runs.push(run);
        }
    }

    // A rim run's end is at sea if a nautical line between the same zones
    // carries on from it, or has a loose end within `SNAP` of it.
    let loose: Vec<(Point, &Vec<usize>)> = next
        .iter()
        .filter_map(|(here, neighbours)| match neighbours.as_slice() {
            [only] => {
                let edge = edge(*here, *only);
                (shore(edge) == Shore::Sea).then(|| (points[here], &owners[&edge]))
            }
            _ => None,
        })
        .collect();
    let at_sea = |end: Key, zones: &Vec<usize>| {
        let on_line = next[&end].iter().any(|&n| {
            let edge = edge(end, n);
            shore(edge) == Shore::Sea && owners[&edge] == *zones
        });
        let near = |(point, owners): &(Point, &Vec<usize>)| {
            *owners == zones
                && (point.lon - points[&end].lon).hypot(point.lat - points[&end].lat) < SNAP
        };
        on_line || loose.iter().any(near)
    };
    runs.into_iter()
        .filter(|run| {
            let first = edge(run[0], run[1]);
            let (start, end) = (run[0], run[run.len() - 1]);
            let (a, b) = (points[&start], points[&end]);
            let hugs = || {
                run.iter()
                    .all(|key| distance_to_segment(points[key], a, b) <= DETOUR)
            };
            !rim(first)
                || (start != end
                    && at_sea(start, &owners[&first])
                    && at_sea(end, &owners[&first])
                    && hugs())
        })
        .map(|run| Border {
            zones: owners[&edge(run[0], run[1])].clone(),
            points: run.iter().map(|key| points[key]).collect(),
        })
        .collect()
}

fn beyond_crop(a: Point, b: Point) -> bool {
    (a.lat > NORTH && b.lat > NORTH) || (a.lat < SOUTH && b.lat < SOUTH)
}

fn render(borders: &[Border], names: &[String]) -> String {
    let mut out = String::from(OUT_HEADER);
    for border in borders {
        out.push('\n');
        let zones: Vec<&str> = border.zones.iter().map(|i| names[*i].as_str()).collect();
        out.push_str(&zones.join(","));
        out.push('\t');
        geojson::render_ring(&mut out, &border.points);
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lon: f64, lat: f64) -> Point {
        Point { lon, lat }
    }

    fn square(lon: f64, lat: f64, side: f64) -> Vec<Point> {
        vec![
            point(lon, lat),
            point(lon + side, lat),
            point(lon + side, lat + side),
            point(lon, lat + side),
        ]
    }

    type Line = (Vec<usize>, Vec<(f64, f64)>);

    /// Normalised for direction and order, so tests ignore how runs were
    /// walked.
    fn lines(borders: &[Border]) -> Vec<Line> {
        let mut lines: Vec<_> = borders
            .iter()
            .map(|border| {
                let mut points: Vec<(f64, f64)> =
                    border.points.iter().map(|p| (p.lon, p.lat)).collect();
                if points.first() > points.last() {
                    points.reverse();
                }
                (border.zones.clone(), points)
            })
            .collect();
        lines.sort_by(|a, b| a.partial_cmp(b).unwrap());
        lines
    }

    #[test]
    fn reads_the_zone_name() {
        let properties = geojson::parse(r#"{"tzid":"Asia/Kolkata"}"#).unwrap();
        assert_eq!(tzid(&properties, 1).unwrap(), "Asia/Kolkata");
        let properties = geojson::parse(r#"{"zone":5.5}"#).unwrap();
        let err = tzid(&properties, 3).unwrap_err().to_string();
        assert!(err.contains("feature 3"), "{err}");
    }

    #[test]
    fn keeps_only_the_edge_two_zones_share() {
        // Meeting along x = 1; their other edges stand in for the frame.
        let west = square(0.0, 0.0, 1.0);
        let east = square(1.0, 0.0, 1.0);
        let borders = borders(&[(0, west), (1, east)], &HashMap::new());
        assert_eq!(
            lines(&borders),
            vec![(vec![0, 1], vec![(1.0, 0.0), (1.0, 1.0)])]
        );
    }

    #[test]
    fn breaks_a_border_where_three_zones_meet() {
        // A tall zone west of two stacked ones; all three meet at (1, 1).
        let west = vec![
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(1.0, 1.0),
            point(1.0, 2.0),
            point(0.0, 2.0),
        ];
        let lower = square(1.0, 0.0, 1.0);
        let upper = square(1.0, 1.0, 1.0);
        let borders = borders(&[(0, west), (1, lower), (2, upper)], &HashMap::new());
        assert_eq!(
            lines(&borders),
            vec![
                (vec![0, 1], vec![(1.0, 0.0), (1.0, 1.0)]),
                (vec![0, 2], vec![(1.0, 1.0), (1.0, 2.0)]),
                (vec![1, 2], vec![(1.0, 1.0), (2.0, 1.0)]),
            ]
        );
    }

    #[test]
    fn walks_an_enclave_as_one_closed_border() {
        let outer = square(0.0, 0.0, 10.0);
        let enclave = square(4.0, 4.0, 2.0);
        // The hole the enclave fills, walked the other way round.
        let mut hole = enclave.clone();
        hole.reverse();
        let borders = borders(&[(0, outer), (0, hole), (1, enclave)], &HashMap::new());
        assert_eq!(borders.len(), 1);
        let border = &borders[0];
        assert_eq!(border.zones, vec![0, 1]);
        assert_eq!(border.points.len(), 5);
        assert_eq!(border.points.first(), border.points.last());
    }

    #[test]
    fn leaves_out_the_rim_of_a_countrys_waters() {
        // A country's waters (zone 0) against open sea on another clock
        // (zone 1). In the land-only release the rim is zone 0's alone.
        let waters = square(0.0, 0.0, 1.0);
        let sea = square(1.0, 0.0, 1.0);
        let shores = shores(&[(0, waters.clone())]);
        assert!(borders(&[(0, waters), (1, sea)], &shores).is_empty());
    }

    #[test]
    fn keeps_the_rim_a_nautical_line_detours_along() {
        // A nautical line at x = 1 between zones 0 and 1, detouring along the
        // rim of zone 0 waters that bulge east across it from y = 1 to y = 2.
        let west = vec![
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(1.0, 1.0),
            point(1.5, 1.0),
            point(1.5, 2.0),
            point(1.0, 2.0),
            point(1.0, 3.0),
            point(0.0, 3.0),
        ];
        let east = vec![
            point(1.0, 0.0),
            point(2.0, 0.0),
            point(2.0, 3.0),
            point(1.0, 3.0),
            point(1.0, 2.0),
            point(1.5, 2.0),
            point(1.5, 1.0),
            point(1.0, 1.0),
        ];
        let waters = vec![
            point(0.5, 1.0),
            point(1.0, 1.0),
            point(1.5, 1.0),
            point(1.5, 2.0),
            point(1.0, 2.0),
            point(0.5, 2.0),
        ];
        let shores = shores(&[(0, waters)]);
        assert_eq!(
            lines(&borders(&[(0, west), (1, east)], &shores)),
            vec![
                (vec![0, 1], vec![(1.0, 0.0), (1.0, 1.0)]),
                (
                    vec![0, 1],
                    vec![(1.0, 1.0), (1.5, 1.0), (1.5, 2.0), (1.0, 2.0)]
                ),
                (vec![0, 1], vec![(1.0, 2.0), (1.0, 3.0)]),
            ]
        );
    }

    #[test]
    fn leaves_out_a_coast_a_nautical_line_meets_at_both_ends() {
        // As above, but the waters reach far east of the line, as West
        // Africa's coast does between two points of 7.5° W.
        let west = vec![
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(1.0, 1.0),
            point(9.0, 1.0),
            point(9.0, 2.0),
            point(1.0, 2.0),
            point(1.0, 3.0),
            point(0.0, 3.0),
        ];
        let east = vec![
            point(1.0, 0.0),
            point(10.0, 0.0),
            point(10.0, 3.0),
            point(1.0, 3.0),
            point(1.0, 2.0),
            point(9.0, 2.0),
            point(9.0, 1.0),
            point(1.0, 1.0),
        ];
        let waters = vec![
            point(0.5, 1.0),
            point(1.0, 1.0),
            point(9.0, 1.0),
            point(9.0, 2.0),
            point(1.0, 2.0),
            point(0.5, 2.0),
        ];
        let shores = shores(&[(0, waters)]);
        assert_eq!(
            lines(&borders(&[(0, west), (1, east)], &shores)),
            vec![
                (vec![0, 1], vec![(1.0, 0.0), (1.0, 1.0)]),
                (vec![0, 1], vec![(1.0, 2.0), (1.0, 3.0)]),
            ]
        );
    }

    #[test]
    fn bridges_a_sliver_between_a_rim_and_its_nautical_line() {
        // The detour two tests up, but where the bulge rejoins the line the
        // zones each go their own way round a sliver, so rim and line share
        // no point.
        let west = vec![
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(1.0, 1.0),
            point(1.5, 1.0),
            point(1.5, 2.0),
            point(1.05, 2.0),
            point(1.0, 2.05),
            point(1.0, 3.0),
            point(0.0, 3.0),
        ];
        let east = vec![
            point(1.0, 0.0),
            point(2.0, 0.0),
            point(2.0, 3.0),
            point(1.0, 3.0),
            point(1.0, 2.05),
            point(1.02, 2.02),
            point(1.05, 2.0),
            point(1.5, 2.0),
            point(1.5, 1.0),
            point(1.0, 1.0),
        ];
        let waters = vec![
            point(0.5, 1.0),
            point(1.0, 1.0),
            point(1.5, 1.0),
            point(1.5, 2.0),
            point(1.05, 2.0),
            point(0.5, 2.0),
        ];
        let shores = shores(&[(0, waters)]);
        assert_eq!(
            lines(&borders(&[(0, west), (1, east)], &shores)),
            vec![
                (vec![0, 1], vec![(1.0, 0.0), (1.0, 1.0)]),
                (
                    vec![0, 1],
                    vec![(1.0, 1.0), (1.5, 1.0), (1.5, 2.0), (1.05, 2.0)]
                ),
                (vec![0, 1], vec![(1.0, 2.05), (1.0, 3.0)]),
            ]
        );
    }

    #[test]
    fn keeps_a_border_between_two_countries_from_the_land_release() {
        let west = square(0.0, 0.0, 1.0);
        let east = square(1.0, 0.0, 1.0);
        let shores = shores(&[(0, west.clone()), (1, east.clone())]);
        assert_eq!(borders(&[(0, west), (1, east)], &shores).len(), 1);
    }

    #[test]
    fn leaves_out_what_lies_off_the_crop() {
        let west = square(0.0, 85.0, 1.0);
        let east = square(1.0, 85.0, 1.0);
        assert!(borders(&[(0, west), (1, east)], &HashMap::new()).is_empty());
    }

    #[test]
    fn renders_one_border_per_line() {
        let names = vec!["Asia/Kolkata".to_string(), "Asia/Kathmandu".to_string()];
        let borders = vec![Border {
            zones: vec![0, 1],
            points: vec![point(80.06, 28.84), point(88.2, 26.4)],
        }];
        assert_eq!(
            render(&borders, &names),
            format!("{OUT_HEADER}\nAsia/Kolkata,Asia/Kathmandu\t80.06,28.84 88.20,26.40\n")
        );
    }
}
