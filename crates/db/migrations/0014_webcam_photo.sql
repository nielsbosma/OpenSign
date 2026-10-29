-- A party can be asked for a live photo taken with their camera while signing. It is stored as an
-- attachment request of kind 'photo', so it is encrypted, sealed into the evidence and purged like any upload.
alter table attachment_requests add column kind text not null default 'file';
alter table attachment_requests add constraint attachment_requests_kind_check check (kind in ('file', 'photo'));
create unique index attachment_requests_one_photo_idx on attachment_requests (party_id) where kind = 'photo';
