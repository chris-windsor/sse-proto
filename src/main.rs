use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::sse::Event;
use axum::response::Sse;
use axum::{routing::get, Json, Router};
use axum_valid::Valid;
use fake::faker::name::raw::Name;
use fake::locales::EN;
use fake::Fake;
use futures::Stream;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde_json::{from_str, json, Map, Value};
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::sleep;
use validator::Validate;

fn fill_string(subject_string: &String) -> Value {
    let new_string = subject_string.replace("$name", &Name(EN).fake::<String>());

    return Value::String(new_string);
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

async fn health_check() -> &'static str {
    "howdy 🤠"
}

#[shuttle_runtime::main]
async fn axum() -> shuttle_axum::ShuttleAxum {
    let router = Router::new()
        .route("/", get(sse))
        .route("/health", get(health_check));

    Ok(router.into())
}
