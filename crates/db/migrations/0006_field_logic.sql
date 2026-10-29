-- When a field is shown: a list of { field, op, value } rules on other fields' labels, all of which must hold.
alter table fields add column conditions jsonb not null default '[]';
-- The expression of a `formula` field, such as "{Hours} * {Rate}".
alter table fields add column formula text;
