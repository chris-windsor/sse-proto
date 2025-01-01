use axum::extract::Query;
use axum::http::{Method, StatusCode};
use axum::response::sse::Event;
use axum::response::Sse;
use axum::{routing::get, Json, Router};
use axum_valid::Valid;
use chrono::Utc;
use fake::faker::address::en::{CityName, StreetName, ZipCode};
use fake::faker::boolean::en::Boolean;
use fake::faker::color::en::HexColor;
use fake::faker::creditcard::en::CreditCardNumber;
use fake::faker::internet::en::{IPv4, SafeEmail};
use fake::faker::lorem::en::{Paragraph, Words};
use fake::faker::name::en::Name;
use fake::faker::number::en::NumberWithFormat;
use fake::faker::phone_number::en::PhoneNumber;
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
use tower_http::cors::{self, CorsLayer};
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
        "address" => || StreetName().fake(),
        "bool" => || Boolean(50).fake::<bool>().to_string(),
        "city" => || CityName().fake(),
        "color" => || HexColor().fake(),
        "creditcard" => || CreditCardNumber().fake(),
        "datetime" => || Utc::now().to_rfc3339(),
        "email" => || SafeEmail().fake(),
        "ip" => || IPv4().fake(),
        "name" => || Name().fake(),
        "number" => || NumberWithFormat("^###").fake(),
        "paragraph" => || Paragraph(1..3).fake(),
        "phone" => || PhoneNumber().fake(),
        "uuid" => || Uuid::new_v4().to_string(),
        "words" => || Words(3..5).fake::<Vec<String>>().join(" "),
        "zip" => || ZipCode().fake()
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

    Value::String(result)
}

fn fill_object_fields(object: &Map<String, Value>) -> Map<String, Value> {
    object
        .iter()
        .map(|(key, value)| {
            let replacement_value = match value {
                Value::Object(object) => (key.clone(), Value::Object(fill_object_fields(object))),
                Value::String(subject_string) => (key.clone(), fill_string(subject_string)),
                _ => (key.clone(), value.clone()),
            };

            replacement_value
        })
        .collect::<Map<String, Value>>()
}

#[derive(Deserialize, Validate)]
struct SSEQuery {
    #[validate(range(min = 1000, message = "interval_min must be >= 1000ms"))]
    interval_min: u64,
    #[validate(range(min = 2000, message = "interval_max must be >= 2000ms"))]
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

#[tokio::main]
async fn main() {
    let cors_layer = CorsLayer::new()
        .allow_methods([Method::HEAD, Method::GET])
        .allow_origin(cors::Any);

    let app = Router::new()
        .route("/", get(sse))
        .route("/substitutions", get(get_available_substitutions))
        .layer(cors_layer);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
