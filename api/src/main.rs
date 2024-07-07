use chrono::{DateTime, Utc};
use errors::FirestoreError;
use firestore::*;
use metrics::{counter, describe_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;
use nanoid::nanoid;
use std::{collections::HashMap, net::{IpAddr, Ipv4Addr}, os::unix::net::SocketAddr, sync::{Arc, Mutex}, time::Duration};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

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
    describe_counter!("custom", "Just a random metric to check everything is working as expected.");
    
    // Create an instance
    let db: FirestoreDb = FirestoreDb::new(&config_env_var("PROJECT_ID")?).await?;

    let user_repo = FirestoreUserRepo {
        collection_name: "test".to_string().into(),
        db: db.into(),
    };
    // let user_repo = InMemoryUserRepo::default();

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `POST /users` goes to `create_user`
        .route("/users", post(create_user))
        .route("/users/:id", get(get_user))
        // .layer() TODO: Middleware (layer) with global generic metrics
        .with_state(AppState {
            user_repo: Arc::new(user_repo.clone()),
        });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<User>, StatusCode> {
    counter!("custom", "system" => "foo").increment(1);
    match state.user_repo.get_user(id).await {
        Ok(user) => match user {
            None => Err(StatusCode::NOT_FOUND),
            Some(user) => Ok(Json(user)),
        },
        // TODO: Map FirestoreError to StatusCodes
        // https://github.com/tokio-rs/axum/blob/main/examples/anyhow-error-response/src/main.rs
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn create_user(
    State(state): State<AppState>,
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> Result<(StatusCode, Json<User>), StatusCode> {
    // insert your application logic here
    let id = nanoid!();
    let user = User {
        id,
        username: payload.username,
        created_at: Utc::now(),
        deleted_at: None,
    };

    match state.user_repo.save_user(&user).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(user))),
        // TODO: Map FirestoreError to StatusCodes
        // https://github.com/tokio-rs/axum/blob/main/examples/anyhow-error-response/src/main.rs
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct CreateUser {
    username: String,
}

#[derive(Clone)]
struct AppState {
    user_repo: Arc<dyn UserRepo>,
}

// the output to our `create_user` handler
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct User {
    id: String,
    username: String,
    created_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}

#[async_trait]
trait UserRepo: Send + Sync {
    async fn get_user(&self, id: String) -> Result<std::option::Option<User>, FirestoreError>;

    async fn save_user(&self, user: &User) -> Result<(), FirestoreError>;
}

#[derive(Debug, Clone, Default)]
struct InMemoryUserRepo {
    map: Arc<Mutex<HashMap<String, User>>>,
}

#[async_trait]
impl UserRepo for InMemoryUserRepo {
    async fn get_user(&self, id: String) -> Result<std::option::Option<User>, FirestoreError> {
        Result::Ok(self.map.lock().unwrap().get(&id).cloned())
    }

    async fn save_user(&self, user: &User) -> Result<(), FirestoreError> {
        self.map
            .lock()
            .unwrap()
            .insert(user.id.clone(), user.clone());

        Result::Ok(())
    }
}

#[derive(Debug, Clone)]
struct FirestoreUserRepo {
    collection_name: Arc<String>,
    db: Arc<FirestoreDb>,
}

#[async_trait]
impl UserRepo for FirestoreUserRepo {
    async fn get_user(&self, id: String) -> Result<std::option::Option<User>, FirestoreError> {
        self.db
            .fluent()
            .select()
            .by_id_in(&self.collection_name)
            .obj()
            .one(&id)
            .await
    }

    async fn save_user(&self, user: &User) -> Result<(), FirestoreError> {
        self.db
            .fluent()
            .insert()
            .into(&self.collection_name)
            .document_id(&user.id)
            .object(user)
            .execute()
            .await
    }
}
