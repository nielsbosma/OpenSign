//! Scrive Document API v2 (`/api/v2/documents/...`). Point an existing Scrive integration at
//! OpenSign and give it an API key as the OAuth access token; the other three OAuth values are
//! accepted and ignored, since the key alone carries the full secret.

use std::collections::HashMap;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{FromRequest, Multipart, Path, RawQuery, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use chrono::{Duration, Utc};
use opensign_core::error::{AppError, ErrorKind, bad_request};
use opensign_core::i18n::Lang;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::{Api, split_name, str_of, uuid};
use crate::AppState;
use crate::rest::query_pairs;

const BODY_LIMIT: usize = 60 * 1024 * 1024;

fn error(e: AppError, request_id: &str) -> Response {
    let (status, kind) = match e.kind {
        ErrorKind::BadRequest => (StatusCode::BAD_REQUEST, "request_parameters_invalid"),
        ErrorKind::Unauthorized => (StatusCode::UNAUTHORIZED, "invalid_authorisation"),
        ErrorKind::Forbidden => (StatusCode::FORBIDDEN, "insufficient_privileges"),
        ErrorKind::NotFound => (StatusCode::NOT_FOUND, "resource_not_found"),
        ErrorKind::Conflict => (StatusCode::CONFLICT, "document_state_error"),
        ErrorKind::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "request_failed"),
        ErrorKind::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "server_error"),
        ErrorKind::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
    };
    let message = super::log_internal(&e, request_id);
    (status, Json(json!({ "error_type": kind, "error_message": message, "http_code": status.as_u16() }))).into_response()
}

/// `oauth_token` from Scrive's PLAINTEXT OAuth header, or a plain bearer token.
fn token(parts: &Parts) -> Option<String> {
    let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    if let Some(bearer) = value.strip_prefix("Bearer ") {
        return Some(bearer.trim().to_string());
    }
    let value = value.strip_prefix("OAuth ").unwrap_or(value);
    value.split(',').find_map(|pair| {
        let (key, raw) = pair.trim().split_once('=')?;
        (key.trim() == "oauth_token").then(|| crate::rest::decode(raw.trim().trim_matches('"')))
    })
}

fn status_out(status: &str) -> &'static str {
    match status {
        "draft" => "preparation",
        "pending" => "pending",
        "completed" => "closed",
        "canceled" => "canceled",
        "expired" => "timedout",
        "declined" => "rejected",
        _ => "document_error",
    }
}

fn delivery_out(delivery: &str) -> &'static str {
    match delivery {
        "sms" => "mobile",
        "in_person" => "pad",
        "link" | "none" => "api",
        _ => "email",
    }
}

fn delivery_in(value: Option<&str>) -> Result<&'static str, AppError> {
    Ok(match value.unwrap_or("email") {
        "email" | "email_mobile" | "portal" => "email",
        "mobile" => "sms",
        "pad" => "in_person",
        "api" => "link",
        other => return Err(bad_request(format!("delivery_method {other} is not supported"))),
    })
}

fn auth_out(method: &str) -> &str {
    match method {
        "none" => "standard",
        other => other,
    }
}

fn auth_in(value: Option<&str>, to_view: bool) -> Result<&'static str, AppError> {
    Ok(match value.unwrap_or("standard") {
        "standard" if to_view => "none",
        "standard" => "standard",
        "se_bankid" => "se_bankid",
        "sms_pin" => "sms_pin",
        "freja" => "freja",
        other => return Err(bad_request(format!("Authentication method {other} is not available here"))),
    })
}

fn placement(f: &Value) -> Value {
    json!({
        "xrel": f["x"], "yrel": f["y"], "wrel": f["width"], "hrel": f["height"], "fsrel": 0.0168,
        "page": f["page"].as_i64().unwrap_or(0) + 1, "tip": null, "anchors": [],
    })
}

