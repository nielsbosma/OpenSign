use std::sync::Arc;

use bytes::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use opensign_crypto::{DataKey, Keys, open_blob, seal_blob, sha256_hex};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::env::Env;
use crate::error::{AppError, Result, not_found};

#[derive(Clone)]
pub struct Storage {
    store: Arc<dyn ObjectStore>,
}

impl Storage {
    pub fn from_env(env: &Env) -> Result<Self> {
        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&env.s3_bucket)
            .with_access_key_id(&env.s3_access_key)
            .with_secret_access_key(&env.s3_secret_key)
            .with_region(&env.s3_region);
        if let Some(endpoint) = &env.s3_endpoint {
            builder =
                builder.with_endpoint(endpoint).with_virtual_hosted_style_request(false).with_allow_http(endpoint.starts_with("http://"));
        }
        Ok(Self { store: Arc::new(builder.build().map_err(AppError::internal)?) })
    }

    pub fn in_memory() -> Self {
        Self { store: Arc::new(object_store::memory::InMemory::new()) }
    }

    pub async fn put_raw(&self, key: &str, body: Vec<u8>) -> Result<()> {
        self.store.put(&Path::from(key), PutPayload::from(body)).await.map_err(AppError::internal)?;
        Ok(())
    }

    pub async fn get_raw(&self, key: &str) -> Result<Bytes> {
        match self.store.get(&Path::from(key)).await {
            Ok(result) => result.bytes().await.map_err(AppError::internal),
            Err(object_store::Error::NotFound { .. }) => Err(not_found("File")),
            Err(e) => Err(AppError::internal(e)),
        }
    }

    pub async fn delete_raw(&self, key: &str) -> Result<()> {
        match self.store.delete(&Path::from(key)).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(AppError::internal(e)),
        }
    }

    /// Checks that the bucket answers and that this key may write and delete.
    pub async fn check(&self) -> Result<()> {
        let key = format!("health/{}", Uuid::new_v4());
        self.put_raw(&key, b"ok".to_vec()).await?;
        self.delete_raw(&key).await
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    #[serde(skip)]
    #[schemars(skip)]
    pub storage_key: String,
    #[serde(skip)]
    #[schemars(skip)]
    pub wrapped_key: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct NewFile<'a> {
    pub kind: &'a str,
    pub name: &'a str,
    pub content_type: &'a str,
    pub body: &'a [u8],
    pub uploaded_by: Option<Uuid>,
}

/// Encrypts the body with a fresh data key, uploads it under an opaque key, and records it.
/// The bucket never sees plaintext or file names.
pub async fn store_file(conn: &mut PgConnection, storage: &Storage, keys: &Keys, file: NewFile<'_>) -> Result<FileRecord> {
    let id = Uuid::now_v7();
    let dek = DataKey::generate();
    let sealed = seal_blob(&dek, id.as_bytes(), file.body);
    let storage_key = format!("files/{}/{}", &id.simple().to_string()[..2], id.simple());
    storage.put_raw(&storage_key, sealed).await?;
    let record = sqlx::query_as::<_, FileRecord>(
        "insert into files (id, kind, name, content_type, size_bytes, sha256, storage_key, wrapped_key, uploaded_by)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning id, kind, name, content_type, size_bytes, sha256, storage_key, wrapped_key, created_at",
    )
    .bind(id)
    .bind(file.kind)
    .bind(file.name)
    .bind(file.content_type)
    .bind(file.body.len() as i64)
    .bind(sha256_hex(file.body))
    .bind(&storage_key)
    .bind(keys.wrap_data_key(&dek, &id.to_string()))
    .bind(file.uploaded_by)
    .fetch_one(&mut *conn)
    .await?;
    Ok(record)
}

pub async fn file_record(conn: &mut PgConnection, id: Uuid) -> Result<FileRecord> {
    sqlx::query_as::<_, FileRecord>(
        "select id, kind, name, content_type, size_bytes, sha256, storage_key, wrapped_key, created_at from files where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| not_found("File"))
}

/// Downloads, decrypts and checks the body against the hash recorded at upload.
pub async fn read_file(storage: &Storage, keys: &Keys, record: &FileRecord) -> Result<Vec<u8>> {
    let sealed = storage.get_raw(&record.storage_key).await?;
    let dek = keys.unwrap_data_key(&record.wrapped_key, &record.id.to_string())?;
    let body = open_blob(&dek, record.id.as_bytes(), &sealed)?;
    if sha256_hex(&body) != record.sha256 {
        return Err(AppError::internal(anyhow::anyhow!("file {} does not match its recorded hash", record.id)));
    }
    Ok(body)
}
