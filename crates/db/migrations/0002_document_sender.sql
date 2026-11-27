-- The person a document is sent in the name of, when the API caller sends it for someone else.
-- Without it, invitations name the document's owner.
alter table documents
  add column sender_name text check (char_length(sender_name) <= 200),
  add column sender_title text check (char_length(sender_title) <= 200);
