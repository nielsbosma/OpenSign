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
  updated_at timestamptz not null default now()
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

create table passkeys (
  id uuid primary key,
  user_id uuid not null references users (id) on delete cascade,
  credential_id bytea not null unique,
  passkey jsonb not null,
  name text not null,
  last_used_at timestamptz,
  created_at timestamptz not null default now()
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
  hash bytea not null unique
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
