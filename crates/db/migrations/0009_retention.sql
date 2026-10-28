-- Retention rules delete documents after a set time; a legal hold keeps one until it is released.

alter table documents
  add column legal_hold boolean not null default false,
  add column legal_hold_reason text,
  add column legal_hold_at timestamptz,
  add column legal_hold_by uuid references users (id) on delete set null;

create index documents_retention_idx on documents (status, updated_at) where not legal_hold and not is_template;
