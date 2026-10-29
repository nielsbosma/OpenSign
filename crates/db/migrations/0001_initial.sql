-- The whole schema, squashed before the first release. Add changes as new numbered files after this one.

-- Identity, settings, audit chain, files, jobs and OAuth for MCP.

create table users (
  id uuid primary key,
  email text not null,
  name text not null,
  password_hash text,
  role text not null,
  active boolean not null default true,
  totp_secret text,
  totp_pending_secret text,
  totp_enabled_at timestamptz,
  totp_last_step bigint,
  failed_login_count integer not null default 0,
  locked_until timestamptz,
  last_login_at timestamptz,
  password_changed_at timestamptz not null default now(),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  has_passkey boolean not null default false,
  language text check (language in ('en', 'zh', 'hi', 'es', 'fr', 'ar', 'bn', 'pt', 'ru', 'ur', 'sv'))
);
create unique index users_email_unique on users (lower(email));

create table recovery_codes (
  id uuid primary key,
  user_id uuid not null references users (id) on delete cascade,
  code_hash text not null,
  used_at timestamptz,
  created_at timestamptz not null default now()
);
create index recovery_codes_user_idx on recovery_codes (user_id);

-- WebAuthn credentials. Only the public key is stored; the private key never leaves the authenticator.
create table passkeys (
  id uuid primary key,
  user_id uuid not null references users (id) on delete cascade,
  credential_id bytea not null unique,
  public_key bytea not null,
  sign_count bigint not null default 0,
  name text not null,
  transports text[] not null default '{}',
  created_at timestamptz not null default now(),
  last_used_at timestamptz
);
create index passkeys_user_idx on passkeys (user_id);

-- id is sha256 of the cookie token; the token itself is never stored.
create table sessions (
  id text primary key,
  user_id uuid not null references users (id) on delete cascade,
  expires_at timestamptz not null,
  last_seen_at timestamptz not null default now(),
  elevated_until timestamptz,
  ip text,
  user_agent text,
  created_at timestamptz not null default now()
);
create index sessions_user_idx on sessions (user_id);