fn party_out(p: &Value, fields: &[&Value], link: Option<&String>, app_url: &str) -> Value {
    let (first, last) = split_name(p["name"].as_str().unwrap_or_default());
    let placements = |kind: &str| -> Vec<Value> { fields.iter().filter(|f| f["kind"] == kind).map(|f| placement(f)).collect() };
    let mut out_fields = vec![
        json!({ "type": "name", "order": 1, "value": first, "is_obligatory": true, "should_be_filled_by_sender": false, "placements": placements("name") }),
        json!({ "type": "name", "order": 2, "value": last, "is_obligatory": false, "should_be_filled_by_sender": false, "placements": [] }),
        json!({ "type": "email", "value": p["email"].as_str().unwrap_or_default(), "is_obligatory": false, "should_be_filled_by_sender": false, "placements": placements("email") }),
        json!({ "type": "mobile", "value": p["phone"].as_str().unwrap_or_default(), "is_obligatory": false, "should_be_filled_by_sender": false, "placements": [] }),
        json!({ "type": "company", "value": p["company"].as_str().unwrap_or_default(), "is_obligatory": false, "should_be_filled_by_sender": false, "placements": placements("company") }),
        json!({ "type": "personal_number", "value": p["personalNumber"].as_str().unwrap_or_default(), "is_obligatory": false, "should_be_filled_by_sender": false, "placements": [] }),
    ];
    let mut radio_groups: Vec<String> = Vec::new();
    for (n, f) in fields.iter().enumerate() {
        let label = f["label"].as_str().filter(|l| !l.is_empty()).map(str::to_string);
        let required = f["required"].as_bool().unwrap_or(true);
        match f["kind"].as_str().unwrap_or_default() {
            "signature" | "initials" => out_fields.push(json!({
                "type": "signature", "name": label.unwrap_or_else(|| format!("Signature {}", n + 1)),
                "is_obligatory": required, "should_be_filled_by_sender": false, "placements": [placement(f)],
            })),
            "checkbox" => out_fields.push(json!({
                "type": "checkbox", "name": label.unwrap_or_else(|| format!("Checkbox {}", n + 1)),
                "is_checked": f["value"] == "true", "is_obligatory": required, "should_be_filled_by_sender": false, "placements": [placement(f)],
            })),
            "radio" => {
                let group = f["groupKey"].as_str().unwrap_or_default();
                if radio_groups.iter().any(|g| g == group) {
                    continue;
                }
                radio_groups.push(group.to_string());
                let buttons: Vec<&&Value> = fields.iter().filter(|b| b["kind"] == "radio" && b["groupKey"] == group).collect();
                out_fields.push(json!({
                    "type": "radiogroup", "name": label.unwrap_or_else(|| group.to_string()),
                    "values": buttons.iter().map(|b| b["options"][0].clone()).collect::<Vec<_>>(),
                    "selected_value": buttons.iter().find_map(|b| b["value"].as_str()),
                    "is_obligatory": buttons.iter().any(|b| b["required"] != false), "should_be_filled_by_sender": false,
                    "placements": buttons.iter().map(|b| placement(b)).collect::<Vec<_>>(),
                }));
            }
            "text" | "date" | "number" | "select" | "multiselect" | "phone" | "date_input" | "image" | "file" => out_fields.push(json!({
                "type": "text", "name": label.unwrap_or_else(|| format!("Field {}", n + 1)), "value": f["value"].as_str().unwrap_or_default(),
                "is_obligatory": required, "should_be_filled_by_sender": f["partyId"].is_null(), "placements": [placement(f)],
            })),
            _ => {}
        }
    }
    let role = p["role"].as_str().unwrap_or("signer");
    json!({
        "id": p["id"], "user_id": null, "is_author": false,
        "is_signatory": role != "viewer",
        "signatory_role": match role { "viewer" => "viewer", "approver" => "approver", _ => "signing_party" },
        "fields": out_fields,
        "consent_module": null,
        "sign_order": p["signOrder"],
        "sign_time": p["signedAt"], "seen_time": p["viewedAt"], "read_invitation_time": p["viewedAt"],
        "rejected_time": p["declinedAt"], "rejection_reason": p["declineReason"],
        "sign_success_redirect_url": null, "reject_redirect_url": null,
        "email_delivery_status": if p["notifiedAt"].is_null() { "unknown" } else { "delivered" },
        "mobile_delivery_status": "unknown",
        "csv": null,
        "delivery_method": delivery_out(p["delivery"].as_str().unwrap_or_default()),
        "authentication_method_to_view": auth_out(p["authToView"].as_str().unwrap_or("none")),
        "authentication_method_to_sign": auth_out(p["authToSign"].as_str().unwrap_or("standard")),
        "confirmation_delivery_method": "email",
        "allows_highlighting": false,
        "has_authenticated_to_view": !p["viewedAt"].is_null(),
        "api_delivery_url": link.map(|l| l.strip_prefix(app_url).unwrap_or(l).to_string()),
    })
}

