//! At ~64k cities a scored linear scan per keystroke is fine. Only the
//! lowercased text is precomputed, so matching allocates nothing.

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
                Entry { city, ascii, name }
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

    #[allow(dead_code)] // no caller yet; the UI looks cities up one at a time
    pub fn cities(&self) -> impl Iterator<Item = &City> {
        self.entries.iter().map(|entry| &entry.city)
    }

    pub fn city(&self, geonameid: u32) -> Option<&City> {
        let at = *self.by_geonameid.get(&geonameid)?;
        Some(&self.entries[at].city)
    }

    /// Case-insensitive, against both the local and the unaccented name, so
    /// "sant julia" and "julià" both find Sant Julià de Lòria.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&City> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut hits: Vec<(Rank, &City)> = self
            .entries
            .iter()
            .filter_map(|entry| Some((entry.rank(&query)?, &entry.city)))
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
    fn rank(&self, query: &str) -> Option<Rank> {
        [Some(self.ascii.as_str()), self.name.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(|haystack| rank(haystack, query))
            .min()
    }
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
