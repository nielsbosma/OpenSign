//! DocuSign eSignature REST API v2.1 (`/restapi/v2.1/accounts/{accountId}/...`). Point the base
//! path at OpenSign and use an API key as the access token. The account id is accepted and ignored.
//! Tabs are placed by position; anchor text is refused with a clear error rather than misplaced.

use std::collections::HashMap;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, RawQuery, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use opensign_core::error::{AppError, ErrorKind, bad_request};
use opensign_core::i18n::Lang;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Api, str_of, uuid};
use crate::AppState;
use crate::rest::query_pairs;

const BODY_LIMIT: usize = 60 * 1024 * 1024;

fn error(e: AppError, request_id: &str) -> Response {
    let (status, code) = match e.kind {
        ErrorKind::BadRequest => (StatusCode::BAD_REQUEST, "INVALID_REQUEST_PARAMETER"),
        ErrorKind::Unauthorized => (StatusCode::UNAUTHORIZED, "USER_AUTHENTICATION_FAILED"),
        ErrorKind::Forbidden => (StatusCode::FORBIDDEN, "USER_LACKS_PERMISSIONS"),
        ErrorKind::NotFound => (StatusCode::NOT_FOUND, "ENVELOPE_DOES_NOT_EXIST"),
        ErrorKind::Conflict => (StatusCode::BAD_REQUEST, "ENVELOPE_INVALID_STATUS"),
        ErrorKind::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "HOURLY_APIINVOCATION_LIMIT_EXCEEDED"),
        ErrorKind::Unavailable | ErrorKind::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "UNSPECIFIED_ERROR"),
    };
    let message = super::log_internal(&e, request_id);
    (status, Json(json!({ "errorCode": code, "message": message }))).into_response()
}

fn token(request: &Request) -> Option<String> {
    let value = request.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|t| t.trim().to_string())
}

async fn start(state: &AppState, request: Request) -> Result<(Api, Request), Box<Response>> {
    let bearer = token(&request);
    let (parts, body) = request.into_parts();
    match Api::new(state, &parts, bearer).await {
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

async fn json_body(request: Request) -> Result<Value, AppError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT).await.map_err(|_| super::too_large())?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(json!({}));
    }
    serde_json::from_slice(&bytes).map_err(|_| bad_request("The body is not valid JSON"))
}

fn envelope_status(doc: &Value) -> &'static str {
    match doc["status"].as_str().unwrap_or_default() {
        "draft" => "created",
        "pending" => {
            let viewed = doc["parties"].as_array().into_iter().flatten().any(|p| !p["viewedAt"].is_null());
            if viewed { "delivered" } else { "sent" }
        }
        "completed" => "completed",
        "declined" => "declined",
        "canceled" | "expired" => "voided",
        _ => "created",
    }
}

fn recipient_status(p: &Value) -> &'static str {
    match p["status"].as_str().unwrap_or_default() {
        "signed" => "completed",
        "declined" => "declined",
        "viewed" => "delivered",
        "notified" => "sent",
        _ => "created",
    }
}

