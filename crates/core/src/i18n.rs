//! What signers read, in their language. Senders and admins use the app in English.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Zh,
    Hi,
    Es,
    Fr,
    Ar,
    Bn,
    Pt,
    Ru,
    Ur,
    Sv,
}

/// Languages a document or party can be sent in, as code and name: the ten most spoken
/// languages in the world, and Swedish.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "中文"),
    ("hi", "हिन्दी"),
    ("es", "Español"),
    ("fr", "Français"),
    ("ar", "العربية"),
    ("bn", "বাংলা"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("ur", "اردو"),
    ("sv", "Svenska"),
];

pub enum Stopped<'a> {
    Declined(&'a str),
    Canceled,
}

struct Texts {
    quotes: (&'static str, &'static str),
    reminder: &'static str,
    subject_sign: &'static str,
    subject_approve: &'static str,
    invitation_sign: &'static str,
    invitation_approve: &'static str,
    link_until: &'static str,
    open_sign: &'static str,
    open_approve: &'static str,
    personal_link: &'static str,
    sms_sign: &'static str,
    sms_approve: &'static str,
    code_sms: &'static str,
    completed_subject: &'static str,
    completed: &'static str,
    completed_attached: &'static str,
    download: &'static str,
    stopped_subject: &'static str,
    stopped_declined: &'static str,
    stopped_canceled: &'static str,
    freja_confirm: &'static str,
}

const EN: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "Reminder: {subject}",
    subject_sign: "{sender} sent you {title} to sign",
    subject_approve: "{sender} sent you {title} to approve",
    invitation_sign: "{sender} at {org} sent you {title} to sign.",
    invitation_approve: "{sender} at {org} sent you {title} to approve.",
    link_until: "The link works until {date}.",
    open_sign: "Review and Sign",
    open_approve: "Review and Approve",
    personal_link: "This link is personal. Do not forward this email.",
    sms_sign: "{org}: {sender} asks you to sign {title}: {url}",
    sms_approve: "{org}: {sender} asks you to approve {title}: {url}",
    code_sms: "{code} is your code for {title} from {org}. Never share it.",
    completed_subject: "Signed: {title}",
    completed: "{title} is signed by everyone.",
    completed_attached: "The signed document is attached. It carries an electronic seal and the full signing evidence.",
    download: "Download Signed Document",
    stopped_subject: "No longer needed: {title}",
    stopped_declined: "{name} declined to sign, so {title} will not be completed. You do not need to do anything.",
    stopped_canceled: "The sender canceled it, so {title} will not be completed. You do not need to do anything.",
    freja_confirm: "Confirm in the Freja eID app.",
};

const ZH: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "提醒：{subject}",
    subject_sign: "{sender} 向您发送了 {title}，请签署",
    subject_approve: "{sender} 向您发送了 {title}，请审批",
    invitation_sign: "{org} 的 {sender} 向您发送了 {title}，请签署。",
    invitation_approve: "{org} 的 {sender} 向您发送了 {title}，请审批。",
    link_until: "此链接有效期至 {date}。",
    open_sign: "查看并签署",
    open_approve: "查看并审批",
    personal_link: "此链接仅供您本人使用，请勿转发此邮件。",
    sms_sign: "{org}：{sender} 请您签署 {title}：{url}",
    sms_approve: "{org}：{sender} 请您审批 {title}：{url}",
    code_sms: "{code} 是您签署 {org} 的 {title} 的验证码。切勿告诉任何人。",
    completed_subject: "已签署：{title}",
    completed: "{title} 已由所有人签署。",
    completed_attached: "已签署的文件见附件。文件带有电子印章和完整的签署证据。",
    download: "下载已签署的文件",
    stopped_subject: "无需再处理：{title}",
    stopped_declined: "{name} 拒绝签署，因此 {title} 将不会完成。您无需进行任何操作。",
    stopped_canceled: "发件人已取消，因此 {title} 将不会完成。您无需进行任何操作。",
    freja_confirm: "请在 Freja eID 应用中确认。",
};

