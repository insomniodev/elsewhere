//! Filters the GeoNames dumps in data/raw/ down to places worth putting on a
//! clock, written to data/cities.tsv.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::path::Path;
use std::str::FromStr;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const RAW_DIR: &str = "data/raw";
const OUT_PATH: &str = "data/cities.tsv";
const CITY_COLUMNS: usize = 19;
const COUNTRY_COLUMNS: usize = 19;
const ADMIN1_COLUMNS: usize = 4;

/// Real, currently populated places. Notably dropped: PPLX (sections of a
/// city), PPLH (historical), PPLQ (abandoned), PPLW (destroyed), PPLR, PPLCH
/// and STLMT.
const KEPT_FEATURE_CODES: &[&str] = &[
    "PPL", "PPLA", "PPLA2", "PPLA3", "PPLA4", "PPLA5", "PPLC", "PPLF", "PPLG", "PPLL", "PPLS",
];

/// How many offending rows an error message names.
const MAX_REPORTED_ROWS: usize = 5;

const OUT_HEADER: &str =
    "#geonameid\tname\tasciiname\tcountry\tadmin1\tpopulation\ttimezone\tlatitude\tlongitude";

/// The columns of cities5000.txt we keep.
#[allow(dead_code)] // alternatenames and feature_class are not emitted
#[derive(Debug)]
struct RawCity {
    geonameid: u32,
    name: String,
    asciiname: String,
    alternatenames: Vec<String>,
    latitude: f64,
    longitude: f64,
    feature_class: char,
    feature_code: String,
    country_code: String,
    admin1_code: String,
    population: u64,
    timezone: String,
}

/// A row of data/cities.tsv, codes resolved to names.
#[derive(Debug)]
struct Row {
    geonameid: u32,
    name: String,
    asciiname: String,
    country: String,
    admin1: String,
    population: u64,
    timezone: String,
    latitude: f64,
    longitude: f64,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let raw = Path::new(RAW_DIR);

    let cities = parse_cities(&read(&raw.join("cities5000.txt"))?)?;
    let countries = parse_countries(&read(&raw.join("countryInfo.txt"))?)?;
    let admin1 = parse_admin1(&read(&raw.join("admin1CodesASCII.txt"))?)?;

    let parsed = cities.len();
    let dropped = dropped_per_feature(&cities);
    let cities = keep_by_feature(cities);
    let filtered = cities.len();
    let cities = dedup(cities);

    let timezones = validate_timezones(&cities)?;

    let rows = resolve(cities, &countries, &admin1)?;
    let empty_admin1 = rows.iter().filter(|row| row.admin1.is_empty()).count();
    fs::write(OUT_PATH, render(&rows))?;

    println!("parsed rows        {parsed}");
    println!("dropped by feature {}", parsed - filtered);
    println!("collapsed by dedup {}", filtered - rows.len());
    println!("kept rows          {}", rows.len());
    println!("\ndropped per feature code:");
    for (code, count) in dropped {
        println!("  {code:<6} {count:>5}");
    }
    println!("\nlookup tables:");
    println!("  countries {}", countries.len());
    println!("  admin1    {}", admin1.len());
    println!("\nkept rows span:");
    println!("  timezones (validated against jiff) {timezones}");
    println!("  rows without an admin1 name        {empty_admin1}");
    println!("\nemitted {} rows to {OUT_PATH}", rows.len());

    Ok(())
}

/// An asciiname that repeats name is blanked, as the loader falls back to
/// name. An unknown admin1 code becomes empty, since plenty of places have no
/// useful subdivision; an unknown country code is a bug in the dumps and
/// fails the run.
fn resolve(
    cities: Vec<RawCity>,
    countries: &HashMap<String, String>,
    admin1: &HashMap<String, String>,
) -> Result<Vec<Row>> {
    let mut unknown_countries: HashSet<&str> = HashSet::new();
    for city in &cities {
        if !countries.contains_key(&city.country_code) {
            unknown_countries.insert(&city.country_code);
        }
    }
    if !unknown_countries.is_empty() {
        let mut names: Vec<&str> = unknown_countries.into_iter().collect();
        names.sort_unstable();
        return Err(format!("unknown country codes: {}", names.join(", ")).into());
    }

    Ok(cities
        .into_iter()
        .map(|city| Row {
            geonameid: city.geonameid,
            country: countries[&city.country_code].clone(),
            admin1: admin1
                .get(&format!("{}.{}", city.country_code, city.admin1_code))
                .cloned()
                .unwrap_or_default(),
            asciiname: if city.asciiname == city.name {
                String::new()
            } else {
                city.asciiname
            },
            name: city.name,
            population: city.population,
            timezone: city.timezone,
            latitude: city.latitude,
            longitude: city.longitude,
        })
        .collect())
}

