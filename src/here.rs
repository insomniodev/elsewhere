//! "Here": this machine's zone, what every view calls it and where the map
//! marks it. One model, so the list, map, grid and banner cannot disagree.
//! Kept gpui-free so it can be tested.

use jiff::tz::TimeZone;

use crate::cities::City;

pub struct Here {
    pub zone: TimeZone,
    /// The first saved city in the zone, in the saved order. It is here's
    /// face, so no view shows here twice.
    pub city: Option<u32>,
    /// The face's name, else the zone's own.
    pub name: String,
    /// Where the map marks here when no saved city is in the zone: see `seat`.
    pub seat: Option<u32>,
}

impl Here {
    /// `saved` in the saved order. `seat` is passed in, since finding it
    /// scans the whole dataset and the zone rarely changes.
    pub fn new<'a>(
        zone: TimeZone,
        saved: impl IntoIterator<Item = &'a City>,
        seat: Option<u32>,
    ) -> Here {
        let face = sharing(zone.iana_name(), saved);
        let name = match (face, zone.iana_name()) {
            (Some(city), _) => city.name.clone(),
            (None, Some(iana)) => zone_name(iana),
            // A bare offset or a POSIX rule has no place to name.
            (None, None) => "local time".to_string(),
        };
        Here {
            city: face.map(|city| city.geonameid),
            zone,
            name,
            seat,
        }
    }

    /// Where the map marks here: the face, else the seat.
    pub fn place(&self) -> Option<u32> {
        self.city.or(self.seat)
    }
}

/// "Guayaquil" for "America/Guayaquil", "Buenos Aires" for
/// "America/Argentina/Buenos_Aires": the part a person would say.
pub fn zone_name(iana: &str) -> String {
    iana.rsplit('/').next().unwrap_or(iana).replace('_', " ")
}

/// The first of `saved` whose zone is `iana`. By name, as the dataset stores
/// zones; a zone with no name shares with nothing.
pub fn sharing<'a>(
    iana: Option<&str>,
    saved: impl IntoIterator<Item = &'a City>,
) -> Option<&'a City> {
    let iana = iana?;
    saved.into_iter().find(|city| city.timezone == iana)
}

/// The city the zone is named for, else its most populous: tzdb names a zone
/// after a city in it, so the marker and its name ("Guayaquil") agree, where
/// the most populous (Quito) would put one city's name on another's dot.
/// `None` when no city in the dataset keeps the zone, and the map then has
/// nowhere to mark here.
pub fn seat<'a>(iana: &str, cities: impl IntoIterator<Item = &'a City>) -> Option<u32> {
    let name = zone_name(iana);
    cities
        .into_iter()
        .filter(|city| city.timezone == iana)
        .max_by_key(|city| (city.asciiname == name, city.population))
        .map(|city| city.geonameid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn city(geonameid: u32, name: &str, timezone: &str, population: u64) -> City {
        City {
            geonameid,
            name: name.to_string(),
            asciiname: name.to_string(),
            country: String::new(),
            admin1: String::new(),
            population,
            timezone: timezone.to_string(),
            latitude: 0.0,
            longitude: 0.0,
        }
    }

    fn zone(name: &str) -> TimeZone {
        TimeZone::get(name).expect("a zone this machine knows")
    }

    #[test]
    fn names_a_zone_by_its_city_part() {
        assert_eq!(zone_name("America/Guayaquil"), "Guayaquil");
        assert_eq!(zone_name("America/New_York"), "New York");
        assert_eq!(zone_name("America/Argentina/Buenos_Aires"), "Buenos Aires");
        assert_eq!(zone_name("UTC"), "UTC");
    }

    #[test]
    fn a_saved_city_in_the_zone_is_here() {
        let saved = [
            city(2643743, "London", "Europe/London", 8_961_989),
            city(3652462, "Quito", "America/Guayaquil", 2_781_641),
            city(3657509, "Guayaquil", "America/Guayaquil", 2_723_665),
        ];
        let found = sharing(Some("America/Guayaquil"), &saved).map(|city| city.geonameid);
        // The first in the saved order, not the most populous.
        assert_eq!(found, Some(3652462));

        let here = Here::new(zone("America/Guayaquil"), &saved, None);
        assert_eq!(here.city, Some(3652462));
        assert_eq!(here.name, "Quito");
        assert_eq!(here.place(), Some(3652462));
    }

    #[test]
    fn with_no_saved_city_in_the_zone_here_is_named_by_the_zone() {
        let saved = [city(2643743, "London", "Europe/London", 8_961_989)];
        assert!(sharing(Some("America/Guayaquil"), &saved).is_none());

        let here = Here::new(zone("America/Guayaquil"), &saved, Some(3657509));
        assert_eq!(here.city, None);
        assert_eq!(here.name, "Guayaquil");
        assert_eq!(here.place(), Some(3657509));
    }

    #[test]
    fn a_zone_with_no_name_shares_with_nothing() {
        let saved = [city(1, "Somewhere", "Etc/GMT+5", 1)];
        assert!(sharing(None, &saved).is_none());
        let here = Here::new(TimeZone::fixed(jiff::tz::offset(-5)), &saved, None);
        assert_eq!(here.city, None);
        assert_eq!(here.name, "local time");
    }

    #[test]
    fn the_seat_is_the_city_the_zone_is_named_for() {
        let cities = [
            city(3652462, "Quito", "America/Guayaquil", 2_781_641),
            city(3657509, "Guayaquil", "America/Guayaquil", 2_723_665),
            city(3688689, "Bogotá", "America/Bogota", 7_674_366),
        ];
        assert_eq!(seat("America/Guayaquil", &cities), Some(3657509));
        assert_eq!(seat("Antarctica/Troll", &cities), None);
    }

    #[test]
    fn without_a_namesake_the_seat_is_the_most_populous() {
        let cities = [
            city(1275339, "Mumbai", "Asia/Kolkata", 12_691_836),
            city(1273294, "Delhi", "Asia/Kolkata", 10_927_986),
        ];
        assert_eq!(seat("Asia/Kolkata", &cities), Some(1275339));
    }
}
