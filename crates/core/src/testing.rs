use std::sync::Arc;

use opensign_crypto::Keys;

use crate::context::{App, Ctx, Surface};
use crate::env::{Env, ProcessRole};
use crate::net::SafeHttp;
use crate::settings::SettingsCache;
use crate::storage::Storage;

pub const TEST_SECRET: &str = "test-secret-test-secret-test-secret-0123";

/// An app on a fresh database schema with an in-memory bucket, or `None` when `TEST_DATABASE_URL`
/// is not set (the test is then skipped).
pub async fn app() -> Option<Arc<App>> {
    app_with(true).await
}

/// As `app`, with the outbound rules a real install has.
pub async fn production_app() -> Option<Arc<App>> {
    app_with(false).await
}

async fn app_with(development: bool) -> Option<Arc<App>> {
    let db = opensign_db::test_db().await?;
    opensign_db::migrate(&db).await.expect("migrations apply");
    let env = Env {
        database_url: String::new(),
        s3_endpoint: None,
        s3_bucket: "test".into(),
        s3_access_key: String::new(),
        s3_secret_key: String::new(),
        s3_region: "us-east-1".into(),
        app_secret: TEST_SECRET.into(),
        app_url: Some("http://localhost:3000".into()),
        setup_code: None,
        converter_url: std::env::var("TEST_CONVERTER_URL").ok(),
        port: 3000,
        role: ProcessRole::All,
        development,
    };
    Some(Arc::new(App {
        env,
        db,
        keys: Keys::derive(TEST_SECRET).expect("test secret"),
        storage: Storage::in_memory(),
        settings: SettingsCache::default(),
        http: SafeHttp::new(),
    }))
}

pub fn system_ctx(app: &Arc<App>) -> Ctx {
    Ctx::new(app.clone(), Surface::System)
}
