//! The sun's position and the night side on a flat map. Degrees in and out,
//! kept gpui-free so it can be tested.

use jiff::Zoned;

/// Degrees east and north, negative west and south.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LonLat {
    pub lon: f64,
    pub lat: f64,
}

/// The map's crop: a whole equirectangular earth is mostly ice at both ends,
/// and cutting it buys height for everywhere else.
pub const NORTH: f64 = 84.0;
pub const SOUTH: f64 = -56.0;

pub fn aspect() -> f64 {
    360.0 / (NORTH - SOUTH)
}

/// `(x, y)` as fractions of the cropped map, y down. Latitudes outside the
/// crop clamp to its edge.
pub fn project(p: LonLat) -> (f64, f64) {
    let x = (p.lon + 180.0) / 360.0;
    let y = (NORTH - p.lat) / (NORTH - SOUTH);
    (x, y.clamp(0.0, 1.0))
}

/// Where the sun is directly overhead.
///
/// Meeus, *Astronomical Algorithms* ch. 25 (low-precision solar position) and
/// eq. 28.3 (the equation of time): about an arcminute, far below a pixel.
pub fn subsolar(at: &Zoned) -> LonLat {
    let timestamp = at.timestamp();
    let unix = timestamp.as_second() as f64 + f64::from(timestamp.subsec_nanosecond()) / 1e9;
    let julian_day = unix / 86_400.0 + 2_440_587.5;
    let t = (julian_day - 2_451_545.0) / 36_525.0;

    let mean_longitude = 280.46646 + t * (36000.76983 + t * 0.0003032);
    let mean_anomaly = (357.52911 + t * (35999.05029 - t * 0.0001537)).to_radians();
    let eccentricity = 0.016708634 - t * (0.000042037 + t * 1.267e-7);
    let centre = (1.914602 - t * (0.004817 + t * 0.000014)) * mean_anomaly.sin()
        + (0.019993 - t * 0.000101) * (2.0 * mean_anomaly).sin()
        + 0.000289 * (3.0 * mean_anomaly).sin();

    // The moon's ascending node, the largest term of the nutation correction.
    let node = (125.04 - 1934.136 * t).to_radians();
    let apparent_longitude =
        (mean_longitude + centre - 0.00569 - 0.00478 * node.sin()).to_radians();
    let obliquity = (23.439291 - t * (0.0130042 + t * (1.64e-7 - t * 5.04e-7))
        + 0.00256 * node.cos())
    .to_radians();

    let declination = (obliquity.sin() * apparent_longitude.sin())
        .asin()
        .to_degrees();

    let y = (obliquity / 2.0).tan().powi(2);
    let l = mean_longitude.to_radians();
    let equation_of_time = y * (2.0 * l).sin() - 2.0 * eccentricity * mean_anomaly.sin()
        + 4.0 * eccentricity * y * mean_anomaly.sin() * (2.0 * l).cos()
        - 0.5 * y * y * (4.0 * l).sin()
        - 1.25 * eccentricity * eccentricity * (2.0 * mean_anomaly).sin();
    let minutes = equation_of_time.to_degrees() * 4.0;

    // Julian days start at noon; the +0.5 moves that to midnight.
    let ut_hours = (julian_day + 0.5).rem_euclid(1.0) * 24.0;
    let lon = -(15.0 * (ut_hours - 12.0) + minutes / 4.0);

    LonLat {
        lon: (lon + 180.0).rem_euclid(360.0) - 180.0,
        lat: declination,
    }
}

/// The terminator sampled every `step_deg` from -180 to 180 inclusive, closed
/// through the dark pole. The last point does not repeat the first.
pub fn night(sun: LonLat, step_deg: f64) -> Vec<LonLat> {
    assert!(
        step_deg > 0.0,
        "night() needs a positive step, got {step_deg}"
    );

    let mut points = Vec::with_capacity((360.0 / step_deg) as usize + 3);
    let mut lon = -180.0;
    while lon < 180.0 {
        points.push(LonLat {
            lon,
            lat: terminator(lon, sun),
        });
        lon += step_deg;
    }
    points.push(LonLat {
        lon: 180.0,
        lat: terminator(180.0, sun),
    });

    // The unlit pole is the one the sun is tilted away from.
    let pole = if sun.lat > 0.0 { -90.0 } else { 90.0 };
    points.push(LonLat {
        lon: 180.0,
        lat: pole,
    });
    points.push(LonLat {
        lon: -180.0,
        lat: pole,
    });
    points
}