async fn document_out(api: &Api, doc: Value) -> Result<Value, AppError> {
    let id = super::uuid(doc["id"].as_str().unwrap_or_default(), "Document")?;
    let links: HashMap<String, String> = api.handed_over_links(id).await?.into_iter().map(|(p, url)| (p.to_string(), url)).collect();
    let app_url = api.ctx.app.public_url();
    let fields = doc["fields"].as_array().cloned().unwrap_or_default();
    let mut parties = vec![json!({
        "id": format!("author-{}", id.simple()), "user_id": doc["ownerId"], "is_author": true, "is_signatory": false,
        "signatory_role": "viewer", "fields": [
            { "type": "name", "order": 1, "value": doc["ownerName"].as_str().map(|n| split_name(n).0).unwrap_or_default(), "placements": [] },
            { "type": "name", "order": 2, "value": doc["ownerName"].as_str().map(|n| split_name(n).1).unwrap_or_default(), "placements": [] },
        ],
        "sign_order": 1, "sign_time": null, "seen_time": null, "rejected_time": null, "delivery_method": "email",
        "authentication_method_to_view": "standard", "authentication_method_to_sign": "standard", "confirmation_delivery_method": "email",
    })];
    for p in doc["parties"].as_array().into_iter().flatten() {
        let mine: Vec<&Value> = fields.iter().filter(|f| f["partyId"] == p["id"]).collect();
        parties.push(party_out(p, &mine, links.get(p["id"].as_str().unwrap_or_default()), &app_url));
    }
    let sender_fields: Vec<&Value> = fields.iter().filter(|f| f["partyId"].is_null()).collect();
    if let Some(author) = parties.first_mut()
        && let Some(list) = author["fields"].as_array_mut()
    {
        for f in sender_fields {
            list.push(json!({ "type": "text", "name": f["label"], "value": f["value"], "is_obligatory": false, "should_be_filled_by_sender": true, "placements": [placement(f)] }));
        }
    }
    let status = doc["status"].as_str().unwrap_or_default();
    let pdf_name = format!("{}.pdf", doc["title"].as_str().unwrap_or("document"));
    let main = doc["files"].as_array().and_then(|f| f.first()).map(|f| json!({ "id": f["id"], "name": f["name"] }));
    let main = main.or_else(|| (status != "draft").then(|| json!({ "id": doc["id"], "name": pdf_name })));
    let sealed = (status == "completed").then(|| json!({ "id": doc["id"], "name": pdf_name }));
    let days_to_sign = doc["expiresAt"]
        .as_str()
        .and_then(|e| e.parse::<chrono::DateTime<Utc>>().ok())
        .map(|e| (e - doc["sentAt"].as_str().and_then(|s| s.parse().ok()).unwrap_or_else(Utc::now)).num_days().max(1))
        .unwrap_or(30);
    Ok(json!({
        "id": doc["id"],
        "title": doc["title"],
        "parties": parties,
        "file": main,
        "sealed_file": sealed,
        "author_attachments": [],
        "ctime": doc["createdAt"],
        "mtime": doc["updatedAt"],
        "timeout_time": doc["expiresAt"],
        "auto_remind_time": null,
        "status": status_out(status),
        "days_to_sign": days_to_sign,
        "days_to_remind": doc["reminderDays"],
        "display_options": { "show_header": true, "show_pdf_download": true, "show_reject_option": true, "show_footer": true },
        "invitation_message": doc["message"],
        "confirmation_message": "",
        "lang": doc["language"],
        "api_callback_url": null,
        "object_version": 1,
        "access_token": null,
        "timezone": "Europe/Stockholm",
        "tags": [],
        "is_template": doc["isTemplate"],
        "is_saved": true,
        "is_shared": false,
        "is_trashed": false,
        "is_deleted": false,
        "viewer": { "role": "company_admin" },
        "experimental_features": {},
    }))
}

