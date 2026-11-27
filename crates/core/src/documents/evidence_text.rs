//! The wording of the verification page appended to a sealed PDF, in the document's language.
//! The page is drawn with the standard PDF fonts, which cover Latin script only, so languages in
//! other scripts keep English. evidence.json always stays English, for the tools that read it.

use opensign_pdf::EvidenceLabels;
use serde_json::Value;

use super::{AuthMethod, PartyRole};
use crate::i18n::Lang;

const SV: EvidenceLabels = EvidenceLabels {
    heading: "Verifiering",
    document: "Dokument {id} · slutfört {at}",
    verify_at: "Verifiera dokumentet på {url}",
    parties: "Parter",
    identified_by: "Identifierad med: {method}",
    signed: "Signerade {at}",
    signed_from: "Signerade {at} från {ip}",
    attachments: "Bilagor",
    attachment: "{name} från {party}: {file}",
    attachments_note: "Filerna är inbäddade i denna PDF. Öppna bilagepanelen i din PDF-läsare för att se dem.",
    events: "Händelser",
    integrity: "Integritet",
    original_sha256: "SHA-256 för originaldokumentet: {hash}",
    signed_sha256: "SHA-256 för det signerade innehållet före denna sida: {hash}",
    audit_head: "Granskningsloggens kedjehuvud: {hash}",
    seal_note: "Filen är förseglad med en elektronisk stämpel (PAdES), med en tidsstämpel när tidsstämpeltjänsten svarade. Varje ändring efter förseglingen gör stämpeln ogiltig. Hela bevisningen är bifogad som evidence.json.",
    footer: "OpenSign-verifiering · sida {page} av {total}",
};

/// The page's language: the document's, when the page can show it.
pub fn page_lang(document_language: &str) -> Lang {
    match Lang::of(document_language) {
        Lang::Sv => Lang::Sv,
        _ => Lang::En,
    }
}

pub fn labels(lang: Lang) -> EvidenceLabels {
    match lang {
        Lang::Sv => SV,
        _ => EvidenceLabels::EN,
    }
}

pub fn role(lang: Lang, role: PartyRole) -> &'static str {
    match (lang, role) {
        (Lang::Sv, PartyRole::Signer) => "Undertecknare",
        (Lang::Sv, PartyRole::Approver) => "Godkännare",
        (Lang::Sv, PartyRole::Viewer) => "Får en kopia",
        (_, PartyRole::Signer) => "Signer",
        (_, PartyRole::Approver) => "Approver",
        (_, PartyRole::Viewer) => "Receives a copy",
    }
}

/// How a party identified; `account` when they signed in to OpenSign instead.
pub fn method(lang: Lang, method: AuthMethod, account: bool) -> &'static str {
    if lang != Lang::Sv {
        return if account { "Signed-in OpenSign account" } else { method.label() };
    }
    if account {
        return "Inloggat OpenSign-konto";
    }
    match method {
        AuthMethod::None | AuthMethod::Standard => "E-postlänk",
        AuthMethod::SmsPin => "SMS-kod",
        AuthMethod::SeBankid => "Svenskt BankID",
        AuthMethod::Freja => "Freja eID",
        AuthMethod::Broker => "e-legitimation",
    }
}

