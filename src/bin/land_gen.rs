//! Turns the Natural Earth 110m land GeoJSON in data/raw/ into the map's
//! coarse outlines in data/land.tsv: outer rings only, simplified.

// Shared with zones_gen, which reads the feature properties this one ignores.
#[allow(dead_code)]
mod geojson;

use std::fs;
use std::path::Path;

use geojson::{Point, Result, simplify};

const RAW_PATH: &str = "data/raw/ne_110m_land.geojson";
const OUT_PATH: &str = "data/land.tsv";

/// Rings whose every point is below this are off the bottom of the map crop.
const SOUTH_CUTOFF: f64 = -56.0;

/// Douglas-Peucker tolerance in degrees; the map is about a pixel per degree.
const TOLERANCE: f64 = 0.15;

/// Below this a ring has no area worth drawing.
const MIN_POINTS: usize = 4;

const OUT_HEADER: &str = "#lon,lat pairs, one ring per line";

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let text = geojson::read(Path::new(RAW_PATH), "scripts/fetch-naturalearth.sh")?;
    let polygons: Vec<Vec<Vec<Point>>> = geojson::features(&text)?
        .into_iter()
        .flat_map(|feature| feature.polygons)
        .collect();
    let features = polygons.len();

    let outers: Vec<Vec<Point>> = polygons
        .into_iter()
        .map(|mut rings| rings.swap_remove(0))
        .filter(|ring| !is_far_south(ring))
        .collect();

    let before: usize = outers.iter().map(Vec::len).sum();
    let rings: Vec<Vec<Point>> = outers
        .into_iter()
        .map(|ring| simplify(&ring, TOLERANCE))
        .filter(|ring| ring.len() >= MIN_POINTS)
        .collect();
    let after: usize = rings.iter().map(Vec::len).sum();

    let out = render(&rings);
    fs::write(OUT_PATH, &out)?;

    println!("features          {features}");
    println!("rings kept        {}", rings.len());
    println!("points before     {before}");
    println!("points after      {after}");
    println!("bytes             {}", out.len());
    println!("\nemitted {} rings to {OUT_PATH}", rings.len());

    Ok(())
}

/// Rings are left open: parsing drops the closing point GeoJSON repeats.
fn render(rings: &[Vec<Point>]) -> String {
    let mut out = String::from(OUT_HEADER);
    for ring in rings {
        out.push('\n');
        geojson::render_ring(&mut out, ring);
    }
    out.push('\n');
    out
}

fn is_far_south(ring: &[Point]) -> bool {
    ring.iter().all(|point| point.lat < SOUTH_CUTOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lon: f64, lat: f64) -> Point {
        Point { lon, lat }
    }

    #[test]
    fn drops_rings_wholly_south_of_the_cutoff() {
        assert!(is_far_south(&[point(0.0, -80.0), point(1.0, -60.0)]));
        assert!(!is_far_south(&[point(0.0, -80.0), point(1.0, -50.0)]));
    }

    #[test]
    fn renders_a_header_and_one_line_per_ring() {
        let rings = vec![
            vec![point(-0.1275, 51.5072), point(2.3522, 48.8566)],
            vec![point(139.6917, 35.6895)],
        ];
        assert_eq!(
            render(&rings),
            "#lon,lat pairs, one ring per line\n-0.13,51.51 2.35,48.86\n139.69,35.69\n"
        );
    }
}