/// Nothing needs escaping: every value came out of a tab-separated dump.
fn render(rows: &[Row]) -> String {
    let mut out = String::from(OUT_HEADER);
    for row in rows {
        out.push('\n');
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.geonameid,
            row.name,
            row.asciiname,
            row.country,
            row.admin1,
            row.population,
            row.timezone,
            row.latitude,
            row.longitude
        ));
    }
    out.push('\n');
    out
}

/// Returns how many distinct zones the rows name. A zone jiff cannot resolve
/// means the dump and this machine's tzdb disagree, so the run fails rather
/// than emit names the app would choke on.
fn validate_timezones(cities: &[RawCity]) -> Result<usize> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut missing: Vec<&RawCity> = Vec::new();
    for city in cities {
        if city.timezone.is_empty() {
            missing.push(city);
        } else {
            *counts.entry(&city.timezone).or_default() += 1;
        }
    }

    if !missing.is_empty() {
        let sample: Vec<String> = missing
            .iter()
            .take(MAX_REPORTED_ROWS)
            .map(|city| format!("{} {}", city.geonameid, city.name))
            .collect();
        return Err(format!(
            "{} rows have an empty timezone: {}",
            missing.len(),
            sample.join(", ")
        )
        .into());
    }

    let mut unknown: Vec<(&str, usize)> = counts
        .iter()
        .filter(|(name, _)| jiff::tz::TimeZone::get(name).is_err())
        .map(|(name, count)| (*name, *count))
        .collect();
    if !unknown.is_empty() {
        unknown.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let list: Vec<String> = unknown
            .iter()
            .map(|(name, count)| format!("  {name} ({count} rows)"))
            .collect();
        return Err(format!(
            "{} timezones cannot be resolved by jiff:\n{}",
            unknown.len(),
            list.join("\n")
        )
        .into());
    }

    Ok(counts.len())
}

/// Most common first.
fn dropped_per_feature(cities: &[RawCity]) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for city in cities {
        if !is_kept(&city.feature_code) {
            *counts.entry(&city.feature_code).or_default() += 1;
        }
    }
    let mut counts: Vec<_> = counts
        .into_iter()
        .map(|(code, count)| (code.to_string(), count))
        .collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts
}

fn is_kept(feature_code: &str) -> bool {
    KEPT_FEATURE_CODES.contains(&feature_code)
}

fn keep_by_feature(cities: Vec<RawCity>) -> Vec<RawCity> {
    cities
        .into_iter()
        .filter(|city| is_kept(&city.feature_code))
        .collect()
}

/// Keeps the most populous row per (name, country, admin1), ties to the lower
/// geonameid. Sorted by geonameid so a run is reproducible.
fn dedup(cities: Vec<RawCity>) -> Vec<RawCity> {
    let mut best: HashMap<(String, String, String), RawCity> = HashMap::new();
    for city in cities {
        let key = (
            city.name.clone(),
            city.country_code.clone(),
            city.admin1_code.clone(),
        );
        match best.entry(key) {
            Entry::Occupied(mut kept) => {
                if beats(&city, kept.get()) {
                    kept.insert(city);
                }
            }
            Entry::Vacant(slot) => {
                slot.insert(city);
            }
        }
    }

    let mut cities: Vec<RawCity> = best.into_values().collect();
    cities.sort_by_key(|city| city.geonameid);
    cities
}

fn beats(city: &RawCity, kept: &RawCity) -> bool {
    city.population > kept.population
        || (city.population == kept.population && city.geonameid < kept.geonameid)
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| {
        format!(
            "{}: {e}\nrun scripts/fetch-geonames.sh from the repo root to download the dumps",
            path.display()
        )
        .into()
    })
}

fn parse_cities(text: &str) -> Result<Vec<RawCity>> {
    text.lines()
        .enumerate()
        .map(|(i, line)| parse_city(line, i + 1))
        .collect()
}

