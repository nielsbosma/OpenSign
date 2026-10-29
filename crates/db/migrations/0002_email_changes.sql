create table email_change_tokens (
  id text primary key,
  user_id uuid not null references users (id) on delete cascade,
  new_email text not null,
  requested_by uuid references users (id) on delete set null,
  expires_at timestamptz not null,
  used_at timestamptz,
  created_at timestamptz not null default now()
);
create index email_change_tokens_user on email_change_tokens (user_id);