const HI: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "रिमाइंडर: {subject}",
    subject_sign: "{sender} ने आपको हस्ताक्षर के लिए {title} भेजा है",
    subject_approve: "{sender} ने आपको स्वीकृति के लिए {title} भेजा है",
    invitation_sign: "{org} के {sender} ने आपको हस्ताक्षर के लिए {title} भेजा है।",
    invitation_approve: "{org} के {sender} ने आपको स्वीकृति के लिए {title} भेजा है।",
    link_until: "यह लिंक {date} तक काम करेगा।",
    open_sign: "देखें और हस्ताक्षर करें",
    open_approve: "देखें और स्वीकृत करें",
    personal_link: "यह लिंक व्यक्तिगत है। इस ईमेल को आगे न भेजें।",
    sms_sign: "{org}: {sender} आपसे {title} पर हस्ताक्षर करने का अनुरोध करते हैं: {url}",
    sms_approve: "{org}: {sender} आपसे {title} को स्वीकृत करने का अनुरोध करते हैं: {url}",
    code_sms: "{org} के {title} के लिए आपका कोड {code} है। इसे कभी किसी के साथ साझा न करें।",
    completed_subject: "हस्ताक्षरित: {title}",
    completed: "{title} पर सभी ने हस्ताक्षर कर दिए हैं।",
    completed_attached: "हस्ताक्षरित दस्तावेज़ संलग्न है। इसमें इलेक्ट्रॉनिक मुहर और हस्ताक्षर का पूरा साक्ष्य है।",
    download: "हस्ताक्षरित दस्तावेज़ डाउनलोड करें",
    stopped_subject: "अब आवश्यक नहीं: {title}",
    stopped_declined: "{name} ने हस्ताक्षर करने से मना कर दिया है, इसलिए {title} पूरा नहीं होगा। आपको कुछ करने की आवश्यकता नहीं है।",
    stopped_canceled: "भेजने वाले ने इसे रद्द कर दिया है, इसलिए {title} पूरा नहीं होगा। आपको कुछ करने की आवश्यकता नहीं है।",
    freja_confirm: "Freja eID ऐप में पुष्टि करें।",
};

const ES: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "Recordatorio: {subject}",
    subject_sign: "{sender} le ha enviado {title} para firmar",
    subject_approve: "{sender} le ha enviado {title} para aprobar",
    invitation_sign: "{sender}, de {org}, le ha enviado {title} para firmar.",
    invitation_approve: "{sender}, de {org}, le ha enviado {title} para aprobar.",
    link_until: "El enlace funciona hasta el {date}.",
    open_sign: "Revisar y firmar",
    open_approve: "Revisar y aprobar",
    personal_link: "Este enlace es personal. No reenvíe este correo.",
    sms_sign: "{org}: {sender} le pide que firme {title}: {url}",
    sms_approve: "{org}: {sender} le pide que apruebe {title}: {url}",
    code_sms: "{code} es su código para {title} de {org}. No lo comparta nunca.",
    completed_subject: "Firmado: {title}",
    completed: "Todos han firmado {title}.",
    completed_attached: "Se adjunta el documento firmado. Lleva un sello electrónico y todas las pruebas de la firma.",
    download: "Descargar documento firmado",
    stopped_subject: "Ya no es necesario: {title}",
    stopped_declined: "{name} ha rechazado firmar, así que {title} no se completará. No tiene que hacer nada.",
    stopped_canceled: "El remitente lo ha cancelado, así que {title} no se completará. No tiene que hacer nada.",
    freja_confirm: "Confirme en la app Freja eID.",
};

