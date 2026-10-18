//! What the map generators share: a small JSON reader, just enough GeoJSON to
//! pull polygons and properties out of a feature collection, and the line
//! simplification both apply.
//!
//! A directory module because a `src/bin/geojson.rs` would be built as a
//! binary of its own.

use std::error::Error;
use std::fs;
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub lon: f64,
    pub lat: f64,
}

/// A JSON value. Objects keep their keys in file order.
#[derive(Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// Each polygon is a list of rings, outer first. A Polygon geometry gives one
/// polygon, a MultiPolygon several.
#[derive(Debug)]
pub struct Feature {
    pub properties: Value,
    pub polygons: Vec<Vec<Vec<Point>>>,
}

pub fn features(text: &str) -> Result<Vec<Feature>> {
    let root = parse(text)?;
    let Some(Value::Array(features)) = root.get("features") else {
        return Err("no \"features\" array".into());
    };
    if features.is_empty() {
        return Err("no features found".into());
    }
    features
        .iter()
        .enumerate()
        .map(|(i, value)| feature(value, i + 1))
        .collect()
}

fn feature(value: &Value, feature: usize) -> Result<Feature> {
    let geometry = value
        .get("geometry")
        .ok_or_else(|| format!("feature {feature}: no geometry"))?;
    let coordinates = geometry
        .get("coordinates")
        .ok_or_else(|| format!("feature {feature}: no coordinates"))?;
    let polygons = match geometry.get("type") {
        Some(Value::String(kind)) if kind == "Polygon" => vec![polygon(coordinates, feature)?],
        Some(Value::String(kind)) if kind == "MultiPolygon" => list(coordinates, feature)?
            .iter()
            .map(|polygon| self::polygon(polygon, feature))
            .collect::<Result<_>>()?,
        Some(Value::String(kind)) => {
            return Err(format!("feature {feature}: expected a polygon, got {kind}").into());
        }
        _ => return Err(format!("feature {feature}: geometry without a type").into()),
    };
    let properties = match value.get("properties") {
        Some(Value::Object(entries)) => Value::Object(entries.iter().map(clone_entry).collect()),
        _ => Value::Null,
    };
    Ok(Feature {
        properties,
        polygons,
    })
}

/// Copied out so the parsed tree need not be kept alive for them. Properties
/// are flat, so nested values are dropped.
fn clone_entry((key, value): &(String, Value)) -> (String, Value) {
    let value = match value {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(*b),
        Value::Number(n) => Value::Number(*n),
        Value::String(s) => Value::String(s.clone()),
        Value::Array(_) | Value::Object(_) => Value::Null,
    };
    (key.clone(), value)
}

fn list(value: &Value, feature: usize) -> Result<&[Value]> {
    match value {
        Value::Array(items) => Ok(items),
        _ => Err(format!("feature {feature}: coordinates are not nested arrays").into()),
    }
}

/// GeoJSON repeats a ring's first point last; it is dropped here so nothing
/// downstream sees it.
fn polygon(value: &Value, feature: usize) -> Result<Vec<Vec<Point>>> {
    let rings = list(value, feature)?;
    if rings.is_empty() {
        return Err(format!("feature {feature}: no rings").into());
    }
    rings
        .iter()
        .map(|ring| {
            let mut points: Vec<Point> = list(ring, feature)?
                .iter()
                .map(|point| self::point(point, feature))
                .collect::<Result<_>>()?;
            match (points.first(), points.last()) {
                (Some(first), Some(last)) if first == last => {
                    points.pop();
                    Ok(points)
                }
                _ => Err(format!("feature {feature}: ring is not closed").into()),
            }
        })
        .collect()
}

fn point(value: &Value, feature: usize) -> Result<Point> {
    match value {
        Value::Array(pair) => match pair.as_slice() {
            [Value::Number(lon), Value::Number(lat)] => Ok(Point {
                lon: *lon,
                lat: *lat,
            }),
            _ => Err(format!("feature {feature}: point is not a pair of numbers").into()),
        },
        _ => Err(format!("feature {feature}: point is not a pair of numbers").into()),
    }
}

