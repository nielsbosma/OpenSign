//! What signers read, in the document's language. Senders and admins use the app in English.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Sv,
}

/// Languages a document can be sent in, as code and name.
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("sv", "Svenska")];

pub enum Stopped<'a> {
    Declined(&'a str),
    Canceled,
}

fn q(title: &str) -> String {
    format!("\u{201c}{title}\u{201d}")
}

impl Lang {
    /// Falls back to English for any language without texts.
    pub fn of(code: &str) -> Self {
        if code == "sv" || code.starts_with("sv-") { Self::Sv } else { Self::En }
    }

    pub fn invitation_subject(self, reminder: bool, sender: &str, title: &str, approve: bool) -> String {
        let title = q(title);
        match (self, approve) {
            (Self::En, false) => format!("{}{sender} sent you {title} to sign", if reminder { "Reminder: " } else { "" }),
            (Self::En, true) => format!("{}{sender} sent you {title} to approve", if reminder { "Reminder: " } else { "" }),
            (Self::Sv, false) => {
                format!("{}{sender} har skickat {title} till dig för signering", if reminder { "Påminnelse: " } else { "" })
            }
            (Self::Sv, true) => {
                format!("{}{sender} har skickat {title} till dig för godkännande", if reminder { "Påminnelse: " } else { "" })
            }
        }
    }

    pub fn invitation(self, sender: &str, org: &str, title: &str, approve: bool) -> String {
        let title = q(title);
        match (self, approve) {
            (Self::En, false) => format!("{sender} at {org} sent you {title} to sign."),
            (Self::En, true) => format!("{sender} at {org} sent you {title} to approve."),
            (Self::Sv, false) => format!("{sender} på {org} har skickat {title} till dig för signering."),
            (Self::Sv, true) => format!("{sender} på {org} har skickat {title} till dig för godkännande."),
        }
    }

    pub fn link_until(self, expires: DateTime<Utc>) -> String {
        let date = expires.format("%Y-%m-%d");
        match self {
            Self::En => format!("The link works until {date}."),
            Self::Sv => format!("Länken fungerar till och med {date}."),
        }
    }

    pub fn open_button(self, approve: bool) -> &'static str {
        match (self, approve) {
            (Self::En, false) => "Review and Sign",
            (Self::En, true) => "Review and Approve",
            (Self::Sv, false) => "Granska och signera",
            (Self::Sv, true) => "Granska och godkänn",
        }
    }

    pub fn personal_link(self) -> &'static str {
        match self {
            Self::En => "This link is personal. Do not forward this email.",
            Self::Sv => "Länken är personlig. Vidarebefordra inte det här mejlet.",
        }
    }

    pub fn invitation_sms(self, org: &str, sender: &str, title: &str, approve: bool, url: &str) -> String {
        let title = q(title);
        match (self, approve) {
            (Self::En, false) => format!("{org}: {sender} asks you to sign {title}: {url}"),
            (Self::En, true) => format!("{org}: {sender} asks you to approve {title}: {url}"),
            (Self::Sv, false) => format!("{org}: {sender} ber dig signera {title}: {url}"),
            (Self::Sv, true) => format!("{org}: {sender} ber dig godkänna {title}: {url}"),
        }
    }

    pub fn code_sms(self, code: &str, title: &str, org: &str) -> String {
        let title = q(title);
        match self {
            Self::En => format!("{code} is your code for {title} from {org}. Never share it."),
            Self::Sv => format!("{code} är din kod för {title} från {org}. Dela den aldrig."),
        }
    }

    pub fn completed_subject(self, title: &str) -> String {
        match self {
            Self::En => format!("Signed: {title}"),
            Self::Sv => format!("Signerat: {title}"),
        }
    }

    pub fn completed(self, title: &str) -> String {
        let title = q(title);
        match self {
            Self::En => format!("{title} is signed by everyone."),
            Self::Sv => format!("{title} är signerat av alla."),
        }
    }

    pub fn completed_attached(self) -> &'static str {
        match self {
            Self::En => "The signed document is attached. It carries an electronic seal and the full signing evidence.",
            Self::Sv => "Det signerade dokumentet är bifogat. Det har en elektronisk försegling och hela underlaget för signeringen.",
        }
    }

    pub fn download_button(self) -> &'static str {
        match self {
            Self::En => "Download Signed Document",
            Self::Sv => "Ladda ner signerat dokument",
        }
    }

    pub fn stopped_subject(self, title: &str) -> String {
        match self {
            Self::En => format!("No longer needed: {title}"),
            Self::Sv => format!("Behövs inte längre: {title}"),
        }
    }

    pub fn stopped(self, why: &Stopped, title: &str) -> String {
        let title = q(title);
        match (self, why) {
            (Self::En, Stopped::Declined(name)) => {
                format!("{name} declined to sign, so {title} will not be completed. You do not need to do anything.")
            }
            (Self::En, Stopped::Canceled) => {
                format!("The sender canceled it, so {title} will not be completed. You do not need to do anything.")
            }
            (Self::Sv, Stopped::Declined(name)) => {
                format!("{name} avböjde att signera, så {title} kommer inte att slutföras. Du behöver inte göra något.")
            }
            (Self::Sv, Stopped::Canceled) => {
                format!("Avsändaren har avbrutit {title}, så det kommer inte att slutföras. Du behöver inte göra något.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_languages_fall_back_to_english() {
        assert_eq!(Lang::of("sv"), Lang::Sv);
        assert_eq!(Lang::of("sv-FI"), Lang::Sv);
        assert_eq!(Lang::of("de"), Lang::En);
        assert_eq!(Lang::Sv.completed_subject("Avtal"), "Signerat: Avtal");
    }
}