/// The latitude where the sun is on the horizon at `lon`: solves
/// sin φ sin δ + cos φ cos δ cos H = 0 for H = lon - sun.lon.
fn terminator(lon: f64, sun: LonLat) -> f64 {
    let hour_angle = (lon - sun.lon).to_radians();
    let declination = sun.lat.to_radians();
    let lat = (-hour_angle.cos() * declination.cos())
        .atan2(declination.sin())
        .to_degrees();
    // atan2 spans a full turn; of the two solutions half a turn apart, only
    // one is a latitude.
    if lat > 90.0 {
        lat - 180.0
    } else if lat < -90.0 {
        lat + 180.0
    } else {
        lat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An instant, as read in `zone`.
    fn at(instant: &str, zone: &str) -> Zoned {
        instant
            .parse::<jiff::Timestamp>()
            .expect("a valid timestamp")
            .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone this machine knows"))
    }

    fn p(lon: f64, lat: f64) -> LonLat {
        LonLat { lon, lat }
    }

    fn close(got: f64, want: f64, within: f64) -> bool {
        (got - want).abs() <= within
    }

    #[test]
    fn projects_the_crop_onto_the_unit_square() {
        assert_eq!(project(p(-180.0, NORTH)), (0.0, 0.0));
        assert_eq!(project(p(180.0, NORTH)), (1.0, 0.0));
        assert_eq!(project(p(-180.0, SOUTH)), (0.0, 1.0));
        assert_eq!(project(p(180.0, SOUTH)), (1.0, 1.0));

        let (x, y) = project(p(0.0, 0.0));
        assert_eq!(x, 0.5);
        assert!(close(y, 0.6, 1e-12), "the equator sits at {y}");

        // Beyond the crop, clamped to the edge it left through.
        assert_eq!(project(p(0.0, 90.0)).1, 0.0);
        assert_eq!(project(p(0.0, -90.0)).1, 1.0);

        assert!(close(aspect(), 360.0 / 140.0, 1e-12));
    }

    #[test]
    fn tracks_the_suns_declination_through_the_year() {
        let equinox = subsolar(&at("2026-03-20T14:46:00Z", "UTC")).lat;
        assert!(close(equinox, 0.0, 0.5), "March equinox gave {equinox}");

        let june = subsolar(&at("2026-06-21T08:24:00Z", "UTC")).lat;
        assert!(close(june, 23.44, 0.2), "June solstice gave {june}");

        let december = subsolar(&at("2026-12-21T20:03:00Z", "UTC")).lat;
        assert!(
            close(december, -23.44, 0.2),
            "December solstice gave {december}"
        );
    }

    #[test]
    fn walks_the_subsolar_point_westward_with_the_clock() {
        let noon = subsolar(&at("2026-09-04T12:00:00Z", "UTC")).lon;
        assert!(close(noon, 0.0, 5.0), "UT noon gave {noon}");

        let evening = subsolar(&at("2026-09-04T18:00:00Z", "UTC")).lon;
        assert!(close(evening, -90.0, 5.0), "six hours later gave {evening}");
    }

    #[test]
    fn reads_the_instant_not_the_zone() {
        let utc = subsolar(&at("2026-09-04T12:00:00Z", "UTC"));
        let tokyo = subsolar(&at("2026-09-04T12:00:00Z", "Asia/Tokyo"));
        assert_eq!(utc, tokyo);
    }

    #[test]
    fn samples_the_terminator_where_the_sun_is_on_the_horizon() {
        for sun in [p(0.0, 23.4), p(-137.0, -18.2), p(92.5, 0.7)] {
            let polygon = night(sun, 3.0);
            let declination = sun.lat.to_radians();
            let (terminator, corners) = polygon.split_at(polygon.len() - 2);
            for point in terminator {
                assert!(
                    (-90.0..=90.0).contains(&point.lat),
                    "{point:?} is not a latitude"
                );
                let hour_angle = (point.lon - sun.lon).to_radians();
                let phi = point.lat.to_radians();
                let altitude = (phi.sin() * declination.sin()
                    + phi.cos() * declination.cos() * hour_angle.cos())
                .asin()
                .to_degrees();
                assert!(
                    altitude.abs() <= 0.01,
                    "{point:?} sees the sun at {altitude}"
                );
            }
            assert_eq!(terminator[0].lon, -180.0);
            assert_eq!(terminator[terminator.len() - 1].lon, 180.0);
            assert_eq!(corners.len(), 2);
        }
    }

    #[test]
    fn closes_through_the_pole_the_sun_is_tilted_away_from() {
        let northern_summer = night(p(0.0, 23.4), 15.0);
        assert_eq!(
            &northern_summer[northern_summer.len() - 2..],
            &[p(180.0, -90.0), p(-180.0, -90.0)]
        );

        let northern_winter = night(p(0.0, -23.4), 15.0);
        assert_eq!(
            &northern_winter[northern_winter.len() - 2..],
            &[p(180.0, 90.0), p(-180.0, 90.0)]
        );
    }
}
