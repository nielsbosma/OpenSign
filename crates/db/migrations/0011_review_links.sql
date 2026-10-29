alter table documents add column review_links boolean not null default true;

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