const FR: Texts = Texts {
    quotes: ("\u{ab}\u{a0}", "\u{a0}\u{bb}"),
    reminder: "Rappel\u{a0}: {subject}",
    subject_sign: "{sender} vous a envoyé {title} à signer",
    subject_approve: "{sender} vous a envoyé {title} à approuver",
    invitation_sign: "{sender} ({org}) vous a envoyé {title} à signer.",
    invitation_approve: "{sender} ({org}) vous a envoyé {title} à approuver.",
    link_until: "Le lien est valable jusqu\u{2019}au {date}.",
    open_sign: "Consulter et signer",
    open_approve: "Consulter et approuver",
    personal_link: "Ce lien est personnel. Ne transférez pas cet e-mail.",
    sms_sign: "{org}\u{a0}: {sender} vous invite à signer {title}\u{a0}: {url}",
    sms_approve: "{org}\u{a0}: {sender} vous invite à approuver {title}\u{a0}: {url}",
    code_sms: "{code} est votre code pour {title} de {org}. Ne le communiquez jamais.",
    completed_subject: "Signé\u{a0}: {title}",
    completed: "{title} a été signé par tous.",
    completed_attached: "Le document signé est joint. Il porte un cachet électronique et l\u{2019}ensemble des preuves de signature.",
    download: "Télécharger le document signé",
    stopped_subject: "Plus nécessaire\u{a0}: {title}",
    stopped_declined: "{name} a refusé de signer, donc {title} ne sera pas finalisé. Vous n\u{2019}avez rien à faire.",
    stopped_canceled: "L\u{2019}expéditeur l\u{2019}a annulé, donc {title} ne sera pas finalisé. Vous n\u{2019}avez rien à faire.",
    freja_confirm: "Confirmez dans l\u{2019}application Freja eID.",
};

const AR: Texts = Texts {
    quotes: ("\u{ab}", "\u{bb}"),
    reminder: "تذكير: {subject}",
    subject_sign: "وصلك المستند {title} من {sender} لتوقيعه",
    subject_approve: "وصلك المستند {title} من {sender} للموافقة عليه",
    invitation_sign: "وصلك المستند {title} من {sender} في {org} لتوقيعه.",
    invitation_approve: "وصلك المستند {title} من {sender} في {org} للموافقة عليه.",
    link_until: "يعمل الرابط حتى {date}.",
    open_sign: "مراجعة وتوقيع",
    open_approve: "مراجعة وموافقة",
    personal_link: "هذا الرابط شخصي. لا تُعِد توجيه هذه الرسالة.",
    sms_sign: "{org}: مطلوب منك توقيع {title} (المرسل: {sender}): {url}",
    sms_approve: "{org}: مطلوب منك الموافقة على {title} (المرسل: {sender}): {url}",
    code_sms: "{code} هو رمزك للمستند {title} من {org}. لا تشاركه مع أي أحد.",
    completed_subject: "تم التوقيع: {title}",
    completed: "وقّع الجميع على {title}.",
    completed_attached: "المستند الموقّع مرفق. يحمل ختمًا إلكترونيًا والأدلة الكاملة على التوقيع.",
    download: "تنزيل المستند الموقّع",
    stopped_subject: "لم يعد مطلوبًا: {title}",
    stopped_declined: "تم رفض التوقيع من قِبل {name}، لذلك لن يكتمل {title}. لا يلزمك فعل أي شيء.",
    stopped_canceled: "ألغى المرسل {title}، لذلك لن يكتمل. لا يلزمك فعل أي شيء.",
    freja_confirm: "أكّد في تطبيق Freja eID.",
};

