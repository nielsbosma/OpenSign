-- The browser an eID broker flow started in; the callback must come back to it.
alter table eid_sessions add column browser_mac text;

-- What was spent on SMS, for the budgets in Admin settings.
create table sms_log (
  id uuid primary key,
  share_link_id uuid,
  sent_at timestamptz not null default now()
);
create index sms_log_sent_idx on sms_log (sent_at);
create index sms_log_share_link_idx on sms_log (share_link_id, sent_at) where share_link_id is not null;

alter table attachment_requests add column uploads integer not null default 0;
alter table fields add column uploads integer not null default 0;
