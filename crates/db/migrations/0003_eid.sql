-- One eID identification of a party. Provider secrets and the provider's evidence are encrypted
-- by the application; the broker's `state` is stored only as a MAC.
create table eid_sessions (
  id uuid primary key,
  party_id uuid not null references parties (id) on delete cascade,
  provider text not null check (provider in ('se_bankid', 'freja', 'broker')),
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