const BN: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "অনুস্মারক: {subject}",
    subject_sign: "{sender} আপনাকে স্বাক্ষরের জন্য {title} পাঠিয়েছেন",
    subject_approve: "{sender} আপনাকে অনুমোদনের জন্য {title} পাঠিয়েছেন",
    invitation_sign: "{org}-এর {sender} আপনাকে স্বাক্ষরের জন্য {title} পাঠিয়েছেন।",
    invitation_approve: "{org}-এর {sender} আপনাকে অনুমোদনের জন্য {title} পাঠিয়েছেন।",
    link_until: "লিংকটি {date} পর্যন্ত কাজ করবে।",
    open_sign: "দেখুন ও স্বাক্ষর করুন",
    open_approve: "দেখুন ও অনুমোদন করুন",
    personal_link: "এই লিংকটি ব্যক্তিগত। এই ইমেইলটি অন্য কাউকে ফরোয়ার্ড করবেন না।",
    sms_sign: "{org}: {sender} আপনাকে {title} স্বাক্ষর করতে অনুরোধ করেছেন: {url}",
    sms_approve: "{org}: {sender} আপনাকে {title} অনুমোদন করতে অনুরোধ করেছেন: {url}",
    code_sms: "{org}-এর {title}-এর জন্য আপনার কোড {code}। এটি কখনো কাউকে জানাবেন না।",
    completed_subject: "স্বাক্ষরিত: {title}",
    completed: "সবাই {title}-এ স্বাক্ষর করেছেন।",
    completed_attached: "স্বাক্ষরিত নথিটি সংযুক্ত করা হয়েছে। এতে একটি ইলেকট্রনিক সিল এবং স্বাক্ষরের সম্পূর্ণ প্রমাণ রয়েছে।",
    download: "স্বাক্ষরিত নথি ডাউনলোড করুন",
    stopped_subject: "আর প্রয়োজন নেই: {title}",
    stopped_declined: "{name} স্বাক্ষর করতে অস্বীকার করেছেন, তাই {title} সম্পন্ন হবে না। আপনাকে কিছু করতে হবে না।",
    stopped_canceled: "প্রেরক এটি বাতিল করেছেন, তাই {title} সম্পন্ন হবে না। আপনাকে কিছু করতে হবে না।",
    freja_confirm: "Freja eID অ্যাপে নিশ্চিত করুন।",
};

const PT: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "Lembrete: {subject}",
    subject_sign: "{sender} enviou {title} para você assinar",
    subject_approve: "{sender} enviou {title} para você aprovar",
    invitation_sign: "{sender}, de {org}, enviou {title} para você assinar.",
    invitation_approve: "{sender}, de {org}, enviou {title} para você aprovar.",
    link_until: "O link funciona até {date}.",
    open_sign: "Revisar e assinar",
    open_approve: "Revisar e aprovar",
    personal_link: "Este link é pessoal. Não encaminhe este e-mail.",
    sms_sign: "{org}: {sender} pede que você assine {title}: {url}",
    sms_approve: "{org}: {sender} pede que você aprove {title}: {url}",
    code_sms: "{code} é o seu código para {title} de {org}. Nunca o compartilhe.",
    completed_subject: "Assinado: {title}",
    completed: "{title} foi assinado por todos.",
    completed_attached: "O documento assinado está em anexo. Ele tem um selo eletrônico e todas as evidências da assinatura.",
    download: "Baixar documento assinado",
    stopped_subject: "Não é mais necessário: {title}",
    stopped_declined: "{name} recusou a assinatura, então {title} não será concluído. Você não precisa fazer nada.",
    stopped_canceled: "O remetente cancelou, então {title} não será concluído. Você não precisa fazer nada.",
    freja_confirm: "Confirme no app Freja eID.",
};