pub fn parse(text: &str) -> Result<Value> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let value = value(bytes, &mut i)?;
    skip_whitespace(bytes, &mut i);
    if i != bytes.len() {
        return Err(format!("byte {i}: trailing text after the value").into());
    }
    Ok(value)
}

fn value(bytes: &[u8], i: &mut usize) -> Result<Value> {
    skip_whitespace(bytes, i);
    match bytes.get(*i) {
        Some(b'[') => {
            *i += 1;
            let mut items = Vec::new();
            if close(bytes, i, b']') {
                return Ok(Value::Array(items));
            }
            loop {
                items.push(value(bytes, i)?);
                if separator(bytes, i, b']')? {
                    return Ok(Value::Array(items));
                }
            }
        }
        Some(b'{') => {
            *i += 1;
            let mut entries = Vec::new();
            if close(bytes, i, b'}') {
                return Ok(Value::Object(entries));
            }
            loop {
                skip_whitespace(bytes, i);
                let key = string(bytes, i)?;
                skip_whitespace(bytes, i);
                if bytes.get(*i) != Some(&b':') {
                    return Err(format!("byte {i}: expected \":\" after a key").into());
                }
                *i += 1;
                entries.push((key, value(bytes, i)?));
                if separator(bytes, i, b'}')? {
                    return Ok(Value::Object(entries));
                }
            }
        }
        Some(b'"') => Ok(Value::String(string(bytes, i)?)),
        Some(b't') => literal(bytes, i, "true", Value::Bool(true)),
        Some(b'f') => literal(bytes, i, "false", Value::Bool(false)),
        Some(b'n') => literal(bytes, i, "null", Value::Null),
        Some(_) => number(bytes, i),
        None => Err(unterminated(*i)),
    }
}

/// For an empty array or object.
fn close(bytes: &[u8], i: &mut usize, end: u8) -> bool {
    skip_whitespace(bytes, i);
    let closed = bytes.get(*i) == Some(&end);
    if closed {
        *i += 1;
    }
    closed
}

/// After an item: whether it was the last, i.e. `end` rather than a comma.
fn separator(bytes: &[u8], i: &mut usize, end: u8) -> Result<bool> {
    skip_whitespace(bytes, i);
    match bytes.get(*i) {
        Some(b',') => {
            *i += 1;
            Ok(false)
        }
        Some(c) if *c == end => {
            *i += 1;
            Ok(true)
        }
        Some(c) => Err(format!("byte {i}: unexpected {:?}", *c as char).into()),
        None => Err(unterminated(*i)),
    }
}

fn literal(bytes: &[u8], i: &mut usize, word: &str, value: Value) -> Result<Value> {
    if bytes[*i..].starts_with(word.as_bytes()) {
        *i += word.len();
        Ok(value)
    } else {
        Err(format!("byte {i}: expected {word}").into())
    }
}

fn string(bytes: &[u8], i: &mut usize) -> Result<String> {
    if bytes.get(*i) != Some(&b'"') {
        return Err(format!("byte {i}: expected a string").into());
    }
    *i += 1;
    let mut out = String::new();
    loop {
        let start = *i;
        while matches!(bytes.get(*i), Some(c) if *c != b'"' && *c != b'\\') {
            *i += 1;
        }
        out.push_str(std::str::from_utf8(&bytes[start..*i])?);
        match bytes.get(*i) {
            Some(b'"') => {
                *i += 1;
                return Ok(out);
            }
            Some(b'\\') => {
                *i += 1;
                out.push(escape(bytes, i)?);
            }
            _ => return Err(unterminated(*i)),
        }
    }
}

