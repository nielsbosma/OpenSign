create table folders (
  id uuid primary key,
  name text not null,
  parent_id uuid references folders (id) on delete cascade,
  scope text not null,
  created_by uuid references users (id) on delete set null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint folders_scope_check check (scope in ('documents', 'templates'))
);
create unique index folders_name_idx on folders (scope, coalesce(parent_id, '00000000-0000-0000-0000-000000000000'), lower(name));
create index folders_parent_idx on folders (parent_id);

alter table documents add column folder_id uuid references folders (id) on delete set null;
alter table documents add column archived_at timestamptz;
create index documents_folder_idx on documents (folder_id) where folder_id is not null;