/// Translates Scrive's `parties` into OpenSign parties and fields.
async fn apply_parties(api: &Api, id: Uuid, scrive: &[Value], owner: &Value) -> Result<(), AppError> {
    let mut parties = Vec::new();
    let mut placed: Vec<(usize, Value)> = Vec::new();
    let mut sender_fields = Vec::new();
    for party in scrive {
        let role = match party["signatory_role"].as_str() {
            Some("viewer") => "viewer",
            Some("approver") => "approver",
            Some(_) => "signer",
            None if party["is_signatory"].as_bool() == Some(false) => "viewer",
            None => "signer",
        };
        let fields = party["fields"].as_array().cloned().unwrap_or_default();
        let value = |kind: &str, order: Option<i64>| {
            fields
                .iter()
                .find(|f| f["type"] == kind && order.is_none_or(|o| f["order"].as_i64() == Some(o)))
                .and_then(|f| str_of(f, "value"))
                .map(str::to_string)
        };
        if party["is_author"].as_bool() == Some(true) {
            for f in &fields {
                if f["should_be_filled_by_sender"].as_bool() == Some(true) || f["type"] == "text" {
                    sender_fields.push(f.clone());
                }
            }
            if role == "viewer" {
                continue;
            }
        }
        let name = [value("name", Some(1)), value("name", Some(2))].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let name = if name.is_empty() && party["is_author"].as_bool() == Some(true) {
            owner["name"].as_str().unwrap_or_default().to_string()
        } else {
            name
        };
        let id = party["id"].as_str().and_then(|i| Uuid::try_parse(i).ok());
        let index = parties.len();
        parties.push(json!({
            "id": id,
            "role": role,
            "name": name,
            "email": value("email", None),
            "phone": value("mobile", None),
            "personalNumber": value("personal_number", None).filter(|n| !n.contains('•')),
            "company": value("company", None),
            "signOrder": party["sign_order"].as_i64().unwrap_or(1),
            "delivery": delivery_in(party["delivery_method"].as_str())?,
            "authToView": auth_in(party["authentication_method_to_view"].as_str(), true)?,
            "authToSign": auth_in(party["authentication_method_to_sign"].as_str(), false)?,
        }));
        for f in fields {
            placed.push((index, f));
        }
    }
    if parties.iter().filter(|p| p["role"] != "viewer").count() == 0 && !parties.is_empty() {
        return Err(bad_request("At least one party must sign or approve"));
    }
    let created = api.call("documents.set_parties", json!({ "documentId": id, "parties": parties })).await?;
    let created = created.as_array().cloned().unwrap_or_default();
    let mut taken = vec![false; created.len()];
    let mut ids = Vec::new();
    for p in &parties {
        let found = created.iter().enumerate().position(|(i, c)| {
            !taken[i]
                && (p["id"].is_string() && c["id"] == p["id"] || p["id"].is_null() && c["name"] == p["name"] && c["role"] == p["role"])
        });
        if let Some(i) = found {
            taken[i] = true;
        }
        ids.push(found.map(|i| created[i]["id"].clone()).unwrap_or(Value::Null));
    }

    let mut out = Vec::new();
    let mut push = |party: Value, f: &Value, kind: &str, value: Value| {
        for p in f["placements"].as_array().into_iter().flatten() {
            out.push(json!({
                "partyId": party, "kind": kind,
                "page": (p["page"].as_i64().unwrap_or(1) - 1).max(0),
                "x": p["xrel"], "y": p["yrel"],
                "width": p["wrel"].as_f64().filter(|w| *w > 0.0).unwrap_or(0.2),
                "height": p["hrel"].as_f64().filter(|h| *h > 0.0).unwrap_or(if kind == "signature" { 0.06 } else { 0.025 }),
                "required": f["is_obligatory"].as_bool().unwrap_or(kind == "signature"),
                "label": f["name"].as_str().unwrap_or_default(),
                "value": value,
            }));
        }
    };
    let mut radios = Vec::new();
    for (index, f) in &placed {
        let party = ids[*index].clone();
        let (kind, value) = match f["type"].as_str().unwrap_or_default() {
            "signature" => ("signature", Value::Null),
            "checkbox" => ("checkbox", if f["is_checked"].as_bool() == Some(true) { json!("true") } else { Value::Null }),
            "name" if f["order"].as_i64() == Some(1) => ("name", Value::Null),
            "email" => ("email", Value::Null),
            "company" => ("company", Value::Null),
            "mobile" if f["value"].as_str().is_none_or(|v| v.is_empty() || v.starts_with('+')) => {
                ("phone", f.get("value").cloned().unwrap_or(Value::Null))
            }
            "radiogroup" => {
                let options: Vec<&str> = f["values"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                let name =
                    f["name"].as_str().filter(|n| !n.is_empty()).map_or_else(|| format!("radio{}", radios.len() + 1), str::to_string);
                let group = format!("p{index}-{}", name.chars().take(50).collect::<String>());
                for (i, p) in f["placements"].as_array().into_iter().flatten().enumerate() {
                    let Some(option) = options.get(i) else { break };
                    radios.push(json!({
                        "partyId": party, "kind": "radio",
                        "page": (p["page"].as_i64().unwrap_or(1) - 1).max(0),
                        "x": p["xrel"], "y": p["yrel"],
                        "width": p["wrel"].as_f64().filter(|w| *w > 0.0).unwrap_or(0.02),
                        "height": p["hrel"].as_f64().filter(|h| *h > 0.0).unwrap_or(0.015),
                        "required": f["is_obligatory"].as_bool().unwrap_or(true),
                        "label": f["name"].as_str().unwrap_or_default(),
                        "options": [option], "groupKey": group,
                        "value": (f["selected_value"].as_str() == Some(option)).then_some(option),
                    }));
                }
                continue;
            }
            "text" | "personal_number" | "mobile" | "company_number" => {
                ("text", f.get("value").cloned().filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).unwrap_or(Value::Null))
            }
            _ => continue,
        };
        push(party, f, kind, value);
    }
    for f in &sender_fields {
        push(Value::Null, f, "text", f.get("value").cloned().unwrap_or(Value::Null));
    }
    out.extend(radios);
    api.call("documents.set_fields", json!({ "documentId": id, "fields": out })).await?;
    Ok(())
}

