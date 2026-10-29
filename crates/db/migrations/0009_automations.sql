-- Rules that run a Composio tool when a document event happens.
create table automations (
  id uuid primary key,
  name text not null,
  event text not null,
  -- Empty: every document.
  template_ids uuid[] not null default '{}',
  folder_ids uuid[] not null default '{}',
  toolkit text not null,
  tool text not null,
  -- The tool version when the rule was saved, so a change at Composio cannot break it silently.
  tool_version text,
  connected_account_id text not null,
  -- The tool's arguments; strings may hold {{placeholders}} filled from the document.
  arguments jsonb not null default '{}',
  active boolean not null default true,
  created_by uuid references users (id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

-- Runs share the webhook delivery log: same retries, attempts and 30-day sweep.
alter table webhook_deliveries
  alter column webhook_id drop not null,
  add column automation_id uuid references automations (id) on delete cascade,
  add constraint webhook_deliveries_target_check check ((webhook_id is null) <> (automation_id is null));
create index webhook_deliveries_automation_idx on webhook_deliveries (automation_id, created_at desc) where automation_id is not null;
