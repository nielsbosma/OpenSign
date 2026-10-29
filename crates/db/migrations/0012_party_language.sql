alter table parties add column language text check (language in ('en', 'zh', 'hi', 'es', 'fr', 'ar', 'bn', 'pt', 'ru', 'ur', 'sv'));