/// One line of the event log, or none for an action the log leaves out.
pub fn event(lang: Lang, action: &str, actor: &str, metadata: &Value) -> Option<String> {
    let sv = lang == Lang::Sv;
    let name = metadata["name"].as_str().unwrap_or(actor);
    let ip = metadata["ip"].as_str().map(|ip| if sv { format!(" från {ip}") } else { format!(" from {ip}") }).unwrap_or_default();
    let str_or = |key: &str, en: &'static str, sv_text: &'static str| -> String {
        metadata[key].as_str().filter(|v| !v.is_empty()).map(str::to_string).unwrap_or_else(|| (if sv { sv_text } else { en }).to_string())
    };
    Some(match (action, sv) {
        ("document.created", false) => format!("Created by {actor}"),
        ("document.created", true) => format!("Skapat av {actor}"),
        ("document.sent", false) => format!("Sent for signing by {actor}"),
        ("document.sent", true) => format!("Skickat för signering av {actor}"),
        ("party.notified", false) => format!("Invitation sent to {name} by {}", metadata["channel"].as_str().unwrap_or("email")),
        ("party.notified", true) => format!(
            "Inbjudan skickad till {name} via {}",
            match metadata["channel"].as_str().unwrap_or("email") {
                "email" => "e-post",
                "sms" => "sms",
                other => other,
            }
        ),
        ("party.reminded", false) => format!("Reminder sent to {name}"),
        ("party.reminded", true) => format!("Påminnelse skickad till {name}"),
        ("party.viewed", false) => format!("Opened by {name}{ip}"),
        ("party.viewed", true) => format!("Öppnat av {name}{ip}"),
        ("party.in_person_started", false) => format!("In-person signing for {name} started by {actor} on their device"),
        ("party.in_person_started", true) => format!("Signering på plats för {name} startad av {actor} på dennes enhet"),
        ("party.link_issued", false) => format!("Signing link for {name} handed out by {actor}"),
        ("party.link_issued", true) => format!("Signeringslänk för {name} utlämnad av {actor}"),
        ("party.authenticated", _) => {
            let method = metadata["method"].as_str().and_then(AuthMethod::parse).map(|m| self::method(lang, m, false));
            if sv {
                format!("{name} identifierade sig med {}", method.unwrap_or("en kod"))
            } else {
                format!("{name} identified with {}", method.unwrap_or("a code"))
            }
        }
        ("party.signed", false) => format!("Signed by {name}{ip}"),
        ("party.signed", true) => format!("Signerat av {name}{ip}"),
        ("party.approved", false) => format!("Approved by {name}{ip}"),
        ("party.approved", true) => format!("Godkänt av {name}{ip}"),
        ("party.photo_taken", false) => format!("{name} took a webcam photo (SHA-256 {})", metadata["sha256"].as_str().unwrap_or("")),
        ("party.photo_taken", true) => {
            format!("{name} tog ett foto med webbkameran (SHA-256 {})", metadata["sha256"].as_str().unwrap_or(""))
        }
        ("party.field_file_uploaded", false) => {
            format!("{name} uploaded {} to \u{201c}{}\u{201d}", str_or("file", "a file", ""), str_or("label", "a field", ""))
        }
        ("party.field_file_uploaded", true) => {
            format!("{name} laddade upp {} till \u{201d}{}\u{201d}", str_or("file", "", "en fil"), str_or("label", "", "ett fält"))
        }
        ("party.attachment_uploaded", false) => {
            format!("{name} uploaded {} ({})", str_or("attachment", "a file", ""), metadata["file"].as_str().unwrap_or(""))
        }
        ("party.attachment_uploaded", true) => {
            format!("{name} laddade upp {} ({})", str_or("attachment", "", "en fil"), metadata["file"].as_str().unwrap_or(""))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn swedish_documents_get_a_swedish_page_and_others_english() {
        assert_eq!(page_lang("sv"), Lang::Sv);
        assert_eq!(page_lang("ar"), Lang::En);
        assert_eq!(labels(Lang::Sv).heading, "Verifiering");
        assert_eq!(labels(Lang::Fr).heading, "Verification");
        assert_eq!(role(Lang::Sv, PartyRole::Signer), "Undertecknare");
        assert_eq!(method(Lang::Sv, AuthMethod::SeBankid, false), "Svenskt BankID");
        assert_eq!(method(Lang::En, AuthMethod::Standard, true), "Signed-in OpenSign account");
    }

    #[test]
    fn events_read_in_both_languages() {
        let signed = json!({ "name": "Anna", "ip": "192.0.2.1" });
        assert_eq!(event(Lang::En, "party.signed", "x", &signed).as_deref(), Some("Signed by Anna from 192.0.2.1"));
        assert_eq!(event(Lang::Sv, "party.signed", "x", &signed).as_deref(), Some("Signerat av Anna från 192.0.2.1"));
        let auth = json!({ "name": "Anna", "method": "se_bankid" });
        assert_eq!(event(Lang::Sv, "party.authenticated", "x", &auth).as_deref(), Some("Anna identifierade sig med Svenskt BankID"));
        assert_eq!(event(Lang::En, "party.uploaded_upload", "x", &auth), None);
        let file = json!({ "name": "Anna" });
        assert_eq!(
            event(Lang::En, "party.field_file_uploaded", "x", &file).as_deref(),
            Some("Anna uploaded a file to \u{201c}a field\u{201d}")
        );
        assert_eq!(
            event(Lang::Sv, "party.field_file_uploaded", "x", &file).as_deref(),
            Some("Anna laddade upp en fil till \u{201d}ett fält\u{201d}")
        );
    }
}
