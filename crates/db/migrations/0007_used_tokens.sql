-- One-time links (file upload links) that have been spent. Rows go once the link has expired anyway.
create table used_tokens (
  id uuid primary key,
  expires_at timestamptz not null
);
create index used_tokens_expires_idx on used_tokens (expires_at);
