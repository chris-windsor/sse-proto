use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::sse::Event;
use axum::response::Sse;
use axum::{routing::get, Json, Router};
use axum_valid::Valid;
use chrono::Utc;
use fake::faker::address::raw::{CityName, StreetName, ZipCode};
use fake::faker::internet::raw::SafeEmail;
use fake::faker::name::raw::Name;
use fake::faker::number::raw::NumberWithFormat;
use fake::faker::phone_number::raw::PhoneNumber;
use fake::locales::EN;
use fake::Fake;
use futures::Stream;
use lazy_static::lazy_static;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde_json::{from_str, json, Map, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;
use validator::Validate;

type StringSubstitutionsMap = HashMap<&'static str, Box<dyn Fn() -> String + Sync>>;

macro_rules! generate_replacements {
    ($($placeholder:expr => $generator:expr),*) => {{
        let mut replacements: StringSubstitutionsMap = HashMap::new();
        $(replacements.insert($placeholder, Box::new($generator));)*
        replacements
    }};
}

lazy_static! {
    static ref STRING_SUBSTITUTIONS: StringSubstitutionsMap = generate_replacements! {
        "address" => || StreetName(EN).fake(),
        "city" => || CityName(EN).fake(),
        "datetime" => || Utc::now().to_rfc3339(),
        "email" => || SafeEmail(EN).fake(),
        "name" => || Name(EN).fake(),
        "number" => || NumberWithFormat(EN, "^###").fake(),
        "phone" => || PhoneNumber(EN).fake(),
        "uuid" => || Uuid::new_v4().to_string(),
        "zip" => || ZipCode(EN).fake()
    };
}

fn fill_string(subject_string: &String) -> Value {
    let mut result = String::new();

    let mut is_placeholder = false;
    let mut placeholder_start: usize = 0;

    for (char_index, char) in subject_string.chars().enumerate() {
        if char == '\\' {
            continue;
        } else if char == '{' {
            is_placeholder = true;
            placeholder_start = char_index + 1;
            continue;
        } else {
            if is_placeholder {
                if char == '}' {
                    if let Some(replacement_func) =
                        STRING_SUBSTITUTIONS.get(&subject_string[placeholder_start..char_index])
                    {
                        result.push_str(&replacement_func());
                        is_placeholder = false;
                        placeholder_start = 0;
                        continue;
                    }
                }
            } else {
                result.push(char);
            }
        }
    }

    return Value::String(result);
}

fn fill_object_fields(object: &Map<String, Value>) -> Map<String, Value> {
    let new_objects = object
        .iter()
        .map(|(key, value)| {
            let replacement_value = match value {
                Value::Object(object) => (key.clone(), Value::Object(fill_object_fields(object))),
                Value::String(subject_string) => (key.clone(), fill_string(subject_string)),
                _ => (key.clone(), value.clone()),
            };

            replacement_value
        })
        .collect::<Map<String, Value>>();

    new_objects
}

#[derive(Deserialize, Validate)]
struct SSEQuery {
    #[validate(range(min = 1000, message = "interval_min must be greater than 1000ms"))]
    interval_min: u64,
    #[validate(range(min = 1000, message = "interval_max must be greater than 1000ms"))]
    interval_max: u64,
    shape: String,
}

async fn sse(
    river_query: Valid<Query<SSEQuery>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<serde_json::Value>)>
{
    let stream = async_stream::stream! {
        loop {
            let delay = Duration::from_millis(
                thread_rng().gen_range(river_query.interval_min..river_query.interval_max)
            );
            sleep(delay).await;

            let shape: Value = from_str(river_query.shape.as_str()).unwrap();
            let shape = shape.as_object().unwrap();

            let new_shape = fill_object_fields(shape);
            let message = json!(new_shape);

            yield Ok(Event::default().json_data(message).unwrap());
        }
    };

    Ok(Sse::new(stream))
}

async fn get_available_substitutions(
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    Ok(Json(json!(STRING_SUBSTITUTIONS
        .keys()
        .cloned()
        .collect::<Vec<&str>>())))
}

async fn health_check() -> &'static str {
    "howdy 🤠"
}

#[shuttle_runtime::main]
async fn axum() -> shuttle_axum::ShuttleAxum {
    let router = Router::new()
        .route("/", get(sse))
        .route("/substitutions", get(get_available_substitutions))
        .route("/health", get(health_check));

    Ok(router.into())
}