const RU: Texts = Texts {
    quotes: ("\u{ab}", "\u{bb}"),
    reminder: "Напоминание: {subject}",
    subject_sign: "{title} от {sender} ждёт вашей подписи",
    subject_approve: "{title} от {sender} ждёт вашего согласования",
    invitation_sign: "{sender} ({org}) просит вас подписать документ {title}.",
    invitation_approve: "{sender} ({org}) просит вас согласовать документ {title}.",
    link_until: "Ссылка действует до {date}.",
    open_sign: "Просмотреть и подписать",
    open_approve: "Просмотреть и согласовать",
    personal_link: "Это персональная ссылка. Не пересылайте это письмо.",
    sms_sign: "{org}: {sender} просит вас подписать {title}: {url}",
    sms_approve: "{org}: {sender} просит вас согласовать {title}: {url}",
    code_sms: "{code} — ваш код для документа {title} от {org}. Никому его не сообщайте.",
    completed_subject: "Подписано: {title}",
    completed: "Документ {title} подписан всеми сторонами.",
    completed_attached: "Подписанный документ во вложении. Он заверен электронной печатью и содержит все доказательства подписания.",
    download: "Скачать подписанный документ",
    stopped_subject: "Больше не требуется: {title}",
    stopped_declined: "Участник {name} отказался от подписания, поэтому документ {title} не будет завершён. Вам ничего не нужно делать.",
    stopped_canceled: "Отправитель отменил документ {title}, поэтому он не будет завершён. Вам ничего не нужно делать.",
    freja_confirm: "Подтвердите в приложении Freja eID.",
};

const UR: Texts = Texts {
    quotes: ("\u{ab}", "\u{bb}"),
    reminder: "یاد دہانی: {subject}",
    subject_sign: "{sender} نے آپ کو دستخط کے لیے {title} بھیجا ہے",
    subject_approve: "{sender} نے آپ کو منظوری کے لیے {title} بھیجا ہے",
    invitation_sign: "{org} کے {sender} نے آپ کو دستخط کے لیے {title} بھیجا ہے۔",
    invitation_approve: "{org} کے {sender} نے آپ کو منظوری کے لیے {title} بھیجا ہے۔",
    link_until: "یہ لنک {date} تک کام کرے گا۔",
    open_sign: "دیکھیں اور دستخط کریں",
    open_approve: "دیکھیں اور منظور کریں",
    personal_link: "یہ لنک ذاتی ہے۔ یہ ای میل کسی اور کو نہ بھیجیں۔",
    sms_sign: "{org}: {sender} آپ سے {title} پر دستخط کی درخواست کرتے ہیں: {url}",
    sms_approve: "{org}: {sender} آپ سے {title} کی منظوری کی درخواست کرتے ہیں: {url}",
    code_sms: "{org} کی جانب سے {title} کے لیے آپ کا کوڈ {code} ہے۔ اسے کبھی کسی کو نہ بتائیں۔",
    completed_subject: "دستخط شدہ: {title}",
    completed: "سب نے {title} پر دستخط کر دیے ہیں۔",
    completed_attached: "دستخط شدہ دستاویز منسلک ہے۔ اس پر الیکٹرانک مہر اور دستخط کے مکمل شواہد موجود ہیں۔",
    download: "دستخط شدہ دستاویز ڈاؤن لوڈ کریں",
    stopped_subject: "اب ضرورت نہیں: {title}",
    stopped_declined: "{name} نے دستخط کرنے سے انکار کر دیا ہے، اس لیے {title} مکمل نہیں ہوگا۔ آپ کو کچھ کرنے کی ضرورت نہیں۔",
    stopped_canceled: "بھیجنے والے نے اسے منسوخ کر دیا ہے، اس لیے {title} مکمل نہیں ہوگا۔ آپ کو کچھ کرنے کی ضرورت نہیں۔",
    freja_confirm: "Freja eID ایپ میں تصدیق کریں۔",
};

const SV: Texts = Texts {
    quotes: ("\u{201c}", "\u{201d}"),
    reminder: "Påminnelse: {subject}",
    subject_sign: "{sender} har skickat {title} till dig för signering",
    subject_approve: "{sender} har skickat {title} till dig för godkännande",
    invitation_sign: "{sender} på {org} har skickat {title} till dig för signering.",
    invitation_approve: "{sender} på {org} har skickat {title} till dig för godkännande.",
    link_until: "Länken fungerar till och med {date}.",
    open_sign: "Granska och signera",
    open_approve: "Granska och godkänn",
    personal_link: "Länken är personlig. Vidarebefordra inte det här mejlet.",
    sms_sign: "{org}: {sender} ber dig signera {title}: {url}",
    sms_approve: "{org}: {sender} ber dig godkänna {title}: {url}",
    code_sms: "{code} är din kod för {title} från {org}. Dela den aldrig.",
    completed_subject: "Signerat: {title}",
    completed: "{title} är signerat av alla.",
    completed_attached: "Det signerade dokumentet är bifogat. Det har en elektronisk försegling och hela underlaget för signeringen.",
    download: "Ladda ner signerat dokument",
    stopped_subject: "Behövs inte längre: {title}",
    stopped_declined: "{name} avböjde att signera, så {title} kommer inte att slutföras. Du behöver inte göra något.",
    stopped_canceled: "Avsändaren har avbrutit {title}, så det kommer inte att slutföras. Du behöver inte göra något.",
    freja_confirm: "Bekräfta i Freja eID-appen.",
};

