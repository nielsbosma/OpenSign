-- Entries written from now on are chained with an HMAC keyed from APP_SECRET, so someone who can
-- write to the database cannot rebuild the chain. Earlier entries keep their plain SHA-256 links.
alter table audit_log add column hash_alg text not null default 'sha256' check (hash_alg in ('sha256', 'hmac-sha256'));
alter table audit_log alter column hash_alg set default 'hmac-sha256';
