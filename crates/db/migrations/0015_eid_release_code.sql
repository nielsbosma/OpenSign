-- An identification started from a framed signing page: the session goes to the frame only with the
-- code shown in the window where the person identified.
alter table eid_sessions add column framed boolean not null default false;
alter table eid_sessions add column release_mac text;
alter table eid_sessions add column release_attempts int not null default 0;