/// DocuSign recipient ids are the client's own strings; OpenSign keeps them in the metadata.
fn recipient_ids(doc: &Value) -> HashMap<String, String> {
    doc["metadata"]["docusign"]["recipients"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
        .unwrap_or_default()
}

fn envelope_out(doc: &Value) -> Value {
    let id = &doc["id"];
    let voided = matches!(doc["status"].as_str(), Some("canceled" | "expired"));
    json!({
        "envelopeId": id,
        "status": envelope_status(doc),
        "emailSubject": doc["title"],
        "emailBlurb": doc["message"],
        "createdDateTime": doc["createdAt"],
        "lastModifiedDateTime": doc["updatedAt"],
        "sentDateTime": doc["sentAt"],
        "completedDateTime": doc["completedAt"],
        "voidedDateTime": if voided { doc["canceledAt"].clone() } else { Value::Null },
        "declinedDateTime": doc["parties"].as_array().into_iter().flatten().find_map(|p| p["declinedAt"].as_str().map(str::to_string)),
        "expireDateTime": doc["expiresAt"],
        "statusChangedDateTime": doc["updatedAt"],
        "envelopeUri": format!("/envelopes/{}", id.as_str().unwrap_or_default()),
        "documentsUri": format!("/envelopes/{}/documents", id.as_str().unwrap_or_default()),
        "recipientsUri": format!("/envelopes/{}/recipients", id.as_str().unwrap_or_default()),
        "customFields": { "textCustomFields": doc["externalId"].as_str().map(|e| vec![json!({ "name": "externalId", "value": e })]).unwrap_or_default() },
        "is21CFRPart11": "false",
        "signingLocation": "online",
    })
}

fn recipients_out(doc: &Value) -> Value {
    let ids = recipient_ids(doc);
    let mut signers = Vec::new();
    let mut copies = Vec::new();
    for (n, p) in doc["parties"].as_array().into_iter().flatten().enumerate() {
        let party_id = p["id"].as_str().unwrap_or_default();
        let r = json!({
            "recipientId": ids.get(party_id).cloned().unwrap_or_else(|| (n + 1).to_string()),
            "recipientIdGuid": party_id,
            "name": p["name"],
            "email": p["email"],
            "routingOrder": p["signOrder"].to_string(),
            "status": recipient_status(p),
            "deliveredDateTime": p["viewedAt"],
            "sentDateTime": p["notifiedAt"],
            "signedDateTime": p["signedAt"],
            "declinedDateTime": p["declinedAt"],
            "declinedReason": p["declineReason"],
            "recipientType": if p["role"] == "viewer" { "carboncopy" } else { "signer" },
        });
        if p["role"] == "viewer" { copies.push(r) } else { signers.push(r) }
    }
    json!({ "signers": signers, "carbonCopies": copies, "recipientCount": (signers.len() + copies.len()).to_string() })
}

struct Tab<'a> {
    tab: &'a Value,
    kind: &'static str,
    width: f64,
    height: f64,
}

fn tabs_of(recipient: &Value) -> Vec<Tab<'_>> {
    let kinds: [(&str, &'static str, f64, f64); 9] = [
        ("signHereTabs", "signature", 150.0, 40.0),
        ("initialHereTabs", "initials", 60.0, 30.0),
        ("dateSignedTabs", "date", 100.0, 18.0),
        ("fullNameTabs", "name", 150.0, 18.0),
        ("emailAddressTabs", "email", 180.0, 18.0),
        ("companyTabs", "company", 150.0, 18.0),
        ("textTabs", "text", 150.0, 18.0),
        ("checkboxTabs", "checkbox", 14.0, 14.0),
        ("dateTabs", "text", 100.0, 18.0),
    ];
    let mut out = Vec::new();
    for (key, kind, width, height) in kinds {
        for tab in recipient["tabs"][key].as_array().into_iter().flatten() {
            out.push(Tab { tab, kind, width, height });
        }
    }
    out
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

async fn create(State(state): State<AppState>, Path(_account): Path<String>, request: Request) -> Response {
    let (api, request) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let body = json_body(request).await?;
        if body.get("templateId").is_some() || body.get("compositeTemplates").is_some() {
            return Err(bad_request("Templates are not supported through this API yet; send documents instead"));
        }
        let title = str_of(&body, "emailSubject").unwrap_or("Please sign");
        let doc = api
            .call(
                "documents.create",
                json!({ "title": title, "message": body["emailBlurb"].as_str().unwrap_or_default(), "sequential": true }),
            )
            .await?;
        let id = uuid(doc["id"].as_str().unwrap_or_default(), "Envelope")?;
        let outcome = fill(&api, id, &body).await;
        if outcome.is_err() {
            let _ = api.call("documents.delete", json!({ "id": id })).await;
        }
        outcome?;
        if body["status"] == "sent" {
            api.call("documents.send", json!({ "id": id })).await?;
        }
        let doc = api.document(id).await?;
        Ok(json!({ "envelopeId": id, "status": envelope_status(&doc), "statusDateTime": doc["updatedAt"], "uri": format!("/envelopes/{id}") }))
    }
    .await;
    reply(&api, result, StatusCode::CREATED)
}