async fn body_form(request: Request, state: &AppState) -> Result<(HashMap<String, String>, Option<(String, Vec<u8>)>), AppError> {
    let is_multipart =
        request.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("multipart/"));
    let mut form = HashMap::new();
    let mut file = None;
    if is_multipart {
        let mut multipart =
            Multipart::from_request(request, state).await.map_err(|_| bad_request("The multipart body could not be read"))?;
        while let Some(field) = multipart.next_field().await.map_err(|_| super::too_large())? {
            let name = field.name().unwrap_or_default().to_string();
            if name == "file" {
                let filename = field.file_name().unwrap_or("document.pdf").to_string();
                file = Some((filename, field.bytes().await.map_err(|_| super::too_large())?.to_vec()));
            } else {
                form.insert(name, field.text().await.map_err(|_| bad_request("A form field could not be read"))?);
            }
        }
    } else {
        let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| super::too_large())?;
        form = query_pairs(&String::from_utf8_lossy(&bytes)).into_iter().collect();
    }
    Ok((form, file))
}

async fn start(state: &AppState, request: Request) -> Result<(Api, Request), Box<Response>> {
    let (parts, body) = request.into_parts();
    match Api::new(state, &parts, token(&parts)).await {
        Ok(api) => Ok((api, Request::from_parts(parts, body))),
        Err((e, request_id)) => Err(Box::new(error(e, &request_id))),
    }
}

