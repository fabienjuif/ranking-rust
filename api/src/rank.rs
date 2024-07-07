use chrono::{DateTime, Utc};
use errors::{FirestoreDataNotFoundError, FirestoreError, FirestoreErrorPublicGenericDetails};
use firestore::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

static RANK_FIRESTORE_COLLECTION: &str = "ranks";

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Rank {
    /// PK is project_id+item_id, the total length is 42 (21+21) since id are generated via nanoid()
    pub id: String,
    pub project_id: String,
    pub item_id: String,
    pub total: i64,
    pub average: f64,
    /// stores the minimal note this item can have (1 for 1-5, 0 for 0-20 or 0-100 for example)
    pub min: f64,
    /// stores the maximal note this item can have (5 for 1-5, 20 for 0-20 or 100 for 0-100 for example)
    pub max: f64,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Rank {
    pub fn get_computed_id(&self) -> String {
        format!("{}{}", self.project_id, self.item_id)
    }

    pub fn compute_id(&mut self) {
        self.id = self.get_computed_id();
    }

    // TODO: add error handling to reject if score < min || score > max
    pub fn update_score(&mut self, score: f64) {
        let old_average = self.average;
        self.average = ((old_average * self.total as f64) + score) / (self.total + 1) as f64;
        self.total += 1;
    }
}

#[async_trait]
pub trait RankRepo: Send + Sync {
    async fn get(&self, id: String) -> Result<std::option::Option<Rank>, FirestoreError>;
    async fn save(&self, rank: &Rank) -> Result<(), FirestoreError>;
    async fn rank(&self, id: String, score: f64) -> Result<(), FirestoreError>;
}

#[derive(Debug, Clone, Default)]
pub struct RankRepoInMemory {
    map: Arc<Mutex<HashMap<String, Rank>>>,
}

#[async_trait]
impl RankRepo for RankRepoInMemory {
    async fn get(&self, id: String) -> Result<std::option::Option<Rank>, FirestoreError> {
        Ok(self.map.lock().unwrap().get(&id).cloned())
    }

    async fn save(&self, rank: &Rank) -> Result<(), FirestoreError> {
        self.map
            .lock()
            .unwrap()
            .insert(rank.get_computed_id(), rank.clone());

        Ok(())
    }

    // TODO: test it I am curious to check it works (that we get the HashMap ref)
    async fn rank(&self, id: String, score: f64) -> Result<(), FirestoreError> {
        let mut guard = self.map.lock().unwrap();
        let Some(rank) = guard.get_mut(&id) else {
            return Err(FirestoreError::DataNotFoundError(
                FirestoreDataNotFoundError::new(
                    FirestoreErrorPublicGenericDetails::new("5".to_string()), // TODO: better error handling, here 5 comes from gRPC not found
                    "5".to_string(),
                ),
            ));
        };

        rank.update_score(score);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RankRepoFirestore {
    db: Arc<FirestoreDb>,
}

impl RankRepoFirestore {
    pub fn new(db: Arc<FirestoreDb>) -> Self {
        RankRepoFirestore { db }
    }
}

#[async_trait]
impl RankRepo for RankRepoFirestore {
    async fn get(&self, id: String) -> Result<std::option::Option<Rank>, FirestoreError> {
        self.db
            .fluent()
            .select()
            .by_id_in(RANK_FIRESTORE_COLLECTION)
            .obj()
            .one(&id)
            .await
    }

    async fn save(&self, rank: &Rank) -> Result<(), FirestoreError> {
        self.db
            .fluent()
            .insert()
            .into(RANK_FIRESTORE_COLLECTION)
            .document_id(&rank.get_computed_id())
            .object(rank)
            .execute()
            .await
    }

    async fn rank(&self, id: String, score: f64) -> Result<(), FirestoreError> {
        self.db
            .run_transaction(|db, transaction| {
                let id: String = id.clone();

                Box::pin(async move {
                    let mut rank: Rank = db
                        .fluent()
                        .select()
                        .by_id_in(RANK_FIRESTORE_COLLECTION)
                        .obj()
                        .one(&id)
                        .await?
                        .expect("Missing document"); // TODO: check the 404 is acting like others 404

                    rank.update_score(score);

                    db.fluent()
                        .update()
                        .fields(paths ! (Rank::{
                         average,
                         total,
                        }))
                        .in_col(RANK_FIRESTORE_COLLECTION)
                        .document_id(&id)
                        .object(&rank)
                        .add_to_transaction(transaction)?;
                    Ok(())
                })
            })
            .await
    }
}
