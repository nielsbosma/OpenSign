-- Saved states of a template: its settings, files, parties, attachment requests and fields as JSON.
create table template_versions (
  id uuid primary key,
  template_id uuid not null references documents (id) on delete cascade,
  number integer not null,
  snapshot jsonb not null,
  note text,
  -- Set when the version was made by restoring an earlier one.
  restored_from integer,
  created_by uuid references users (id) on delete set null,
  created_at timestamptz not null default now(),
  constraint template_versions_number_key unique (template_id, number)
);

-- Keeps the files a version names from being purged while the version exists.
create table template_version_files (
  version_id uuid not null references template_versions (id) on delete cascade,
  file_id uuid not null references files (id),
  primary key (version_id, file_id)
);
create index template_version_files_file_idx on template_version_files (file_id);

-- The template version a document was made from.
alter table documents add column template_version integer;
create index documents_template_idx on documents (template_id, template_version) where template_id is not null;
