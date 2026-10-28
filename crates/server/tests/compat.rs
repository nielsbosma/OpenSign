#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use base64::Engine;
use http_body_util::BodyExt;
use opensign_core::context::App;
use opensign_core::testing::app;
use opensign_server::{AppState, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple 42";
const ORIGIN: &str = "http://localhost:3000";
const IP: &str = "203.0.113.20";

async fn send(router: &axum::Router, request: Request<Body>) -> (Response<Body>, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (Response::from_parts(parts, Body::empty()), serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn rpc(name: &str, body: Value, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::post(format!("/api/rpc/{name}")).header(header::CONTENT_TYPE, "application/json").header("x-forwarded-for", IP);
    if let Some(c) = cookie {
        b = b.header("cookie", c).header("origin", ORIGIN);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn with(method: &str, uri: &str, auth: &str, content_type: Option<&str>, body: Vec<u8>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri).header("authorization", auth).header("x-forwarded-for", IP);
    if let Some(ct) = content_type {
        b = b.header(header::CONTENT_TYPE, ct);
    }
    b.body(Body::from(body)).unwrap()
}

/// An admin with an API key that may create documents, and the admin's browser cookie.
async fn setup(app: &Arc<App>) -> (axum::Router, String, String) {
    let router = router(AppState::new(app.clone()));
    let first = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada Admin", "email": "ada@example.com", "password": PASSWORD });
    send(&router, rpc("setup.create_first_admin", first, None)).await;
    let (response, _) = send(&router, rpc("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), None)).await;
    let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_string();
    let (_, key) =
        send(&router, rpc("api_keys.create", json!({ "name": "Integration", "permissions": ["documents.create"] }), Some(&cookie))).await;
    (router, key["key"].as_str().unwrap().to_string(), cookie)
}

fn form(document: &Value) -> Vec<u8> {
    let encoded: String = document
        .to_string()
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("document={encoded}").into_bytes()
}

fn pdf_fixture() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf 72 720 Td (Agreement) Tj ET".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content, "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[tokio::test]
async fn scrive_compatible_api() {
    let Some(app) = app().await else { return };
    let (router, key, cookie) = setup(&app).await;
    let oauth =
        format!("oauth_signature_method=\"PLAINTEXT\", oauth_consumer_key=\"anything\", oauth_token=\"{key}\", oauth_signature=\"a&b\"");

    let boundary = "SCRIVE";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Avtal.pdf\"\r\nContent-Type: application/pdf\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&pdf_fixture());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let ct = format!("multipart/form-data; boundary={boundary}");

    let cookie_only = Request::post("/scrive/api/v2/documents/new")
        .header("cookie", &cookie)
        .header("origin", ORIGIN)
        .header(header::CONTENT_TYPE, &ct)
        .body(Body::from(body.clone()))
        .unwrap();
    let (response, err) = send(&router, cookie_only).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a browser session cannot drive the compatibility API");
    assert_eq!(err["error_type"], "invalid_authorisation");

    let (response, doc) = send(&router, with("POST", "/scrive/api/v2/documents/new", &oauth, Some(&ct), body)).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{doc}");
    assert_eq!(doc["title"], "Avtal");
    assert_eq!(doc["status"], "preparation");
    assert_eq!(doc["parties"][0]["is_author"], true);
    let id = doc["id"].as_str().unwrap().to_string();

    let mut update = doc.clone();
    update["title"] = json!("Anställningsavtal");
    update["days_to_sign"] = json!(14);
    update["parties"] = json!([
        doc["parties"][0],
        {
            "is_signatory": true, "signatory_role": "signing_party", "sign_order": 1, "delivery_method": "api",
            "authentication_method_to_view": "standard", "authentication_method_to_sign": "sms_pin",
            "fields": [
                { "type": "name", "order": 1, "value": "Bo" }, { "type": "name", "order": 2, "value": "Berg" },
                { "type": "email", "value": "bo@example.com" }, { "type": "mobile", "value": "+46701234567" },
                { "type": "signature", "name": "Signatur", "is_obligatory": true, "placements": [{ "xrel": 0.1, "yrel": 0.8, "wrel": 0.3, "hrel": 0.06, "page": 1 }] },
                { "type": "checkbox", "name": "Villkor", "is_checked": false, "is_obligatory": true, "placements": [{ "xrel": 0.1, "yrel": 0.7, "wrel": 0.03, "hrel": 0.02, "page": 1 }] }
            ]
        },
        { "is_signatory": false, "fields": [{ "type": "name", "order": 1, "value": "Cecilia" }, { "type": "email", "value": "cecilia@example.com" }] }
    ]);
    let request =
        with("POST", &format!("/scrive/api/v2/documents/{id}/update"), &oauth, Some("application/x-www-form-urlencoded"), form(&update));
    let (response, updated) = send(&router, request).await;
    assert_eq!(response.status(), StatusCode::OK, "{updated}");
    assert_eq!(updated["title"], "Anställningsavtal");
    let bo = &updated["parties"][1];
    assert_eq!(bo["authentication_method_to_sign"], "sms_pin");
    assert_eq!(bo["delivery_method"], "api");
    let signature = bo["fields"].as_array().unwrap().iter().find(|f| f["type"] == "signature").unwrap();
    assert_eq!(signature["placements"][0]["page"], 1);
    assert!((signature["placements"][0]["xrel"].as_f64().unwrap() - 0.1).abs() < 1e-9);
    assert_eq!(updated["parties"][2]["signatory_role"], "viewer");

    let (response, started) = send(&router, with("POST", &format!("/scrive/api/v2/documents/{id}/start"), &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK, "{started}");
    assert_eq!(started["status"], "pending");
    let link = started["parties"][1]["api_delivery_url"].as_str().unwrap();
    assert!(link.starts_with("/sign#"), "{link}");

    let filter = "%5B%7B%22filter_by%22%3A%22status%22%2C%22statuses%22%3A%5B%22pending%22%5D%7D%5D";
    let (response, list) =
        send(&router, with("GET", &format!("/scrive/api/v2/documents/list?filter={filter}"), &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK, "{list}");
    assert_eq!(list["documents"].as_array().unwrap().len(), 1);

    let (response, _) =
        send(&router, with("GET", &format!("/scrive/api/v2/documents/{id}/files/main/Avtal.pdf"), &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/pdf");

    let (response, err) =
        send(&router, with("GET", &format!("/scrive/api/v2/documents/{}/get", uuid::Uuid::new_v4()), &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(err["error_type"], "resource_not_found");
    assert_eq!(err["http_code"], 404);

    let (response, canceled) = send(&router, with("POST", &format!("/scrive/api/v2/documents/{id}/cancel"), &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK, "{canceled}");
    assert_eq!(canceled["status"], "canceled");
    assert!(canceled["parties"][1]["api_delivery_url"].is_null(), "no links once canceled");

    let (response, _) = send(&router, with("GET", &format!("/api/v2/documents/{id}/get"), &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK, "old path stays as an alias");
    let (response, _) = send(&router, with("GET", "/scrive/api/v2/nothing", &oauth, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
}

#[tokio::test]
async fn docusign_compatible_api() {
    let Some(app) = app().await else { return };
    let (router, key, _) = setup(&app).await;
    let bearer = format!("Bearer {key}");
    let base = "/docusign/restapi/v2.1/accounts/1234567";
    let pdf = base64::engine::general_purpose::STANDARD.encode(pdf_fixture());
    let envelope = |tabs: Value| {
        json!({
            "emailSubject": "Please sign the NDA",
            "emailBlurb": "Hi",
            "documents": [{ "documentBase64": pdf, "name": "NDA", "fileExtension": "pdf", "documentId": "1" }],
            "recipients": {
                "signers": [{ "email": "bo@example.com", "name": "Bo Berg", "recipientId": "1", "routingOrder": "1", "clientUserId": "crm-7", "tabs": tabs }],
                "carbonCopies": [{ "email": "cc@example.com", "name": "Cecilia", "recipientId": "2", "routingOrder": "2" }]
            },
            "status": "sent"
        })
    };
    let json_ct = Some("application/json");

    let anchored = envelope(json!({ "signHereTabs": [{ "anchorString": "/sig1/" }] }));
    let (response, err) =
        send(&router, with("POST", &format!("{base}/envelopes"), &bearer, json_ct, anchored.to_string().into_bytes())).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(err["errorCode"], "INVALID_REQUEST_PARAMETER");
    let (_, list) = send(&router, with("GET", &format!("{base}/envelopes?status=created"), &bearer, None, vec![])).await;
    assert_eq!(list["envelopes"].as_array().unwrap().len(), 0, "a refused envelope leaves no draft behind");

    let tabs = json!({
        "signHereTabs": [{ "documentId": "1", "pageNumber": "1", "xPosition": "100", "yPosition": "600" }],
        "dateSignedTabs": [{ "documentId": "1", "pageNumber": "1", "xPosition": "300", "yPosition": "640" }]
    });
    let (response, created) =
        send(&router, with("POST", &format!("{base}/envelopes"), &bearer, json_ct, envelope(tabs).to_string().into_bytes())).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{created}");
    assert_eq!(created["status"], "sent");
    let id = created["envelopeId"].as_str().unwrap().to_string();

    let (_, native) = send(&router, with("GET", &format!("/api/v1/documents/{id}"), &bearer, None, vec![])).await;
    let field = native["fields"].as_array().unwrap().iter().find(|f| f["kind"] == "signature").unwrap();
    assert!((field["x"].as_f64().unwrap() - 100.0 / 595.0).abs() < 1e-6, "points become page fractions: {field}");
    assert!((field["y"].as_f64().unwrap() - 600.0 / 842.0).abs() < 1e-6);

    let (response, envelope) = send(&router, with("GET", &format!("{base}/envelopes/{id}"), &bearer, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(envelope["status"], "sent");
    assert_eq!(envelope["emailSubject"], "Please sign the NDA");

    let (_, recipients) = send(&router, with("GET", &format!("{base}/envelopes/{id}/recipients"), &bearer, None, vec![])).await;
    assert_eq!(recipients["signers"][0]["recipientId"], "1");
    assert_eq!(recipients["carbonCopies"][0]["recipientId"], "2");
    assert_eq!(recipients["recipientCount"], "2");

    let view = json!({ "returnUrl": "https://crm.example/done", "authenticationMethod": "none", "email": "bo@example.com", "userName": "Bo Berg", "clientUserId": "crm-7" });
    let (response, view) =
        send(&router, with("POST", &format!("{base}/envelopes/{id}/views/recipient"), &bearer, json_ct, view.to_string().into_bytes()))
            .await;
    assert_eq!(response.status(), StatusCode::CREATED, "{view}");
    assert!(view["url"].as_str().unwrap().contains("/sign#"));

    let (response, _) = send(&router, with("GET", &format!("{base}/envelopes/{id}/documents/combined"), &bearer, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/pdf");

    let (response, _) = send(
        &router,
        with("PUT", &format!("{base}/envelopes/{id}"), &bearer, json_ct, json!({ "status": "voided" }).to_string().into_bytes()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "voiding needs a reason");
    let void = json!({ "status": "voided", "voidedReason": "Wrong version" });
    let (response, _) =
        send(&router, with("PUT", &format!("{base}/envelopes/{id}"), &bearer, json_ct, void.to_string().into_bytes())).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (_, envelope) = send(&router, with("GET", &format!("{base}/envelopes/{id}"), &bearer, None, vec![])).await;
    assert_eq!(envelope["status"], "voided");

    let (response, err) = send(&router, with("GET", &format!("{base}/envelopes/{}", uuid::Uuid::new_v4()), &bearer, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(err["errorCode"], "ENVELOPE_DOES_NOT_EXIST");
    let (response, _) = send(&router, Request::get(format!("{base}/envelopes/{id}")).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let (response, _) = send(&router, with("GET", &format!("/restapi/v2.1/accounts/1234567/envelopes/{id}"), &bearer, None, vec![])).await;
    assert_eq!(response.status(), StatusCode::OK, "old path stays as an alias");
}
