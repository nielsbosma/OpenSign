-- A user's saved signature, offered (never applied silently) when they sign themselves.
alter table users add column signature_file_id uuid references files (id) on delete set null;
alter table users add column signature_typed text;
alter table users add column signature_initials text;
alter table users add column signature_updated_at timestamptz;

-- A public link to a template. Whoever opens it confirms their email, then becomes `party_id`'s
-- party on a new document owned by `owner_id`.
create table share_links (
  id uuid primary key,
  template_id uuid not null unique references documents (id) on delete cascade,
  slug text not null unique,
  enabled boolean not null default false,
  party_id uuid references parties (id) on delete set null,
  owner_id uuid references users (id) on delete set null,
  per_hour integer not null default 10,
  max_total integer,
  expires_at timestamptz,
  documents_created integer not null default 0,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint share_links_per_hour_check check (per_hour between 1 and 1000),
  constraint share_links_max_total_check check (max_total is null or max_total >= 1)
);

-- Email confirmations started from a share link. Deleted once used, and after a day if not.
create table share_link_requests (
  id uuid primary key,
  share_link_id uuid not null references share_links (id) on delete cascade,
  email text not null,
  name text not null,
  phone text,
  code_hash text not null,
  attempts integer not null default 0,
  ip text,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);
create index share_link_requests_link_idx on share_link_requests (share_link_id, created_at);
create index share_link_requests_email_idx on share_link_requests (lower(email), created_at);

-- Documents made from a share link, for the per-hour and total limits.
alter table documents add column share_link_id uuid references share_links (id) on delete set null;
create index documents_share_link_idx on documents (share_link_id, created_at) where share_link_id is not null;
