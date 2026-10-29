alter table webhook_deliveries
  add column resent_from uuid references webhook_deliveries (id) on delete set null,
  add column last_attempt_at timestamptz,
  -- Null once delivered or given up.
  add column next_attempt_at timestamptz;
create index webhook_deliveries_created_idx on webhook_deliveries (created_at);
create index webhook_deliveries_resent_from_idx on webhook_deliveries (resent_from);

-- One row per try, with the start of the answer so failures can be diagnosed.
create table webhook_attempts (
  id uuid primary key,
  delivery_id uuid not null references webhook_deliveries (id) on delete cascade,
  status_code integer,
  error text,
  response_excerpt text,
  duration_ms integer not null,
  created_at timestamptz not null default now()
);
create index webhook_attempts_delivery_idx on webhook_attempts (delivery_id, created_at);