fn parse_city(line: &str, lineno: usize) -> Result<RawCity> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() != CITY_COLUMNS {
        return Err(columns_error(lineno, CITY_COLUMNS, cols.len()));
    }
    Ok(RawCity {
        geonameid: field(cols[0], lineno, "geonameid")?,
        name: cols[1].to_string(),
        asciiname: cols[2].to_string(),
        alternatenames: cols[3]
            .split(',')
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect(),
        latitude: field(cols[4], lineno, "latitude")?,
        longitude: field(cols[5], lineno, "longitude")?,
        feature_class: field(cols[6], lineno, "feature class")?,
        feature_code: cols[7].to_string(),
        country_code: cols[8].to_string(),
        admin1_code: cols[10].to_string(),
        population: field(cols[14], lineno, "population")?,
        timezone: cols[17].to_string(),
    })
}

/// country code -> country name
fn parse_countries(text: &str) -> Result<HashMap<String, String>> {
    let mut countries = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != COUNTRY_COLUMNS {
            return Err(columns_error(i + 1, COUNTRY_COLUMNS, cols.len()));
        }
        countries.insert(cols[0].to_string(), cols[4].to_string());
    }
    Ok(countries)
}

/// "CC.ADM1" -> admin1 name
fn parse_admin1(text: &str) -> Result<HashMap<String, String>> {
    let mut admin1 = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() != ADMIN1_COLUMNS {
            return Err(columns_error(i + 1, ADMIN1_COLUMNS, cols.len()));
        }
        admin1.insert(cols[0].to_string(), cols[1].to_string());
    }
    Ok(admin1)
}

fn field<T: FromStr>(value: &str, lineno: usize, name: &str) -> Result<T>
where
    T::Err: Display,
{
    value
        .parse()
        .map_err(|e| format!("line {lineno}: bad {name} {value:?}: {e}").into())
}

