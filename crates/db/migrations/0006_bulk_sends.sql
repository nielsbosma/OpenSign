-- One template sent to many recipients from a spreadsheet. Rows are encrypted: they hold personal data.
create table bulk_sends (
  id uuid primary key,
  template_id uuid references documents (id) on delete set null,
  template_title text not null,
  owner_id uuid references users (id) on delete set null,
  file_name text not null,
  mapping jsonb not null,
  title_pattern text not null default '',
  status text not null default 'queued',
  total integer not null,
  sent integer not null default 0,
  failed integer not null default 0,
  created_at timestamptz not null default now(),
  finished_at timestamptz,
  constraint bulk_sends_status_check check (status in ('queued', 'running', 'done', 'canceled'))
);
create index bulk_sends_owner_idx on bulk_sends (owner_id, created_at desc);

create table bulk_rows (
  bulk_id uuid not null references bulk_sends (id) on delete cascade,
  row_number integer not null,
  label text not null default '',
  data text,
  status text not null default 'queued',
  document_id uuid references documents (id) on delete set null,
  error text,
  primary key (bulk_id, row_number),
  constraint bulk_rows_status_check check (status in ('queued', 'sent', 'failed', 'canceled'))
);
