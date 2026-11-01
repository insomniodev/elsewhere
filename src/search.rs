//! At ~64k cities a scored linear scan per keystroke is fine. Only the
//! lowercased text is precomputed, so matching allocates nothing per city.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use crate::cities::City;

pub struct SearchIndex {
    entries: Vec<Entry>,
    /// The saved list keys cities by geonameid.
    by_geonameid: HashMap<u32, usize>,
    /// Names shared by several cities, which need an admin1 to tell apart.
    ambiguous: HashSet<String>,
}

struct Entry {
    city: City,
    ascii: String,
    /// `None` when it would equal `ascii`.
    name: Option<String>,
    /// Lowercased and unaccented, for the qualifiers after a comma.
    places: [String; 2],
}

impl SearchIndex {
    pub fn new(cities: Vec<City>) -> Self {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for city in &cities {
            *counts.entry(city.name.as_str()).or_default() += 1;
        }
        let ambiguous = counts
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(name, _)| (*name).to_string())
            .collect();

        let entries: Vec<Entry> = cities
            .into_iter()
            .map(|city| {
                let ascii = city.asciiname.to_lowercase();
                let name = city.name.to_lowercase();
                let name = (name != ascii).then_some(name);
                let places = [fold(&city.admin1), fold(&city.country)];
                Entry {
                    city,
                    ascii,
                    name,
                    places,
                }
            })
            .collect();

        let by_geonameid = entries
            .iter()
            .enumerate()
            .map(|(at, entry)| (entry.city.geonameid, at))
            .collect();

        Self {
            entries,
            by_geonameid,
            ambiguous,
        }
    }

    pub fn cities(&self) -> impl Iterator<Item = &City> {
        self.entries.iter().map(|entry| &entry.city)
    }

    pub fn city(&self, geonameid: u32) -> Option<&City> {
        let at = *self.by_geonameid.get(&geonameid)?;
        Some(&self.entries[at].city)
    }

    /// Case-insensitive, against both the local and the unaccented name, so
    /// "sant julia" and "julià" both find Sant Julià de Lòria. Commas split off
    /// qualifiers, each of which must start the city's region or country, so
    /// "paris, fr" and "Springfield, Missouri" narrow the name before them.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&City> {
        let mut parts = query.split(',');
        let name = parts.next().unwrap_or_default().trim().to_lowercase();
        let qualifiers: Vec<String> = parts
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(fold)
            .collect();
        if name.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut hits: Vec<(Rank, &City)> = self
            .entries
            .iter()
            .filter(|entry| entry.qualified(&qualifiers))
            .filter_map(|entry| Some((entry.rank(&name)?, &entry.city)))
            .collect();
        hits.sort_unstable_by_key(|(rank, city)| (*rank, Reverse(city.population), city.geonameid));
        hits.truncate(limit);
        hits.into_iter().map(|(_, city)| city).collect()
    }

    /// "Name, Country", or "Name, Admin1, Country" when the name is ambiguous.
    pub fn label(&self, city: &City) -> String {
        if self.ambiguous.contains(&city.name) && !city.admin1.is_empty() {
            format!("{}, {}, {}", city.name, city.admin1, city.country)
        } else {
            format!("{}, {}", city.name, city.country)
        }
    }
}

impl Entry {
    /// Each qualifier starts the region or the country.
    fn qualified(&self, qualifiers: &[String]) -> bool {
        qualifiers.iter().all(|qualifier| {
            self.places
                .iter()
                .any(|place| place.starts_with(qualifier.as_str()))
        })
    }

    fn rank(&self, query: &str) -> Option<Rank> {
        [Some(self.ascii.as_str()), self.name.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(|haystack| rank(haystack, query))
            .min()
    }
}

/// Lowercase with the Latin accents the dataset's regions and countries use
/// dropped, so "ile" finds Île-de-France. The dataset keeps no unaccented form
/// of these, as it does of city names.
fn fold(text: &str) -> String {
    const GROUPS: [(&str, char); 14] = [
        ("àáâãäåāăąǎạảấầẩẫậắằẳẵặ", 'a'),
        ("çćĉċč", 'c'),
        ("ďđḑ", 'd'),
        ("èéêëēĕėęěẹẻẽếềểễệ", 'e'),
        ("ğġĝģ", 'g'),
        ("ḩħĥ", 'h'),
        ("ìíîïĩīĭįıị", 'i'),
        ("łĺļľ", 'l'),
        ("ñńņň", 'n'),
        ("òóôõöøōŏőơọỏốồổỗộớờởỡợ", 'o'),
        ("śŝşšș", 's'),
        ("ţťțŧ", 't'),
        ("ùúûüũūŭůűųưụủứừửữự", 'u'),
        ("źżž", 'z'),
    ];
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|char| {
            GROUPS
                .iter()
                .find(|(accented, _)| accented.contains(char))
                .map_or(char, |(_, plain)| *plain)
        })
        .collect()
}

