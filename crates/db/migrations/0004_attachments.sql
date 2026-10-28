-- Files a party is asked to upload while signing, such as a copy of an ID or a certificate.
-- They are listed in the evidence and embedded in the sealed PDF.
create table attachment_requests (
  id uuid primary key,
  document_id uuid not null references documents (id) on delete cascade,
  party_id uuid not null references parties (id) on delete cascade,
  position integer not null default 0,
  name text not null,
  description text not null default '',
  required boolean not null default true,
  file_id uuid references files (id),
  uploaded_at timestamptz,
  created_at timestamptz not null default now(),
  constraint attachment_requests_upload_check check ((file_id is null) = (uploaded_at is null))
);
create index attachment_requests_document_idx on attachment_requests (document_id, party_id, position);