/// Uploads the documents, then adds recipients and their tabs.
async fn fill(api: &Api, id: Uuid, body: &Value) -> Result<(), AppError> {
    let mut documents: Vec<&Value> = body["documents"].as_array().map(|d| d.iter().collect()).unwrap_or_default();
    if documents.is_empty() {
        return Err(bad_request("An envelope needs at least one document"));
    }
    documents.sort_by_key(|d| d["order"].as_str().and_then(|o| o.parse::<i64>().ok()).or(d["order"].as_i64()).unwrap_or(0));
    let mut first_page: HashMap<String, usize> = HashMap::new();
    let mut page = 0;
    for d in &documents {
        let encoded = str_of(d, "documentBase64").ok_or_else(|| bad_request("Each document needs documentBase64"))?;
        let bytes = B64.decode(encoded).map_err(|_| bad_request("documentBase64 is not valid base64"))?;
        let ext = str_of(d, "fileExtension").unwrap_or("pdf");
        let name = str_of(d, "name").unwrap_or("document");
        let name = if name.contains('.') { name.to_string() } else { format!("{name}.{ext}") };
        let uploaded = api.upload(id, &name, &bytes).await?;
        let key = d["documentId"].as_str().map(str::to_string).unwrap_or_else(|| d["documentId"].to_string());
        first_page.insert(key, page);
        page += uploaded["pages"].as_u64().unwrap_or(1) as usize;
    }
    let (_, combined) = opensign_core::services::documents::preview_pdf(&api.ctx, id).await?;
    let sizes = super::page_sizes(&combined)?;

    let recipients = &body["recipients"];
    let mut parties = Vec::new();
    let mut tab_sets = Vec::new();
    let mut ids = Vec::new();
    for (group, role) in
        [("signers", "signer"), ("inPersonSigners", "signer"), ("certifiedDeliveries", "viewer"), ("carbonCopies", "viewer")]
    {
        for r in recipients[group].as_array().into_iter().flatten() {
            let name = str_of(r, "name").or_else(|| str_of(r, "signerName")).ok_or_else(|| bad_request("Each recipient needs a name"))?;
            let sms = r.get("smsAuthentication").is_some() || r.get("phoneAuthentication").is_some();
            let phone = r["smsAuthentication"]["senderProvidedNumbers"][0].as_str().map(str::to_string).or_else(|| {
                r["phoneNumber"]["number"].as_str().map(|n| format!("+{}{}", r["phoneNumber"]["countryCode"].as_str().unwrap_or(""), n))
            });
            if r.get("identityVerification").is_some() || r.get("idCheckConfigurationName").is_some() {
                return Err(bad_request(
                    "identityVerification is not supported here; use smsAuthentication, or set BankID or Freja on the party in OpenSign",
                ));
            }
            let embedded = r.get("clientUserId").is_some_and(|c| !c.is_null());
            parties.push(json!({
                "role": role,
                "name": name,
                "email": str_of(r, "email"),
                "phone": phone,
                "signOrder": number(&r["routingOrder"]).map(|o| o as i64).unwrap_or(1),
                "delivery": if embedded { "link" } else { "email" },
                "authToView": if sms { "sms_pin" } else { "none" },
                "authToSign": if sms { "sms_pin" } else { "standard" },
                "language": r["emailNotification"]["supportedLanguage"].as_str().and_then(Lang::parse).map(Lang::code),
            }));
            ids.push(r["recipientId"].as_str().map(str::to_string).unwrap_or_else(|| r["recipientId"].to_string()));
            tab_sets.push(tabs_of(r));
        }
    }
    if parties.is_empty() {
        return Err(bad_request("An envelope needs at least one recipient"));
    }
    let created = api.call("documents.set_parties", json!({ "documentId": id, "parties": parties })).await?;
    let created = created.as_array().cloned().unwrap_or_default();
    let mut taken = vec![false; created.len()];
    let mut party_ids = Vec::new();
    for p in &parties {
        let i = created
            .iter()
            .enumerate()
            .position(|(i, c)| !taken[i] && c["name"] == p["name"] && c["role"] == p["role"])
            .ok_or_else(|| AppError::new(ErrorKind::Internal, "a recipient went missing"))?;
        taken[i] = true;
        party_ids.push(created[i]["id"].clone());
    }

    let mut fields = Vec::new();
    for (party, tabs) in party_ids.iter().zip(&tab_sets) {
        for Tab { tab, kind, width, height } in tabs {
            if tab.get("anchorString").is_some_and(|a| !a.is_null()) {
                return Err(bad_request(
                    "Anchor tabs are not supported yet; place tabs with documentId, pageNumber, xPosition and yPosition",
                ));
            }
            let doc_key = tab["documentId"].as_str().map(str::to_string).unwrap_or_else(|| tab["documentId"].to_string());
            let start = *first_page
                .get(&doc_key)
                .ok_or_else(|| bad_request(format!("A tab names documentId {doc_key}, which is not in the envelope")))?;
            let page_number = number(&tab["pageNumber"]).unwrap_or(1.0).max(1.0) as usize;
            let index = start + page_number - 1;
            let (pw, ph) =
                *sizes.get(index).ok_or_else(|| bad_request(format!("A tab is on page {page_number}, past the end of its document")))?;
            let w = number(&tab["width"]).filter(|w| *w > 0.0).unwrap_or(*width).min(pw);
            let h = number(&tab["height"]).filter(|h| *h > 0.0).unwrap_or(*height).min(ph);
            let x = number(&tab["xPosition"]).unwrap_or(0.0).clamp(0.0, pw - w);
            let y = number(&tab["yPosition"]).unwrap_or(0.0).clamp(0.0, ph - h);
            let required = tab["required"].as_str().map(|r| r != "false").or(tab["required"].as_bool()).unwrap_or(*kind != "checkbox");
            let value = match *kind {
                "checkbox" => (tab["selected"] == "true" || tab["selected"] == true).then(|| json!("true")),
                "text" => str_of(tab, "value").map(|v| json!(v)),
                _ => None,
            };
            fields.push(json!({
                "partyId": party, "kind": kind, "page": index,
                "x": x / pw, "y": y / ph, "width": w / pw, "height": h / ph,
                "required": required,
                "label": str_of(tab, "tabLabel").unwrap_or_default(),
                "value": value,
            }));
        }
    }
    api.call("documents.set_fields", json!({ "documentId": id, "fields": fields })).await?;
    let map: serde_json::Map<String, Value> =
        party_ids.iter().zip(ids).filter_map(|(p, r)| p.as_str().map(|p| (p.to_string(), Value::String(r)))).collect();
    let mut metadata = api.document(id).await?["metadata"].clone();
    if !metadata.is_object() {
        metadata = json!({});
    }
    metadata["docusign"] = json!({ "recipients": map });
    if let Some(custom) = body["customFields"]["textCustomFields"].as_array() {
        for f in custom {
            if let (Some(name), Some(value)) = (str_of(f, "name"), str_of(f, "value")) {
                metadata["docusign"]["customFields"][name] = json!(value);
            }
        }
    }
    api.call("documents.update", json!({ "id": id, "metadata": metadata })).await?;
    Ok(())
}