fn columns_error(lineno: usize, expected: usize, got: usize) -> Box<dyn Error> {
    format!("line {lineno}: expected {expected} columns, got {got}").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CITIES: &str = "\
1850147\tTokyo\tTokyo\tEdo,TYO,Tokio,Tokió,東京,도쿄\t35.6895\t139.69171\tP\tPPLC\tJP\t\t40\t\t\t\t9733276\t\t44\tAsia/Tokyo\t2025-07-22
3039163\tSant Julià de Lòria\tSant Julia de Loria\tSan Julià,Sant Julia de Loria\t42.46372\t1.49129\tP\tPPLA\tAD\t\t06\t\t\t\t8022\t\t921\tEurope/Andorra\t2018-10-14
9179507\tMalmok\tMalmok\t\t12.60087\t-70.05064\tP\tPPL\tAW\t\t\t\t\t\t5637\t\t9\tAmerica/Aruba\t2026-06-24";

    const COUNTRIES: &str = "\
# comment line
#ISO\tISO3\tISO-Numeric\tfips\tCountry\tCapital\tArea(in sq km)\tPopulation\tContinent\ttld\tCurrencyCode\tCurrencyName\tPhone\tPostal Code Format\tPostal Code Regex\tLanguages\tgeonameid\tneighbours\tEquivalentFipsCode
AD\tAND\t020\tAN\tAndorra\tAndorra la Vella\t468\t77006\tEU\t.ad\tEUR\tEuro\t376\tAD###\t^(?:AD)*(\\d{3})$\tca\t3041565\tES,FR\t
AW\tABW\t533\tAA\tAruba\tOranjestad\t193\t106766\tNA\t.aw\tAWG\tGuilder\t297\t\t\tnl-AW,pap,es,en\t3577279\t\t
JP\tJPN\t392\tJA\tJapan\tTokyo\t377835\t126529100\tAS\t.jp\tJPY\tYen\t81\t###-####\t^\\d{3}-\\d{4}$\tja\t1861060\t\t";

    const ADMIN1: &str = "\
AD.06\tSant Julià de Lòria\tSant Julia de Loria\t3039162
JP.40\tTokyo\tTokyo\t1850144";

    #[test]
    fn parses_cities() {
        let cities = parse_cities(CITIES).unwrap();
        assert_eq!(cities.len(), 3);

        let tokyo = &cities[0];
        assert_eq!(tokyo.geonameid, 1850147);
        assert_eq!(tokyo.feature_class, 'P');
        assert_eq!(tokyo.feature_code, "PPLC");
        assert_eq!(tokyo.population, 9733276);
        assert_eq!(tokyo.timezone, "Asia/Tokyo");
        assert_eq!(tokyo.alternatenames.len(), 6);
        assert_eq!(tokyo.alternatenames[5], "도쿄");
        assert_eq!(tokyo.latitude, 35.6895);
        assert_eq!(tokyo.longitude, 139.69171);
    }

    #[test]
    fn keeps_non_ascii_names() {
        let city = &parse_cities(CITIES).unwrap()[1];
        assert_eq!(city.name, "Sant Julià de Lòria");
        assert_eq!(city.asciiname, "Sant Julia de Loria");
        assert_eq!(city.admin1_code, "06");
    }

    #[test]
    fn handles_empty_fields() {
        let city = &parse_cities(CITIES).unwrap()[2];
        assert!(city.admin1_code.is_empty());
        assert!(city.alternatenames.is_empty());
        assert_eq!(city.longitude, -70.05064);
    }

    #[test]
    fn rejects_wrong_column_count() {
        let err = parse_cities("1850147\tTokyo\tTokyo")
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 1"), "{err}");
        assert!(err.contains("expected 19 columns, got 3"), "{err}");
    }

    #[test]
    fn rejects_unparseable_numbers() {
        let broken = CITIES.replace("9733276", "many");
        let err = parse_cities(&broken).unwrap_err().to_string();
        assert!(err.contains("line 1"), "{err}");
        assert!(err.contains("population"), "{err}");
    }

    #[test]
    fn parses_countries_skipping_comments() {
        let countries = parse_countries(COUNTRIES).unwrap();
        assert_eq!(countries.len(), 3);
        assert_eq!(countries["JP"], "Japan");
        assert_eq!(countries["AD"], "Andorra");
    }

    #[test]
    fn parses_admin1() {
        let admin1 = parse_admin1(ADMIN1).unwrap();
        assert_eq!(admin1["AD.06"], "Sant Julià de Lòria");
        assert_eq!(admin1["JP.40"], "Tokyo");
    }

    fn city(
        geonameid: u32,
        name: &str,
        country_code: &str,
        admin1_code: &str,
        feature_code: &str,
        population: u64,
    ) -> RawCity {
        RawCity {
            geonameid,
            name: name.to_string(),
            asciiname: name.to_string(),
            alternatenames: Vec::new(),
            latitude: 0.0,
            longitude: 0.0,
            feature_class: 'P',
            feature_code: feature_code.to_string(),
            country_code: country_code.to_string(),
            admin1_code: admin1_code.to_string(),
            population,
            timezone: "UTC".to_string(),
        }
    }

    fn city_in(geonameid: u32, name: &str, timezone: &str) -> RawCity {
        let mut city = city(geonameid, name, "US", "IL", "PPL", 5000);
        city.timezone = timezone.to_string();
        city
    }

    fn ids(cities: &[RawCity]) -> Vec<u32> {
        cities.iter().map(|city| city.geonameid).collect()
    }

    #[test]
    fn keeps_only_allowlisted_feature_codes() {
        let cities = vec![
            city(1, "London", "GB", "ENG", "PPLC", 8961989),
            city(2, "Camden Town", "GB", "ENG", "PPLX", 50000),
            city(3, "Ruins", "GB", "ENG", "PPLQ", 0),
            city(4, "Farm", "GB", "ENG", "PPLF", 5000),
            city(5, "Old Town", "GB", "ENG", "PPLH", 0),
            city(6, "Camp", "GB", "ENG", "STLMT", 5000),
        ];
        assert_eq!(ids(&keep_by_feature(cities)), [1, 4]);
    }

    #[test]
    fn counts_dropped_feature_codes() {
        let cities = vec![
            city(1, "A", "GB", "ENG", "PPL", 5000),
            city(2, "B", "GB", "ENG", "PPLX", 5000),
            city(3, "C", "GB", "ENG", "PPLX", 5000),
            city(4, "D", "GB", "ENG", "PPLH", 5000),
        ];
        assert_eq!(
            dropped_per_feature(&cities),
            [("PPLX".to_string(), 2), ("PPLH".to_string(), 1)]
        );
    }

    #[test]
    fn dedup_keeps_the_most_populous_row() {
        let cities = vec![
            city(10, "Springfield", "US", "IL", "PPL", 1000),
            city(20, "Springfield", "US", "IL", "PPLA2", 116250),
            city(30, "Springfield", "US", "IL", "PPL", 500),
        ];
        assert_eq!(ids(&dedup(cities)), [20]);
    }

    #[test]
    fn dedup_is_deterministic() {
        let rows = || {
            vec![
                city(30, "Twin", "US", "IL", "PPL", 5000),
                city(10, "Twin", "US", "IL", "PPL", 5000),
                city(20, "Twin", "US", "IL", "PPL", 5000),
            ]
        };
        let mut reversed = rows();
        reversed.reverse();

        // Equal populations tie-break on the lower geonameid, whatever the order.
        assert_eq!(ids(&dedup(rows())), [10]);
        assert_eq!(ids(&dedup(reversed)), [10]);
    }

    #[test]
    fn validates_real_timezones() {
        let cities = vec![
            city_in(1, "Tokyo", "Asia/Tokyo"),
            city_in(2, "Buenos Aires", "America/Argentina/Buenos_Aires"),
            city_in(3, "Yokohama", "Asia/Tokyo"),
        ];
        assert_eq!(validate_timezones(&cities).unwrap(), 2);
    }

    #[test]
    fn rejects_unknown_timezones() {
        let cities = vec![
            city_in(1, "Tokyo", "Asia/Tokyo"),
            city_in(2, "Olympus", "Mars/Olympus_Mons"),
            city_in(3, "Olympus Base", "Mars/Olympus_Mons"),
        ];
        let err = validate_timezones(&cities).unwrap_err().to_string();
        assert!(err.contains("Mars/Olympus_Mons (2 rows)"), "{err}");
    }

    #[test]
    fn rejects_empty_timezones() {
        let cities = vec![city_in(1, "Tokyo", "Asia/Tokyo"), city_in(2, "Nowhere", "")];
        let err = validate_timezones(&cities).unwrap_err().to_string();
        assert!(err.contains("1 rows have an empty timezone"), "{err}");
        assert!(err.contains("2 Nowhere"), "{err}");
    }

    fn lookups() -> (HashMap<String, String>, HashMap<String, String>) {
        (
            parse_countries(COUNTRIES).unwrap(),
            parse_admin1(ADMIN1).unwrap(),
        )
    }

    #[test]
    fn resolves_codes_to_names() {
        let (countries, admin1) = lookups();
        let rows = resolve(parse_cities(CITIES).unwrap(), &countries, &admin1).unwrap();

        assert_eq!(rows[0].country, "Japan");
        assert_eq!(rows[0].admin1, "Tokyo");
        assert_eq!(rows[1].name, "Sant Julià de Lòria");
        assert_eq!(rows[1].asciiname, "Sant Julia de Loria");
        assert_eq!(rows[1].admin1, "Sant Julià de Lòria");
        assert_eq!(rows[0].latitude, 35.6895);
        assert_eq!(rows[0].longitude, 139.69171);
    }

    #[test]
    fn resolve_blanks_a_redundant_asciiname_and_a_missing_admin1() {
        let (countries, admin1) = lookups();
        let rows = resolve(parse_cities(CITIES).unwrap(), &countries, &admin1).unwrap();

        assert_eq!(rows[0].name, "Tokyo");
        assert!(rows[0].asciiname.is_empty());
        // Malmok has no admin1 code at all.
        assert!(rows[2].admin1.is_empty());
    }

    #[test]
    fn resolve_rejects_unknown_country_codes() {
        let (countries, admin1) = lookups();
        let cities = vec![city(1, "Olympus", "MA", "01", "PPL", 5000)];
        let err = resolve(cities, &countries, &admin1)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown country codes: MA"), "{err}");
    }

    #[test]
    fn renders_a_header_and_one_line_per_row() {
        let (countries, admin1) = lookups();
        let rows = resolve(parse_cities(CITIES).unwrap(), &countries, &admin1).unwrap();
        let out = render(&rows);

        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with('#'));
        assert_eq!(
            lines[1],
            "1850147\tTokyo\t\tJapan\tTokyo\t9733276\tAsia/Tokyo\t35.6895\t139.69171"
        );
        assert_eq!(
            lines[3],
            "9179507\tMalmok\t\tAruba\t\t5637\tAmerica/Aruba\t12.60087\t-70.05064"
        );
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn dedup_keeps_distinct_places_with_the_same_name() {
        let cities = vec![
            city(4726206, "San Antonio", "US", "TX", "PPLA2", 1526656),
            city(4568074, "San Antonio", "PR", "051", "PPL", 6456),
            city(3872395, "San Antonio", "CL", "01", "PPL", 87675),
            city(3670107, "San Antonio", "CO", "38", "PPL", 8476),
            city(3670162, "San Antonio", "CO", "28", "PPLA2", 5185),
        ];
        assert_eq!(
            ids(&dedup(cities)),
            [3670107, 3670162, 3872395, 4568074, 4726206]
        );
    }
}