fn reply(api: &Api, result: Result<Value, AppError>, status: StatusCode) -> Response {
    match result {
        Ok(value) => (status, Json(value)).into_response(),
        Err(e) => error(e, &api.request_id),
    }
}

fn title_from(filename: &str) -> String {
    let stem = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    stem.rsplit_once('.').map(|(s, _)| s).unwrap_or(stem).chars().take(200).collect()
}

async fn new(State(state): State<AppState>, request: Request) -> Response {
    let (api, request) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let (_, file) = body_form(request, &state).await?;
        let title = match &file {
            Some((name, _)) => title_from(name),
            None => "Untitled document".to_string(),
        };
        let doc = api.call("documents.create", json!({ "title": title })).await?;
        let id = uuid(doc["id"].as_str().unwrap_or_default(), "Document")?;
        if let Some((name, bytes)) = file {
            api.upload(id, &name, &bytes).await?;
        }
        document_out(&api, api.document(id).await?).await
    }
    .await;
    reply(&api, result, StatusCode::CREATED)
}

async fn new_from_template(State(state): State<AppState>, Path(template): Path<String>, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let template = uuid(&template, "Template")?;
        let source = api.document(template).await?;
        let doc = api.call("documents.create", json!({ "title": source["title"], "templateId": template })).await?;
        let id = uuid(doc["id"].as_str().unwrap_or_default(), "Document")?;
        document_out(&api, api.document(id).await?).await
    }
    .await;
    reply(&api, result, StatusCode::CREATED)
}

async fn get_document(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async { document_out(&api, api.document(uuid(&id, "Document")?).await?).await }.await;
    reply(&api, result, StatusCode::OK)
}