async fn get_envelope(State(state): State<AppState>, Path((_account, id)): Path<(String, String)>, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async { Ok(envelope_out(&api.document(uuid(&id, "Envelope")?).await?)) }.await;
    reply(&api, result, StatusCode::OK)
}

async fn update_envelope(State(state): State<AppState>, Path((_account, id)): Path<(String, String)>, request: Request) -> Response {
    let (api, request) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let id = uuid(&id, "Envelope")?;
        let body = json_body(request).await?;
        match body["status"].as_str() {
            Some("voided") => {
                let reason = str_of(&body, "voidedReason").ok_or_else(|| bad_request("voidedReason is required to void an envelope"))?;
                api.call("documents.cancel", json!({ "id": id, "reason": reason })).await?;
            }
            Some("sent") => {
                api.call("documents.send", json!({ "id": id })).await?;
            }
            Some(other) => return Err(bad_request(format!("Changing the status to {other} is not supported"))),
            None => {
                if let Some(subject) = str_of(&body, "emailSubject") {
                    api.call("documents.update", json!({ "id": id, "title": subject })).await?;
                }
            }
        }
        Ok(json!({ "envelopeId": id }))
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

async fn list_envelopes(
    State(state): State<AppState>,
    Path(_account): Path<String>,
    RawQuery(query): RawQuery,
    request: Request,
) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let params: HashMap<String, String> = query_pairs(query.as_deref().unwrap_or_default()).into_iter().collect();
        let start = params.get("start_position").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0).max(0);
        let count = params.get("count").and_then(|c| c.parse::<i64>().ok()).unwrap_or(100).clamp(1, 100);
        let mut input = json!({ "offset": start, "limit": count });
        if let Some(status) = params.get("status").map(String::as_str) {
            let ours = match status {
                "created" => "draft",
                "sent" | "delivered" => "pending",
                "completed" => "completed",
                "declined" => "declined",
                "voided" => "canceled",
                "any" | "" => "",
                other => return Err(bad_request(format!("Status {other} is not supported here"))),
            };
            if !ours.is_empty() {
                input["status"] = json!(ours);
            }
        }
        if let Some(text) = params.get("search_text") {
            input["search"] = json!(text);
        }
        let from = params.get("from_date").and_then(|d| d.parse::<chrono::DateTime<chrono::Utc>>().ok());
        let summaries = api.call("documents.list", input).await?;
        let mut envelopes = Vec::new();
        for s in summaries.as_array().into_iter().flatten() {
            if let (Some(from), Some(created)) =
                (from, s["createdAt"].as_str().and_then(|c| c.parse::<chrono::DateTime<chrono::Utc>>().ok()))
                && created < from
            {
                continue;
            }
            let doc = api.document(uuid(s["id"].as_str().unwrap_or_default(), "Envelope")?).await?;
            envelopes.push(envelope_out(&doc));
        }
        let size = envelopes.len() as i64;
        Ok(json!({
            "envelopes": envelopes,
            "resultSetSize": size.to_string(),
            "startPosition": start.to_string(),
            "endPosition": (start + size - 1).max(start).to_string(),
            "totalSetSize": (start + size + if size == count { 1 } else { 0 }).to_string(),
        }))
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