/// Replaces `{name}` placeholders in one pass, so a value that itself contains a placeholder
/// (a title like "{url}") is never expanded.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}').and_then(|end| values.iter().find(|(k, _)| *k == &after[..end]).map(|(_, v)| (end, v))) {
            Some((end, value)) => {
                out.push_str(value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

impl Lang {
    pub const ALL: &'static [Lang] =
        &[Lang::En, Lang::Zh, Lang::Hi, Lang::Es, Lang::Fr, Lang::Ar, Lang::Bn, Lang::Pt, Lang::Ru, Lang::Ur, Lang::Sv];

    /// A supported language from a code such as `sv`, `pt-BR` or `zh_CN`.
    pub fn parse(code: &str) -> Option<Self> {
        let base = code.trim().split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
        Self::ALL.iter().copied().find(|l| l.code() == base)
    }

    /// Falls back to English for any language without texts.
    pub fn of(code: &str) -> Self {
        Self::parse(code).unwrap_or(Self::En)
    }

    /// The party's own language if set, otherwise the document's.
    pub fn for_party(party: Option<&str>, document: &str) -> Self {
        party.and_then(Self::parse).unwrap_or_else(|| Self::of(document))
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
            Self::Hi => "hi",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::Ar => "ar",
            Self::Bn => "bn",
            Self::Pt => "pt",
            Self::Ru => "ru",
            Self::Ur => "ur",
            Self::Sv => "sv",
        }
    }

    pub fn rtl(self) -> bool {
        matches!(self, Self::Ar | Self::Ur)
    }

    fn texts(self) -> &'static Texts {
        match self {
            Self::En => &EN,
            Self::Zh => &ZH,
            Self::Hi => &HI,
            Self::Es => &ES,
            Self::Fr => &FR,
            Self::Ar => &AR,
            Self::Bn => &BN,
            Self::Pt => &PT,
            Self::Ru => &RU,
            Self::Ur => &UR,
            Self::Sv => &SV,
        }
    }

    fn q(self, title: &str) -> String {
        let (open, close) = self.texts().quotes;
        format!("{open}{title}{close}")
    }

    pub fn invitation_subject(self, reminder: bool, sender: &str, title: &str, approve: bool) -> String {
        let t = self.texts();
        let subject = fill(if approve { t.subject_approve } else { t.subject_sign }, &[("sender", sender), ("title", &self.q(title))]);
        if reminder { fill(t.reminder, &[("subject", &subject)]) } else { subject }
    }

    pub fn invitation(self, sender: &str, org: &str, title: &str, approve: bool) -> String {
        let t = self.texts();
        fill(if approve { t.invitation_approve } else { t.invitation_sign }, &[("sender", sender), ("org", org), ("title", &self.q(title))])
    }

    pub fn link_until(self, expires: DateTime<Utc>) -> String {
        fill(self.texts().link_until, &[("date", &expires.format("%Y-%m-%d").to_string())])
    }

    pub fn open_button(self, approve: bool) -> &'static str {
        if approve { self.texts().open_approve } else { self.texts().open_sign }
    }

    pub fn personal_link(self) -> &'static str {
        self.texts().personal_link
    }

    pub fn invitation_sms(self, org: &str, sender: &str, title: &str, approve: bool, url: &str) -> String {
        let t = self.texts();
        fill(if approve { t.sms_approve } else { t.sms_sign }, &[("org", org), ("sender", sender), ("title", &self.q(title)), ("url", url)])
    }

    pub fn code_sms(self, code: &str, title: &str, org: &str) -> String {
        fill(self.texts().code_sms, &[("code", code), ("title", &self.q(title)), ("org", org)])
    }

    pub fn completed_subject(self, title: &str) -> String {
        fill(self.texts().completed_subject, &[("title", title)])
    }

    pub fn completed(self, title: &str) -> String {
        fill(self.texts().completed, &[("title", &self.q(title))])
    }

    pub fn completed_attached(self) -> &'static str {
        self.texts().completed_attached
    }

    pub fn download_button(self) -> &'static str {
        self.texts().download
    }

    pub fn stopped_subject(self, title: &str) -> String {
        fill(self.texts().stopped_subject, &[("title", title)])
    }

    pub fn stopped(self, why: &Stopped, title: &str) -> String {
        let t = self.texts();
        match why {
            Stopped::Declined(name) => fill(t.stopped_declined, &[("name", name), ("title", &self.q(title))]),
            Stopped::Canceled => fill(t.stopped_canceled, &[("title", &self.q(title))]),
        }
    }

    pub fn freja_confirm(self) -> &'static str {
        self.texts().freja_confirm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_languages_fall_back_to_english() {
        assert_eq!(Lang::of("sv"), Lang::Sv);
        assert_eq!(Lang::of("sv-FI"), Lang::Sv);
        assert_eq!(Lang::of("pt_BR"), Lang::Pt);
        assert_eq!(Lang::of("de"), Lang::En);
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::Sv.completed_subject("Avtal"), "Signerat: Avtal");
    }

    #[test]
    fn every_language_is_listed() {
        assert_eq!(LANGUAGES.len(), Lang::ALL.len());
        for (lang, (code, _)) in Lang::ALL.iter().zip(LANGUAGES) {
            assert_eq!(lang.code(), *code);
        }
        assert!(Lang::Ar.rtl() && Lang::Ur.rtl() && !Lang::Hi.rtl());
    }

    #[test]
    fn party_language_wins_over_the_document() {
        assert_eq!(Lang::for_party(Some("ar"), "sv"), Lang::Ar);
        assert_eq!(Lang::for_party(None, "sv"), Lang::Sv);
        assert_eq!(Lang::for_party(Some("xx"), "fr"), Lang::Fr);
    }

    #[test]
    fn placeholders_in_values_are_not_expanded() {
        let sms = Lang::En.invitation_sms("Acme", "{url}", "{code}", false, "https://x");
        assert_eq!(sms, "Acme: {url} asks you to sign \u{201c}{code}\u{201d}: https://x");
    }

    #[test]
    fn every_text_fills_its_placeholders() {
        let at = chrono::TimeZone::with_ymd_and_hms(&Utc, 2030, 1, 2, 0, 0, 0).unwrap();
        for &lang in Lang::ALL {
            let texts = [
                lang.invitation_subject(true, "S", "T", false),
                lang.invitation_subject(false, "S", "T", true),
                lang.invitation("S", "O", "T", false),
                lang.invitation("S", "O", "T", true),
                lang.link_until(at),
                lang.invitation_sms("O", "S", "T", false, "U"),
                lang.invitation_sms("O", "S", "T", true, "U"),
                lang.code_sms("123456", "T", "O"),
                lang.completed_subject("T"),
                lang.completed("T"),
                lang.stopped_subject("T"),
                lang.stopped(&Stopped::Declined("N"), "T"),
                lang.stopped(&Stopped::Canceled, "T"),
            ];
            for text in texts {
                assert!(!text.contains('{') && !text.contains('}'), "{lang:?}: {text}");
            }
        }
    }
}