/// Lower is better.
type Rank = u8;

const PREFIX: Rank = 0;
/// A later word starts with the query.
const WORD_PREFIX: Rank = 1;
/// Mid-word.
const SUBSTRING: Rank = 2;

fn rank(haystack: &str, query: &str) -> Option<Rank> {
    let mut best = None;
    for (at, _) in haystack.match_indices(query) {
        if at == 0 {
            return Some(PREFIX);
        }
        let rank = if starts_word(haystack, at) {
            WORD_PREFIX
        } else {
            SUBSTRING
        };
        best = Some(best.map_or(rank, |best: Rank| best.min(rank)));
    }
    best
}

/// `at` is a byte offset; a word begins after any non-alphanumeric char.
fn starts_word(haystack: &str, at: usize) -> bool {
    haystack[..at]
        .chars()
        .next_back()
        .is_some_and(|char| !char.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities;

    fn city(geonameid: u32, name: &str, asciiname: &str, admin1: &str, population: u64) -> City {
        City {
            geonameid,
            name: name.to_string(),
            asciiname: if asciiname.is_empty() {
                name.to_string()
            } else {
                asciiname.to_string()
            },
            country: "Testland".to_string(),
            admin1: admin1.to_string(),
            population,
            timezone: "UTC".to_string(),
            latitude: 0.0,
            longitude: 0.0,
        }
    }

    fn index() -> SearchIndex {
        SearchIndex::new(vec![
            city(1, "Springfield", "", "Illinois", 116_000),
            city(2, "Springfield", "", "Missouri", 169_000),
            city(3, "Springfield", "", "", 60_000),
            city(4, "West Springfield", "", "Massachusetts", 28_000),
            city(5, "Kaiserspringfield", "", "Bavaria", 500_000),
            city(
                6,
                "Sant Julià de Lòria",
                "Sant Julia de Loria",
                "Andorra",
                8_022,
            ),
            city(7, "Quito", "", "Pichincha", 1_399_000),
        ])
    }

    fn names(cities: &[&City]) -> Vec<String> {
        cities.iter().map(|city| city.name.clone()).collect()
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        let index = index();
        assert!(index.search("", 10).is_empty());
        assert!(index.search("   ", 10).is_empty());
    }

    #[test]
    fn matching_ignores_case_and_surrounding_space() {
        let index = index();
        assert_eq!(names(&index.search("  QUITO ", 10)), ["Quito"]);
    }

    #[test]
    fn a_limit_truncates_the_results() {
        let index = index();
        assert_eq!(index.search("springfield", 2).len(), 2);
        assert!(index.search("springfield", 0).is_empty());
    }

    #[test]
    fn whole_name_prefixes_beat_word_prefixes_beat_substrings() {
        let index = index();
        assert_eq!(
            names(&index.search("springfield", 10)),
            [
                // Prefixes of the whole name, largest first.
                "Springfield",
                "Springfield",
                "Springfield",
                // Then a later word, then mid-word, however big.
                "West Springfield",
                "Kaiserspringfield",
            ]
        );
    }

    #[test]
    fn equal_ranks_are_ordered_by_population_then_geonameid() {
        let index = index();
        let ids: Vec<u32> = index
            .search("springfield", 3)
            .iter()
            .map(|city| city.geonameid)
            .collect();
        assert_eq!(ids, [2, 1, 3]);
    }

    #[test]
    fn accents_are_searchable_either_way() {
        let index = index();
        assert_eq!(
            names(&index.search("sant julia", 10)),
            ["Sant Julià de Lòria"]
        );
        assert_eq!(names(&index.search("julià", 10)), ["Sant Julià de Lòria"]);
        assert_eq!(
            names(&index.search("JULIA DE", 10)),
            ["Sant Julià de Lòria"]
        );
    }

    #[test]
    fn ambiguous_names_are_labelled_with_their_region() {
        let index = index();
        let labelled: Vec<String> = index
            .search("springfield", 3)
            .iter()
            .map(|city| index.label(city))
            .collect();
        assert_eq!(
            labelled,
            [
                "Springfield, Missouri, Testland",
                "Springfield, Illinois, Testland",
                // Ambiguous but regionless: nothing to add.
                "Springfield, Testland",
            ]
        );
    }

    #[test]
    fn unique_names_are_labelled_without_their_region() {
        let index = index();
        let quito = index.search("quito", 1);
        assert_eq!(index.label(quito[0]), "Quito, Testland");
    }

    #[test]
    fn cities_are_looked_up_by_geonameid() {
        let index = index();
        assert_eq!(index.city(7).map(|city| city.name.as_str()), Some("Quito"));
        assert_eq!(index.city(404), None);
    }

    #[test]
    fn a_qualifier_narrows_by_region_or_country() {
        let index = index();
        let ids = |query| -> Vec<u32> {
            index
                .search(query, 10)
                .iter()
                .map(|city| city.geonameid)
                .collect()
        };
        assert_eq!(ids("springfield, missouri"), [2]);
        assert_eq!(ids("Springfield , MISS"), [2]);
        // The country is the same for all of them.
        assert_eq!(ids("springfield, test"), [2, 1, 3, 4, 5]);
        assert_eq!(ids("springfield, testland, illinois"), [1]);
        assert!(ids("springfield, france").is_empty());
        // A region must start with it, not merely contain it.
        assert!(ids("springfield, souri").is_empty());
    }

    #[test]
    fn an_empty_qualifier_is_ignored_and_a_bare_one_matches_nothing() {
        let index = index();
        assert_eq!(index.search("quito,", 10).len(), 1);
        assert_eq!(index.search("quito, ,  ", 10).len(), 1);
        assert!(index.search(", testland", 10).is_empty());
    }

    #[test]
    fn qualifiers_ignore_accents() {
        let index = SearchIndex::new(vec![
            city(1, "Paris", "", "Île-de-France", 2_000_000),
            city(2, "Paris", "", "Texas", 25_000),
        ]);
        assert_eq!(index.search("paris, ile", 10).len(), 1);
        assert_eq!(index.search("paris, Île-de", 10)[0].geonameid, 1);
    }

    #[test]
    fn qualifiers_keep_the_ranking() {
        let index = index();
        let ids: Vec<u32> = index
            .search("springfield, testland", 10)
            .iter()
            .map(|city| city.geonameid)
            .collect();
        let plain: Vec<u32> = index
            .search("springfield", 10)
            .iter()
            .map(|city| city.geonameid)
            .collect();
        assert_eq!(ids, plain);
    }

    #[test]
    fn real_places_qualify_by_country_and_region() {
        let index = SearchIndex::new(cities::load());
        let found = index.search("Paris, France", 5);
        assert_eq!(found[0].name, "Paris");
        assert_eq!(found[0].country, "France");
        assert!(found.iter().all(|city| city.country == "France"));

        let found = index.search("paris, fr", 5);
        assert_eq!(found[0].country, "France");

        let found = index.search("Springfield, Missouri", 5);
        assert_eq!(found[0].name, "Springfield");
        assert_eq!(found[0].admin1, "Missouri");
        assert!(found.iter().all(|city| city.admin1 == "Missouri"));
    }

    #[test]
    fn finds_tokyo_in_the_real_dataset() {
        let index = SearchIndex::new(cities::load());
        let found = index.search("tokyo", 5);
        assert_eq!(found[0].name, "Tokyo");
        assert_eq!(found[0].timezone, "Asia/Tokyo");
    }

    #[test]
    fn prefers_the_largest_san_antonio() {
        let index = SearchIndex::new(cities::load());
        let found = index.search("san antonio", 5);
        assert_eq!(found[0].name, "San Antonio");
        assert_eq!(found[0].admin1, "Texas");
        assert_eq!(index.label(found[0]), "San Antonio, Texas, United States");
    }

    #[test]
    fn ranks_york_above_new_york() {
        let index = SearchIndex::new(cities::load());
        let found = index.search("york", 20);
        assert_eq!(found[0].name, "York");
        assert_eq!(found[0].country, "United Kingdom");

        let york = found
            .iter()
            .position(|city| city.name == "York")
            .expect("York is in the results");
        let new_york = found
            .iter()
            .position(|city| city.name == "New York City")
            .expect("New York City is in the results");
        assert!(york < new_york, "{:?}", names(&found));

        let found = index.search("new york", 5);
        assert_eq!(found[0].name, "New York City");
    }
}