async fn recipients(State(state): State<AppState>, Path((_account, id)): Path<(String, String)>, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async { Ok(recipients_out(&api.document(uuid(&id, "Envelope")?).await?)) }.await;
    reply(&api, result, StatusCode::OK)
}

async fn documents(State(state): State<AppState>, Path((_account, id)): Path<(String, String)>, request: Request) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let doc = api.document(uuid(&id, "Envelope")?).await?;
        let list: Vec<Value> = doc["files"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(n, f)| json!({ "documentId": (n + 1).to_string(), "documentIdGuid": f["id"], "name": f["name"], "type": "content", "order": (n + 1).to_string() }))
            .collect();
        Ok(json!({ "envelopeId": doc["id"], "envelopeDocuments": list }))
    }
    .await;
    reply(&api, result, StatusCode::OK)
}

/// Any document id returns the whole envelope as one PDF: sealed with the evidence pages once
/// complete. Single parts are not served separately after sending.
async fn document_pdf(
    State(state): State<AppState>,
    Path((_account, id, _document)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let (api, _) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match async { api.pdf(uuid(&id, "Envelope")?).await }.await {
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

/// Embedded signing: the recipient must have been added with a clientUserId. `returnUrl` is
/// followed only when its site is allowed to embed; otherwise the signing page shows its own
/// confirmation.
async fn recipient_view(State(state): State<AppState>, Path((_account, id)): Path<(String, String)>, request: Request) -> Response {
    let (api, request) = match start(&state, request).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let result = async {
        let id = uuid(&id, "Envelope")?;
        let body = json_body(request).await?;
        let doc = api.document(id).await?;
        let ids = recipient_ids(&doc);
        let wanted_email = str_of(&body, "email").map(str::to_lowercase);
        let wanted_name = str_of(&body, "userName");
        let wanted_recipient = str_of(&body, "recipientId");
        let party = doc["parties"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| {
                let pid = p["id"].as_str().unwrap_or_default();
                match wanted_recipient {
                    Some(r) => ids.get(pid).map(String::as_str) == Some(r),
                    None => {
                        p["email"].as_str().map(str::to_lowercase) == wanted_email
                            && wanted_name.is_none_or(|n| p["name"].as_str() == Some(n))
                    }
                }
            })
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "No such recipient in this envelope"))?;
        if party["delivery"] != "link" {
            return Err(bad_request("The recipient is not embedded; add them with a clientUserId"));
        }
        let return_url = str_of(&body, "returnUrl");
        let input = |redirect: Option<&str>| json!({ "documentId": id, "partyId": party["id"], "redirectUrl": redirect });
        let link = match api.call("documents.signing_link", input(return_url)).await {
            Err(e) if return_url.is_some() && e.kind == ErrorKind::BadRequest => api.call("documents.signing_link", input(None)).await?,
            other => other?,
        };
        Ok(json!({ "url": link["url"] }))
    }
    .await;
    reply(&api, result, StatusCode::CREATED)
}

pub fn router() -> Router<AppState> {
    let limit = axum::extract::DefaultBodyLimit::max(BODY_LIMIT);
    Router::new()
        .route("/accounts/{account}/envelopes", post(create).get(list_envelopes).layer(limit))
        .route("/accounts/{account}/envelopes/{id}", get(get_envelope).put(update_envelope))
        .route("/accounts/{account}/envelopes/{id}/recipients", get(recipients))
        .route("/accounts/{account}/envelopes/{id}/documents", get(documents))
        .route("/accounts/{account}/envelopes/{id}/documents/{document}", get(document_pdf))
        .route("/accounts/{account}/envelopes/{id}/views/recipient", post(recipient_view))
}