async fn update(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    let (api, request) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let id = uuid(&id, "Document")?;
        let (form, _) = body_form(request, &state).await?;
        let raw = form.get("document").ok_or_else(|| bad_request("The document parameter is missing"))?;
        let input: Value = serde_json::from_str(raw).map_err(|_| bad_request("The document parameter is not valid JSON"))?;
        let mut patch = Map::new();
        patch.insert("id".into(), json!(id));
        if let Some(title) = str_of(&input, "title") {
            patch.insert("title".into(), json!(title));
        }
        if let Some(message) = input.get("invitation_message").and_then(Value::as_str) {
            patch.insert("message".into(), json!(message));
        }
        if let Some(lang) = str_of(&input, "lang") {
            patch.insert("language".into(), json!(Lang::of(lang).code()));
        }
        if let Some(days) = input.get("days_to_sign").and_then(Value::as_i64) {
            patch.insert("expiresAt".into(), json!(Utc::now() + Duration::days(days.clamp(1, 365))));
        }
        if let Some(days) = input.get("days_to_remind").and_then(Value::as_i64) {
            patch.insert("reminderDays".into(), json!(days.clamp(0, 365)));
        }
        if patch.len() > 1 {
            api.call("documents.update", Value::Object(patch)).await?;
        }
        if let Some(parties) = input.get("parties").and_then(Value::as_array) {
            let me = api.call("account.me", json!({})).await.unwrap_or_default();
            apply_parties(&api, id, parties, &me).await?;
        }
        document_out(&api, api.document(id).await?).await
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

async fn set_file(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    let (api, request) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let id = uuid(&id, "Document")?;
        let (_, file) = body_form(request, &state).await?;
        let doc = api.document(id).await?;
        for f in doc["files"].as_array().into_iter().flatten() {
            api.call("documents.remove_file", json!({ "documentId": id, "fileId": f["id"] })).await?;
        }
        if let Some((name, bytes)) = file {
            api.upload(id, &name, &bytes).await?;
        }
        document_out(&api, api.document(id).await?).await
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

async fn action(state: AppState, id: String, request: Request, service: &'static str, extra: Value) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let id = uuid(&id, "Document")?;
        let mut input = json!({ "id": id });
        if let (Some(target), Some(extra)) = (input.as_object_mut(), extra.as_object()) {
            target.extend(extra.clone());
        }
        api.call(service, input).await?;
        if service == "documents.delete" {
            return Ok(json!({}));
        }
        document_out(&api, api.document(id).await?).await
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

async fn start_signing(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    action(state, id, request, "documents.send", json!({})).await
}

async fn cancel(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    action(state, id, request, "documents.cancel", json!({})).await
}

async fn remind(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    action(state, id, request, "documents.remind", json!({})).await
}

async fn trash(State(state): State<AppState>, Path(id): Path<String>, request: Request) -> Response {
    action(state, id, request, "documents.delete", json!({})).await
}

async fn list(State(state): State<AppState>, RawQuery(query): RawQuery, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let params: HashMap<String, String> = query_pairs(query.as_deref().unwrap_or_default()).into_iter().collect();
        let offset = params.get("offset").and_then(|o| o.parse::<i64>().ok()).unwrap_or(0).max(0);
        let max = params.get("max").and_then(|m| m.parse::<i64>().ok()).unwrap_or(100).clamp(1, 100);
        let filters: Vec<Value> = params.get("filter").and_then(|f| serde_json::from_str(f).ok()).unwrap_or_default();
        let mut input = json!({ "offset": offset, "limit": max });
        for filter in &filters {
            match filter["filter_by"].as_str() {
                Some("status") => {
                    let ours: Vec<&str> = filter["statuses"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|s| match s {
                            "preparation" => "draft",
                            "closed" => "completed",
                            "timedout" => "expired",
                            "rejected" => "declined",
                            other => other,
                        })
                        .collect();
                    if ours.len() > 1 {
                        return Err(bad_request("Filter on one status at a time"));
                    }
                    if let Some(status) = ours.first() {
                        input["status"] = json!(status);
                    }
                }
                Some("text") => input["search"] = filter["text"].clone(),
                Some("template") | Some("is_template") => input["templates"] = json!(true),
                _ => {}
            }
        }
        let summaries = api.call("documents.list", input).await?;
        let mut documents = Vec::new();
        for s in summaries.as_array().into_iter().flatten() {
            let id = uuid(s["id"].as_str().unwrap_or_default(), "Document")?;
            documents.push(document_out(&api, api.document(id).await?).await?);
        }
        let total = offset + documents.len() as i64 + if documents.len() as i64 == max { 1 } else { 0 };
        Ok(json!({ "total_matching": total, "documents": documents }))
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

async fn main_file(State(state): State<AppState>, Path((id, _name)): Path<(String, String)>, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async { api.pdf(uuid(&id, "Document")?).await }.await;
    match result {
        Ok((name, bytes)) => {
            let mut response = Body::from(bytes).into_response();
            let h = response.headers_mut();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
            h.insert(header::CONTENT_DISPOSITION, crate::files::content_disposition(&name));
            h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
            response
        }
        Err(e) => error(e, &api.request_id),
    }
}

pub fn router() -> Router<AppState> {
    let limit = axum::extract::DefaultBodyLimit::max(BODY_LIMIT);
    Router::new()
        .route("/documents/new", post(new).layer(limit))
        .route("/documents/newfromtemplate/{id}", post(new_from_template))
        .route("/documents/list", get(list))
        .route("/documents/{id}/get", get(get_document))
        .route("/documents/{id}/update", post(update).layer(limit))
        .route("/documents/{id}/setfile", post(set_file).layer(limit))
        .route("/documents/{id}/start", post(start_signing))
        .route("/documents/{id}/cancel", post(cancel))
        .route("/documents/{id}/remind", post(remind))
        .route("/documents/{id}/trash", post(trash))
        .route("/documents/{id}/delete", post(trash))
        .route("/documents/{id}/files/main/{name}", get(main_file))
}
