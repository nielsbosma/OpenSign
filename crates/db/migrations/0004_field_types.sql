-- Checks a signer's value must pass: pattern, message, minLength, maxLength, min, max.
alter table fields add column validation jsonb not null default '{}';
-- How a date or number is written on the page, e.g. "DD/MM/YYYY" or "1 234,00 kr".
alter table fields add column format text;
-- Radio boxes with the same key form one group; each box holds its option in `options`.
alter table fields add column group_key text;
-- The signer sees the sender's value but cannot change it.
alter table fields add column read_only boolean not null default false;
-- The image or file a signer put in an image or file field; `value` holds its name.
alter table fields add column file_id uuid references files (id) on delete set null;
create index fields_file_idx on fields (file_id) where file_id is not null;
