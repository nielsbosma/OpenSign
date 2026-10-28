alter table parties drop constraint parties_delivery_check;
alter table parties add constraint parties_delivery_check check (delivery in ('email', 'sms', 'link', 'in_person', 'none'));
