-- Failed sign-ins per email and address. The row with address '*' is the total across addresses.
-- users.failed_login_count and users.locked_until are no longer read and can go in a later release.
create table login_failures (
  subject text not null,
  address text not null,
  failures integer not null,
  last_failed_at timestamptz not null,
  locked_until timestamptz,
  primary key (subject, address)
);

create index login_failures_last_failed_at on login_failures (last_failed_at);

alter table webauthn_challenges drop constraint webauthn_challenges_purpose_check;
alter table webauthn_challenges add constraint webauthn_challenges_purpose_check
  check (purpose in ('register', 'sign_in', 'second_factor', 'step_up'));

create index invites_invited_by on invites (invited_by) where accepted_at is null and revoked_at is null;
