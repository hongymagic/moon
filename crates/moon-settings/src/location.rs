//! One-time Windows location and explicit city search; neither writes settings.
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Search {
    pub query: String,
}

fn search_path(query: &str) -> Result<String, String> {
    let query = query.trim();
    if !(2..=200).contains(&query.chars().count()) || query.chars().any(char::is_control) {
        return Err("Enter a city name between 2 and 200 characters.".into());
    }
    let mut encoded = String::new();
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    Ok(format!("/v1/search?name={encoded}&count=8&language=en&format=json"))
}

fn coordinates(latitude: f64, longitude: f64) -> bool {
    (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)
}

fn city_results(body: &Value) -> Result<Value, String> {
    if body["error"] == true {
        return Err(
            "The city service could not complete that search. Try a city and country.".into()
        );
    }
    let Some(results) = body.get("results") else { return Ok(json!({"results":[]})) };
    let results = results.as_array().ok_or("The city service returned an invalid result.")?;
    let results: Vec<_> = results.iter().filter_map(|entry| {
        let id = entry["id"].as_u64()?;
        let latitude = entry["latitude"].as_f64()?;
        let longitude = entry["longitude"].as_f64()?;
        if !coordinates(latitude, longitude) { return None; }
        let name = entry["name"].as_str()?.trim();
        if name.is_empty() { return None; }
        let mut parts = vec![name];
        for field in ["admin1", "country"] {
            if let Some(value) = entry[field].as_str().map(str::trim)
                && !value.is_empty() && !parts.contains(&value) { parts.push(value); }
        }
        let label: String = parts.join(", ").chars().take(200).collect();
        if label.chars().any(char::is_control) { return None; }
        Some(json!({"id":id.to_string(),"label":label,"latitude":latitude,"longitude":longitude}))
    }).take(8).collect();
    Ok(json!({"results":results}))
}

#[cfg(windows)]
pub fn search(query: &str) -> Result<Value, String> {
    let path = search_path(query)?;
    let body = moon_win::http::meteo_json("geocoding-api.open-meteo.com", &path).map_err(|_| {
        "City search is unavailable. Check your connection and try again.".to_owned()
    })?;
    city_results(&body)
}

/// Call on the foreground Settings UI thread. The Windows permission request is
/// initiated there; waiting and the bounded location lookup run on a dedicated MTA.
#[cfg(windows)]
pub fn current(finish: impl FnOnce(Result<Value, String>) + Send + 'static) -> Result<(), String> {
    use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy};
    use windows::Foundation::TimeSpan;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    let access = Geolocator::RequestAccessAsync()
        .map_err(|_| "Windows location is unavailable. Choose a city instead.".to_owned())?;
    std::thread::Builder::new().name("moon-settings-location".into()).spawn(move || {
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) { unsafe { RoUninitialize() }; }
        }
        let result = (|| {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| error.to_string())?;
            let _apartment = Apartment;
            let permission = access.join().map_err(|_| "Windows could not check location permission. Choose a city instead.")?;
            if permission != GeolocationAccessStatus::Allowed {
                return Err("Location access is off. Enable it in Windows location settings or choose a city.".into());
            }
            let locator = Geolocator::new().map_err(|error| error.to_string())?;
            locator.SetDesiredAccuracy(PositionAccuracy::Default).map_err(|error| error.to_string())?;
            let position = locator.GetGeopositionAsyncWithAgeAndTimeout(
                TimeSpan { Duration: 300 * 10_000_000 },
                TimeSpan { Duration: 15 * 10_000_000 },
            ).and_then(|operation| operation.join())
                .map_err(|_| "Windows could not find your location. Choose a city instead.")?;
            let point = position.Coordinate().and_then(|coordinate| coordinate.Point())
                .and_then(|point| point.Position()).map_err(|error| error.to_string())?;
            if !coordinates(point.Latitude, point.Longitude) {
                return Err("Windows returned an unavailable location. Choose a city instead.".into());
            }
            // City weather needs no street-level coordinate in the saved file.
            Ok(json!({"latitude":(point.Latitude*100.0).round()/100.0,
                "longitude":(point.Longitude*100.0).round()/100.0,"label":"Current location"}))
        })();
        finish(result);
    }).map(|_| ()).map_err(|_| "Could not start the location lookup. Try again.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_query_is_bounded_and_cannot_add_parameters() {
        let search: Search = serde_json::from_value(json!({"query":"Sydney"})).unwrap();
        assert!(search_path(&search.query).is_ok());
        assert!(serde_json::from_value::<Search>(json!({"query":"Sydney","extra":true})).is_err());
        assert!(search_path(" Sydney, AU ").unwrap().contains("name=Sydney%2C%20AU&count=8"));
        assert!(
            search_path("São Paulo &count=99")
                .unwrap()
                .contains("S%C3%A3o%20Paulo%20%26count%3D99")
        );
        for query in ["a", "\nParis\n\0", &"x".repeat(201)] {
            assert!(search_path(query).is_err());
        }
    }

    #[test]
    fn results_preserve_identity_and_disambiguate_without_accepting_bad_coordinates() {
        let body = json!({"results":[
            {"id":9007199254740993u64,"name":"Sydney","admin1":"New South Wales","country":"Australia","latitude":-33.87,"longitude":151.21},
            {"id":2,"name":"Broken","latitude":91,"longitude":0},
            {"id":3,"name":"Broken\nlabel","latitude":0,"longitude":0}
        ]});
        let result = city_results(&body).unwrap();
        assert_eq!(result["results"].as_array().unwrap().len(), 1);
        assert_eq!(result["results"][0]["id"], "9007199254740993");
        assert_eq!(result["results"][0]["label"], "Sydney, New South Wales, Australia");
        assert_eq!(city_results(&json!({})).unwrap(), json!({"results":[]}));
        assert!(city_results(&json!({"results":"invalid"})).is_err());
        assert!(city_results(&json!({"error":true})).is_err());
        assert!(!coordinates(f64::NAN, 0.0));
    }
}
