-- Unkeyed chain entries are no longer accepted. NOT VALID so that a database with old rows still
-- boots; audit.verify reports such rows as broken.
alter table audit_log add constraint audit_log_keyed_only check (hash_alg = 'hmac-sha256') not valid;
do $$
begin
  if not exists (select 1 from audit_log where hash_alg <> 'hmac-sha256') then
    alter table audit_log validate constraint audit_log_keyed_only;
  end if;
end;
$$;

create trigger audit_anchors_no_truncate before truncate on audit_anchors for each statement execute function refuse_change();
