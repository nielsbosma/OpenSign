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
  constraint documents_status_check check (status in ('draft', 'pending', 'completed', 'declined', 'canceled', 'expired'))
);
create index documents_owner_idx on documents (owner_id, created_at desc);
create index documents_status_idx on documents (status, created_at desc);
create index documents_external_idx on documents (external_id) where external_id is not null;

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
  constraint parties_role_check check (role in ('signer', 'approver', 'viewer')),
  constraint parties_delivery_check check (delivery in ('email', 'sms', 'link', 'none')),
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
  key text not null,
  api_key_id uuid not null references api_keys (id) on delete cascade,
  request_hash text not null,
  response jsonb not null,
  status integer not null,
  created_at timestamptz not null default now(),
  primary key (api_key_id, key)
);

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