/// `i` starts just past the backslash.
fn escape(bytes: &[u8], i: &mut usize) -> Result<char> {
    let at = *i;
    let Some(c) = bytes.get(*i) else {
        return Err(unterminated(*i));
    };
    *i += 1;
    Ok(match c {
        b'"' => '"',
        b'\\' => '\\',
        b'/' => '/',
        b'b' => '\u{8}',
        b'f' => '\u{c}',
        b'n' => '\n',
        b'r' => '\r',
        b't' => '\t',
        b'u' => {
            let high = hex4(bytes, i)?;
            // Outside the basic plane, JSON spells a character as a pair.
            let code = if (0xD800..0xDC00).contains(&high) && bytes[*i..].starts_with(b"\\u") {
                *i += 2;
                let low = hex4(bytes, i)?;
                0x10000 + ((high - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF)
            } else {
                high
            };
            char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
        }
        _ => return Err(format!("byte {at}: bad escape").into()),
    })
}

fn hex4(bytes: &[u8], i: &mut usize) -> Result<u32> {
    let digits = bytes
        .get(*i..*i + 4)
        .ok_or_else(|| unterminated(*i))
        .and_then(|digits| Ok(std::str::from_utf8(digits)?))?;
    let code = u32::from_str_radix(digits, 16)
        .map_err(|e| format!("byte {i}: bad \\u escape {digits:?}: {e}"))?;
    *i += 4;
    Ok(code)
}

fn number(bytes: &[u8], i: &mut usize) -> Result<Value> {
    let start = *i;
    while matches!(bytes.get(*i), Some(c) if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E'))
    {
        *i += 1;
    }
    let text = std::str::from_utf8(&bytes[start..*i])?;
    text.parse()
        .map(Value::Number)
        .map_err(|e| format!("byte {start}: bad number {text:?}: {e}").into())
}

fn skip_whitespace(bytes: &[u8], i: &mut usize) {
    while matches!(bytes.get(*i), Some(c) if c.is_ascii_whitespace()) {
        *i += 1;
    }
}

fn unterminated(i: usize) -> Box<dyn Error> {
    format!("byte {i}: text ends mid-value").into()
}

/// Douglas-Peucker over lon/lat as if planar: the map is equirectangular, so
/// degrees are what the error is judged in anyway. Keeps both endpoints.
pub fn simplify(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }

    let last = points.len() - 1;
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[last] = true;

    let mut spans = vec![(0, last)];
    while let Some((start, end)) = spans.pop() {
        let mut worst = start;
        let mut worst_distance = 0.0;
        for (i, point) in points.iter().enumerate().take(end).skip(start + 1) {
            let distance = distance_to_segment(*point, points[start], points[end]);
            if distance > worst_distance {
                worst = i;
                worst_distance = distance;
            }
        }
        if worst_distance > tolerance {
            keep[worst] = true;
            spans.push((start, worst));
            spans.push((worst, end));
        }
    }

    points
        .iter()
        .zip(keep)
        .filter(|(_, keep)| *keep)
        .map(|(point, _)| *point)
        .collect()
}

pub fn distance_to_segment(point: Point, start: Point, end: Point) -> f64 {
    let (dx, dy) = (end.lon - start.lon, end.lat - start.lat);
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (((point.lon - start.lon) * dx + (point.lat - start.lat) * dy) / length_squared)
            .clamp(0.0, 1.0)
    };
    let (nx, ny) = (start.lon + t * dx, start.lat + t * dy);
    ((point.lon - nx).powi(2) + (point.lat - ny).powi(2)).sqrt()
}

/// The ring format both generated files share, to two decimal places.
pub fn render_ring(out: &mut String, ring: &[Point]) {
    for (i, point) in ring.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:.2},{:.2}", point.lon, point.lat));
    }
}

