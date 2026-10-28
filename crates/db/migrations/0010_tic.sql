alter table eid_sessions drop constraint eid_sessions_provider_check;
alter table eid_sessions add constraint eid_sessions_provider_check check (provider in ('se_bankid', 'freja', 'broker', 'tic'));
