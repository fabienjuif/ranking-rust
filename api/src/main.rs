use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use firestore::*;
use metrics::{counter, describe_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use rank::{Rank, RankRepo, RankRepoFirestore};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

mod rank;

pub fn config_env_var(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|e| format!("{}: {}", name, e))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("firestore=error")
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Metrics
    let builder = PrometheusBuilder::new();
    builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
            Some(Duration::from_secs(10)),
        )
        .install()
        .expect("failed to install Prometheus recorder");
    describe_counter!(
        "custom",
        "Just a random metric to check everything is working as expected."
    );

    // Create an instance
    let db: FirestoreDb = FirestoreDb::new(&config_env_var("PROJECT_ID")?).await?;
    let rank_repo = RankRepoFirestore::new(db.into());
    // let rank_repo = RankRepoInMemory::default();

    // build our application with a route
    let app = Router::new()
        .route("/projects/:projectId/items", post(create_item))
        .route("/projects/:projectId/items/:itemId", get(get_item))
        .route("/projects/:projectId/items/:itemId/rank", post(rank_item))
        // .layer() TODO: Middleware (layer) with global generic metrics
        .with_state(AppState {
            rank_repo: Arc::new(rank_repo.clone()),
        });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn get_item(
    State(state): State<AppState>,
    Path((project_id, item_id)): Path<(String, String)>,
) -> Result<Json<Rank>, StatusCode> {
    counter!("custom", "system" => "foo").increment(1);
    let rank_id: Rank = Rank {
        item_id,
        project_id,
        ..Default::default()
    };
    match state.rank_repo.get(rank_id.get_computed_id()).await {
        Ok(user) => match user {
            None => Err(StatusCode::NOT_FOUND),
            Some(user) => Ok(Json(user)),
        },
        // TODO: Map FirestoreError to StatusCodes
        // https://github.com/tokio-rs/axum/blob/main/examples/anyhow-error-response/src/main.rs
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn create_item(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(payload): Json<CreateItem>,
) -> Result<(StatusCode, Json<Rank>), StatusCode> {
    let mut item = Rank {
        project_id,
        item_id: payload.item_id,
        min: payload.min,
        max: payload.max,
        // average can be anything since the total is 0
        average: 0.,
        total: 0,
        created_at: Utc::now(),
        ..Default::default()
    };
    item.compute_id();

    match state.rank_repo.save(&item).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(item))),
        // TODO: Map FirestoreError to StatusCodes
        // https://github.com/tokio-rs/axum/blob/main/examples/anyhow-error-response/src/main.rs
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateItem {
    item_id: String,
    min: f64,
    max: f64,
}

async fn rank_item(
    State(state): State<AppState>,
    Path((project_id, item_id)): Path<(String, String)>,
    Json(payload): Json<RankItem>,
) -> StatusCode {
    let rank_id: Rank = Rank {
        item_id,
        project_id,
        ..Default::default()
    };
    match state
        .rank_repo
        .rank(rank_id.get_computed_id(), payload.score)
        .await
    {
        Ok(_) => StatusCode::OK,
        // TODO: Map FirestoreError to StatusCodes
        // https://github.com/tokio-rs/axum/blob/main/examples/anyhow-error-response/src/main.rs
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RankItem {
    score: f64,
}

#[derive(Clone)]
struct AppState {
    rank_repo: Arc<dyn RankRepo>,
}