create table login_challenges (
  id text primary key,
  user_id uuid not null references users (id) on delete cascade,
  attempts integer not null default 0,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create table password_reset_tokens (
  id text primary key,
  user_id uuid not null references users (id) on delete cascade,
  expires_at timestamptz not null,
  used_at timestamptz,
  created_at timestamptz not null default now()
);

create table invites (
  id uuid primary key,
  email text not null,
  name text,
  role text not null,
  token_hash text not null unique,
  invited_by uuid references users (id) on delete set null,
  expires_at timestamptz not null,
  accepted_at timestamptz,
  accepted_user_id uuid references users (id) on delete set null,
  revoked_at timestamptz,
  created_at timestamptz not null default now()
);
create index invites_email_idx on invites (lower(email));

create table api_keys (
  id uuid primary key,
  name text not null,
  prefix text not null unique,
  key_hash text not null unique,
  user_id uuid not null references users (id) on delete cascade,
  permissions text[] not null,
  allowed_ips text[] not null default '{}',
  expires_at timestamptz,
  last_used_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now()
);
create index api_keys_user_idx on api_keys (user_id);

-- One row per settings group. Secret fields live in `secrets`, each encrypted with a key derived from APP_SECRET.
create table settings (
  "group" text primary key,
  "values" jsonb not null default '{}',
  secrets jsonb not null default '{}',
  updated_by uuid references users (id) on delete set null,
  updated_at timestamptz not null default now()
);

-- Append-only. Each row's hash covers the previous row's hash, so removing or editing a row breaks
-- the chain from that point on; anchors timestamp the chain head with an external TSA.
create table audit_log (
  id bigserial primary key,
  occurred_at timestamptz not null default now(),
  actor_id uuid,
  actor_label text not null,
  surface text not null,
  action text not null,
  entity_type text not null,
  entity_id text,
  before jsonb,
  after jsonb,
  metadata jsonb,
  prev_hash bytea not null,
  hash bytea not null unique,
  -- Chained with an HMAC keyed from APP_SECRET, so someone who can write to the database cannot rebuild the chain.
  hash_alg text not null default 'hmac-sha256' check (hash_alg in ('sha256', 'hmac-sha256'))
);
create index audit_log_entity_idx on audit_log (entity_type, entity_id);
create index audit_log_actor_idx on audit_log (actor_id);
create index audit_log_occurred_idx on audit_log (occurred_at);
create index audit_log_action_idx on audit_log (action);

create function refuse_change() returns trigger language plpgsql as $$
begin
  raise exception '% is append-only', tg_table_name using errcode = 'insufficient_privilege';
end;
$$;
create trigger audit_log_append_only before update or delete on audit_log for each row execute function refuse_change();
create trigger audit_log_no_truncate before truncate on audit_log for each statement execute function refuse_change();

create table audit_anchors (
  id bigserial primary key,
  audit_id bigint not null references audit_log (id),
  head_hash bytea not null,
  tsa_token bytea,
  tsa_url text,
  created_at timestamptz not null default now()
);
create trigger audit_anchors_append_only before update or delete on audit_anchors for each row execute function refuse_change();

-- Bodies live in the bucket, encrypted with a per-file key; wrapped_key is that key wrapped by the KEK.
create table files (
  id uuid primary key,
  kind text not null,
  name text not null,
  content_type text not null,
  size_bytes bigint not null,
  sha256 text not null,
  storage_key text not null unique,
  wrapped_key text not null,
  uploaded_by uuid references users (id) on delete set null,
  created_at timestamptz not null default now(),
  deleted_at timestamptz
);
create index files_kind_idx on files (kind, created_at);

create table notifications (
  id uuid primary key,
  user_id uuid not null references users (id) on delete cascade,
  type text not null,
  title text not null,
  body text not null default '',
  link text,
  read_at timestamptz,
  created_at timestamptz not null default now()
);
create index notifications_user_idx on notifications (user_id, created_at);

-- A small job queue: workers claim rows with FOR UPDATE SKIP LOCKED.
create table jobs (
  id uuid primary key,
  kind text not null,
  payload jsonb not null default '{}',
  run_at timestamptz not null default now(),
  attempts integer not null default 0,
  max_attempts integer not null default 8,
  locked_until timestamptz,
  last_error text,
  done_at timestamptz,
  unique_key text,
  created_at timestamptz not null default now()
);
create index jobs_due_idx on jobs (run_at) where done_at is null;
create unique index jobs_unique_key on jobs (unique_key) where done_at is null and unique_key is not null;

create table oauth_clients (
  client_id text primary key,
  name text not null,
  redirect_uris text[] not null,
  metadata jsonb not null,
  created_at timestamptz not null default now()
);

create table oauth_codes (
  id text primary key,
  client_id text not null references oauth_clients (client_id) on delete cascade,
  user_id uuid not null references users (id) on delete cascade,
  code_challenge text not null,
  redirect_uri text not null,
  scopes text[] not null,
  resource text,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create table oauth_grants (
  id uuid primary key,
  client_id text not null references oauth_clients (client_id) on delete cascade,
  user_id uuid not null references users (id) on delete cascade,
  scopes text[] not null,
  resource text,
  access_token_hash text not null unique,
  refresh_token_hash text not null unique,
  -- Refresh tokens already rotated away; seeing one again revokes the grant.
  previous_refresh_hashes text[] not null default '{}',
  access_expires_at timestamptz not null,
  refresh_expires_at timestamptz not null,
  last_used_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz not null default now()
);
create index oauth_grants_user_idx on oauth_grants (user_id);
create index oauth_grants_previous_idx on oauth_grants using gin (previous_refresh_hashes);

-- Single-use challenges for passkey ceremonies, stored as a hash.
create table webauthn_challenges (
  id text primary key,
  user_id uuid references users (id) on delete cascade,
  purpose text not null,
  expires_at timestamptz not null,
  constraint webauthn_challenges_purpose_check check (purpose in ('register', 'sign_in'))
);

-- Documents to sign, their parties and fields, templates and webhooks.

create table documents (
  id uuid primary key,
  title text not null,
  status text not null default 'draft',
  is_template boolean not null default false,
  template_id uuid references documents (id) on delete set null,
  owner_id uuid references users (id) on delete set null,
  message text not null default '',
  language text not null default 'en',
  sequential boolean not null default false,
  expires_at timestamptz,
  reminder_days integer,
  source_file_id uuid references files (id),
  sealed_file_id uuid references files (id),
  original_sha256 text,
  sealed_sha256 text,
  page_count integer,
  external_id text,
  metadata jsonb not null default '{}',
  sent_at timestamptz,
  completed_at timestamptz,
  canceled_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  -- A legal hold keeps a document until it is released, whatever the retention rules say.
  legal_hold boolean not null default false,
  legal_hold_reason text,
  legal_hold_at timestamptz,
  legal_hold_by uuid references users (id) on delete set null,
  review_links boolean not null default true,
  constraint documents_status_check check (status in ('draft', 'pending', 'completed', 'declined', 'canceled', 'expired'))
);
create index documents_owner_idx on documents (owner_id, created_at desc);
create index documents_status_idx on documents (status, created_at desc);
create index documents_external_idx on documents (external_id) where external_id is not null;
create index documents_retention_idx on documents (status, updated_at) where not legal_hold and not is_template;

-- A document's source files, merged into one PDF in this order when it is sent.
create table document_files (
  document_id uuid not null references documents (id) on delete cascade,
  file_id uuid not null references files (id),
  position integer not null,
  primary key (document_id, file_id)
);

create table document_access (
  document_id uuid not null references documents (id) on delete cascade,
  user_id uuid not null references users (id) on delete cascade,
  can_edit boolean not null default false,
  primary key (document_id, user_id)
);

create table parties (
  id uuid primary key,
  document_id uuid not null references documents (id) on delete cascade,
  role text not null default 'signer',
  name text not null,
  email text,
  phone text,
  -- Encrypted: a national id number to match against the eID result.
  personal_number text,
  company text,
  sign_order integer not null default 1,
  delivery text not null default 'email',
  auth_to_view text not null default 'none',
  auth_to_sign text not null default 'standard',
  status text not null default 'waiting',
  -- Signer links are MACs over the party id and this version; bumping it revokes every link.
  link_version integer not null default 0,
  link_expires_at timestamptz,
  notified_at timestamptz,
  viewed_at timestamptz,
  signed_at timestamptz,
  declined_at timestamptz,
  decline_reason text,
  signature jsonb,
  evidence jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  language text check (language in ('en', 'zh', 'hi', 'es', 'fr', 'ar', 'bn', 'pt', 'ru', 'ur', 'sv')),
  constraint parties_role_check check (role in ('signer', 'approver', 'viewer')),
  constraint parties_delivery_check check (delivery in ('email', 'sms', 'link', 'in_person', 'none')),
  constraint parties_status_check check (status in ('waiting', 'notified', 'viewed', 'signed', 'declined', 'canceled'))
);
create index parties_document_idx on parties (document_id, sign_order);

create table fields (
  id uuid primary key,
  document_id uuid not null references documents (id) on delete cascade,
  party_id uuid references parties (id) on delete cascade,
  kind text not null,
  page integer not null,
  -- Position and size as fractions of the page, origin top-left.
  x double precision not null,
  y double precision not null,
  width double precision not null,
  height double precision not null,
  required boolean not null default true,
  label text not null default '',
  value text,
  options jsonb not null default '[]',
  created_at timestamptz not null default now(),
  constraint fields_box_check check (x >= 0 and y >= 0 and width > 0 and height > 0 and x + width <= 1.0001 and y + height <= 1.0001)
);
create index fields_document_idx on fields (document_id);

-- The organisation's document seal. The private key is encrypted with a key derived from APP_SECRET.
create table seal_identities (
  id uuid primary key,
  private_key text not null,
  -- DER certificates, base64, the seal's own first.
  chain text[] not null,
  subject text not null,
  not_after timestamptz not null,
  active boolean not null default true,
  created_by uuid references users (id) on delete set null,
  created_at timestamptz not null default now()
);
create unique index seal_identities_active on seal_identities (active) where active;

create table sms_challenges (
  id uuid primary key,
  party_id uuid not null references parties (id) on delete cascade,
  purpose text not null,
  code_hash text not null,
  attempts integer not null default 0,
  expires_at timestamptz not null,
  verified_at timestamptz,
  created_at timestamptz not null default now()
);

create table webhooks (
  id uuid primary key,
  url text not null,
  -- Encrypted.
  secret text not null,
  events text[] not null,
  active boolean not null default true,
  created_by uuid references users (id) on delete set null,
  created_at timestamptz not null default now()
);

create table webhook_deliveries (
  id uuid primary key,
  webhook_id uuid not null references webhooks (id) on delete cascade,
  event text not null,
  payload jsonb not null,
  status_code integer,
  error text,
  attempts integer not null default 0,
  delivered_at timestamptz,
  created_at timestamptz not null default now()
);
create index webhook_deliveries_webhook_idx on webhook_deliveries (webhook_id, created_at desc);

create table idempotency_keys (
  actor text not null,
  key text not null,
  request_sha256 bytea not null,
  status integer,
  response jsonb,
  created_at timestamptz not null default now(),
  primary key (actor, key)
);
create index idempotency_keys_created on idempotency_keys (created_at);

-- AI reviews of a document's text, kept so the editor can show the latest one.
create table ai_reviews (
  id uuid primary key,
  document_id uuid not null references documents (id) on delete cascade,
  created_by uuid references users (id) on delete set null,
  model text not null,
  result jsonb not null,
  created_at timestamptz not null default now()
);
create index ai_reviews_document_idx on ai_reviews (document_id, created_at desc);

-- Tokens spent per call, for the monthly budget and the Admin's overview.
create table ai_usage (
  id bigserial primary key,
  user_id uuid references users (id) on delete set null,
  kind text not null,
  model text not null,
  input_tokens integer not null,
  output_tokens integer not null,
  created_at timestamptz not null default now()
);
create index ai_usage_created_idx on ai_usage (created_at);

-- A sealed document cannot change: only its row's bookkeeping may.
create function protect_sealed_document() returns trigger language plpgsql as $$
begin
  if old.status = 'completed' and (new.sealed_file_id is distinct from old.sealed_file_id
      or new.sealed_sha256 is distinct from old.sealed_sha256
      or new.original_sha256 is distinct from old.original_sha256
      or new.source_file_id is distinct from old.source_file_id
      or new.status is distinct from old.status) then
    raise exception 'a completed document cannot be changed' using errcode = 'insufficient_privilege';
  end if;
  return new;
end;
$$;
create trigger documents_protect_sealed before update on documents for each row execute function protect_sealed_document();

-- One eID identification of a party. Provider secrets and the provider's evidence are encrypted
-- by the application; the broker's `state` is stored only as a MAC.
create table eid_sessions (
  id uuid primary key,
  party_id uuid not null references parties (id) on delete cascade,
  provider text not null check (provider in ('se_bankid', 'freja', 'broker', 'tic')),
  purpose text not null check (purpose in ('view', 'sign')),
  reference text,
  state_mac text unique,
  secrets text,
  status text not null default 'pending' check (status in ('pending', 'complete', 'failed')),
  message text,
  identity jsonb,
  evidence text,
  expires_at timestamptz not null,
  completed_at timestamptz,
  consumed_at timestamptz,
  created_at timestamptz not null default now()
);

create index eid_sessions_party on eid_sessions (party_id, created_at);

-- Files a party is asked to upload while signing, such as a copy of an ID or a certificate.
-- They are listed in the evidence and embedded in the sealed PDF.
create table attachment_requests (
  id uuid primary key,
  document_id uuid not null references documents (id) on delete cascade,
  party_id uuid not null references parties (id) on delete cascade,
  position integer not null default 0,
  name text not null,
  description text not null default '',
  required boolean not null default true,
  file_id uuid references files (id),
  uploaded_at timestamptz,
  created_at timestamptz not null default now(),
  -- 'photo' is a live photo taken with the camera while signing.
  kind text not null default 'file',
  constraint attachment_requests_upload_check check ((file_id is null) = (uploaded_at is null)),
  constraint attachment_requests_kind_check check (kind in ('file', 'photo'))
);
create index attachment_requests_document_idx on attachment_requests (document_id, party_id, position);
create unique index attachment_requests_one_photo_idx on attachment_requests (party_id) where kind = 'photo';

-- One template sent to many recipients from a spreadsheet. Rows are encrypted: they hold personal data.
create table bulk_sends (
  id uuid primary key,
  template_id uuid references documents (id) on delete set null,
  template_title text not null,
  owner_id uuid references users (id) on delete set null,
  file_name text not null,
  mapping jsonb not null,
  title_pattern text not null default '',
  status text not null default 'queued',
  total integer not null,
  sent integer not null default 0,
  failed integer not null default 0,
  created_at timestamptz not null default now(),
  finished_at timestamptz,
  constraint bulk_sends_status_check check (status in ('queued', 'running', 'done', 'canceled'))
);
create index bulk_sends_owner_idx on bulk_sends (owner_id, created_at desc);

create table bulk_rows (
  bulk_id uuid not null references bulk_sends (id) on delete cascade,
  row_number integer not null,
  label text not null default '',
  data text,
  status text not null default 'queued',
  document_id uuid references documents (id) on delete set null,
  error text,
  primary key (bulk_id, row_number),
  constraint bulk_rows_status_check check (status in ('queued', 'sent', 'failed', 'canceled'))
);

-- Read-only links a signer can share with their own AI agent.
create table review_links (
    id uuid primary key,
    party_id uuid not null references parties (id) on delete cascade,
    document_id uuid not null references documents (id) on delete cascade,
    secret_hash text not null,
    link_version integer not null,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null,
    revoked_at timestamptz,
    last_used_at timestamptz,
    uses integer not null default 0 check (uses >= 0)
);

create index review_links_party on review_links (party_id) where revoked_at is null;
create index review_links_document on review_links (document_id) where revoked_at is null;
