-- The placeholder table from 0001 was never written to; replace it.
drop table passkeys;

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

-- Single-use challenges for passkey ceremonies, stored as a hash.
create table webauthn_challenges (
  id text primary key,
  user_id uuid references users (id) on delete cascade,
  purpose text not null,
  expires_at timestamptz not null,
  constraint webauthn_challenges_purpose_check check (purpose in ('register', 'sign_in'))
);

alter table users add column has_passkey boolean not null default false;