/// The error names the script that downloads the file.
pub fn read(path: &Path, fetch: &str) -> Result<String> {
    fs::read_to_string(path).map_err(|e| {
        format!(
            "{}: {e}\nrun {fetch} from the repo root to download it",
            path.display()
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEATURE: &str = r#"{"type":"FeatureCollection","features":[
{"type":"Feature","properties":{"scalerank":1,"name":"a \"quoted\" é"},"bbox":[0,0,2,2],"geometry":{"type":"Polygon","coordinates":[
[[0.0,0.0],[2.0,0.0],[2.0,2.0],[0.0,2.0],[0.0,0.0]],
[[0.5,0.5],[1.0,0.5],[1.0,1.0],[0.5,0.5]]]}}]}"#;

    const TOLERANCE: f64 = 0.15;

    fn point(lon: f64, lat: f64) -> Point {
        Point { lon, lat }
    }

    fn collection(geometry: &str) -> String {
        format!(r#"{{"features":[{{"properties":{{}},"geometry":{geometry}}}]}}"#)
    }

    #[test]
    fn parses_a_polygon_with_a_hole() {
        let features = features(FEATURE).unwrap();
        assert_eq!(features.len(), 1);
        let polygons = &features[0].polygons;
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].len(), 2);
        assert_eq!(
            polygons[0][0],
            [
                point(0.0, 0.0),
                point(2.0, 0.0),
                point(2.0, 2.0),
                point(0.0, 2.0)
            ]
        );
        assert_eq!(polygons[0][1].len(), 3);
    }

    #[test]
    fn keeps_the_properties() {
        let properties = &features(FEATURE).unwrap()[0].properties;
        assert_eq!(properties.get("scalerank"), Some(&Value::Number(1.0)));
        assert_eq!(
            properties.get("name"),
            Some(&Value::String("a \"quoted\" é".to_string()))
        );
    }

    #[test]
    fn parses_a_multipolygon_as_several_polygons() {
        let text = collection(
            r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]],[[[5,5],[6,5],[6,6],[5,5]]]]}"#,
        );
        let polygons = &features(&text).unwrap()[0].polygons;
        assert_eq!(polygons.len(), 2);
        assert_eq!(polygons[1][0][0], point(5.0, 5.0));
    }

    #[test]
    fn parses_negative_and_exponent_numbers() {
        let text = collection(
            r#"{"type":"Polygon","coordinates":[[[-66.29,-8.1e1],[1,2],[3,4],[-66.29,-81]]]}"#,
        );
        let ring = &features(&text).unwrap()[0].polygons[0][0];
        assert_eq!(ring[0], point(-66.29, -81.0));
        assert_eq!(ring[2], point(3.0, 4.0));
    }

    #[test]
    fn parses_literals_and_surrogate_pairs() {
        assert_eq!(
            parse(r#"[true, false, null, "🌍"]"#).unwrap(),
            Value::Array(vec![
                Value::Bool(true),
                Value::Bool(false),
                Value::Null,
                Value::String("🌍".to_string()),
            ])
        );
    }

    #[test]
    fn rejects_geometries_that_are_not_polygons() {
        let text = collection(r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#);
        let err = features(&text).unwrap_err().to_string();
        assert!(err.contains("got LineString"), "{err}");
    }

    #[test]
    fn rejects_an_unclosed_ring() {
        let text = collection(r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1]]]}"#);
        let err = features(&text).unwrap_err().to_string();
        assert!(err.contains("ring is not closed"), "{err}");
    }

    #[test]
    fn rejects_points_that_are_not_pairs() {
        let text =
            collection(r#"{"type":"Polygon","coordinates":[[[0,0,7],[1,0],[1,1],[0,0,7]]]}"#);
        let err = features(&text).unwrap_err().to_string();
        assert!(err.contains("not a pair of numbers"), "{err}");
    }

    #[test]
    fn rejects_a_truncated_file() {
        let text = r#"{"features":[{"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0]"#;
        let err = features(text).unwrap_err().to_string();
        assert!(err.contains("ends mid-value"), "{err}");
    }

    #[test]
    fn simplify_drops_points_within_the_tolerance() {
        let points = [
            point(0.0, 0.0),
            point(1.0, 0.1),
            point(2.0, -0.1),
            point(3.0, 0.05),
            point(4.0, 0.0),
        ];
        assert_eq!(
            simplify(&points, TOLERANCE),
            [point(0.0, 0.0), point(4.0, 0.0)]
        );
    }

    #[test]
    fn simplify_keeps_a_point_past_the_tolerance() {
        let points = [point(0.0, 0.0), point(1.0, 0.2), point(2.0, 0.0)];
        assert_eq!(simplify(&points, TOLERANCE), points);
    }

    #[test]
    fn simplify_keeps_the_endpoints_of_a_short_ring() {
        let points = [point(0.0, 0.0), point(1.0, 0.0)];
        assert_eq!(simplify(&points, TOLERANCE), points);
    }

    #[test]
    fn simplify_measures_distance_to_the_segment_not_its_line() {
        // The middle point is close to the line through the ends but far past
        // the segment, so a line-distance test would drop it.
        let points = [point(0.0, 0.0), point(10.0, 0.05), point(0.1, 0.0)];
        assert_eq!(simplify(&points, TOLERANCE), points);
    }
}
