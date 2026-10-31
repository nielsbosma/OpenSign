-- Single sign-on through OpenID Connect. An account is the issuer and subject, never the email.
alter table users add column sso boolean not null default false;

create table user_identities (
  id uuid primary key,
  user_id uuid not null unique references users (id) on delete cascade,
  issuer text not null,
  subject text not null,
  created_at timestamptz not null default now(),
  last_login_at timestamptz,
  unique (issuer, subject)
);

-- One row per sign-in on its way through the provider, taken on the way back.
create table sso_logins (
  id uuid primary key,
  state_mac text not null unique,
  browser_mac text not null,
  secrets text not null,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create index sso_logins_expires_at on sso_logins (expires_at);

-- Null for a session from a local sign-in; otherwise whether the provider's token proved a second factor.
alter table sessions add column sso_mfa boolean;
