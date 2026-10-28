import { ArrowLeft, ArrowRight, Hash, Menu, Moon, Search, Sun } from 'lucide-react';
import { type ComponentProps, type ReactNode, useEffect, useMemo, useState } from 'react';
import Markdown, { type Components } from 'react-markdown';
import { Link, NavLink, useLocation, useNavigate, useParams } from 'react-router';
import remarkGfm from 'remark-gfm';
import { Logo } from '@/components/common';
import { Button } from '@/components/ui/button';
import { CommandDialog, CommandEmpty, CommandInput, CommandItem, CommandList } from '@/components/ui/command';
import { Kbd } from '@/components/ui/kbd';
import { Sheet, SheetContent, SheetTitle } from '@/components/ui/sheet';
import { useTheme } from '@/lib/theme';
import { cn } from '@/lib/utils';
import { type DocPage, docHref, findPage, GROUPS, headingSlugger, headings, PAGES, pagePath, search } from './content';

const REPO = 'https://github.com/nielsbosma/OpenSign';

type HastNode = { type: string; tagName?: string; value?: string; properties?: Record<string, unknown>; children?: HastNode[] };

function textOf(node: HastNode): string {
  return node.type === 'text' ? (node.value ?? '') : (node.children ?? []).map(textOf).join('');
}

function rehypeHeadingIds() {
  return (tree: HastNode) => {
    const slug = headingSlugger();
    const walk = (node: HastNode) => {
      if (node.type === 'element' && /^h[1-6]$/.test(node.tagName ?? '')) {
        node.properties = { ...node.properties, id: slug(textOf(node).replace(/\s+/g, ' ').trim()) };
      }
      node.children?.forEach(walk);
    };
    walk(tree);
  };
}

function Heading({ level, id, children }: { level: 1 | 2 | 3 | 4; id?: string; children?: ReactNode }) {
  const Tag = `h${level}` as const;
  const size = {
    1: 'mb-4 text-3xl font-semibold tracking-tight',
    2: 'mt-12 mb-3 border-b pb-2 text-xl font-semibold tracking-tight',
    3: 'mt-8 mb-2 text-base font-semibold',
    4: 'mt-6 mb-2 text-sm font-semibold',
  }[level];
  return (
    <Tag id={id} className={cn('group scroll-mt-20', size)}>
      {children}
      {id && level > 1 && (
        <a
          href={`#${id}`}
          className="ml-1.5 inline-flex align-middle text-muted-foreground opacity-0 group-hover:opacity-100"
          aria-label="Link to this section"
        >
          <Hash className="size-3.5" />
        </a>
      )}
    </Tag>
  );
}

function DocLink({ href = '', children, ...props }: ComponentProps<'a'>) {
  const internal = docHref(href);
  if (internal)
    return (
      <Link to={internal} className={props.className}>
        {children}
      </Link>
    );
  if (href.startsWith('#') || href.startsWith('/'))
    return (
      <a href={href} className={props.className}>
        {children}
      </a>
    );
  return (
    <a href={href} target="_blank" rel="noreferrer" {...props}>
      {children}
    </a>
  );
}

const components: Components = {
  h1: ({ id, children }) => (
    <Heading level={1} id={id}>
      {children}
    </Heading>
  ),
  h2: ({ id, children }) => (
    <Heading level={2} id={id}>
      {children}
    </Heading>
  ),
  h3: ({ id, children }) => (
    <Heading level={3} id={id}>
      {children}
    </Heading>
  ),
  h4: ({ id, children }) => (
    <Heading level={4} id={id}>
      {children}
    </Heading>
  ),
  p: ({ children }) => <p className="my-4 leading-7">{children}</p>,
  a: ({ href, children }) => (
    <DocLink href={href} className="font-medium text-primary underline-offset-4 hover:underline">
      {children}
    </DocLink>
  ),
  ul: ({ children }) => <ul className="my-4 list-disc space-y-1.5 pl-6 leading-7">{children}</ul>,
  ol: ({ children }) => <ol className="my-4 list-decimal space-y-1.5 pl-6 leading-7">{children}</ol>,
  blockquote: ({ children }) => (
    <blockquote className="my-5 rounded-md border border-primary/25 bg-primary/5 px-4 py-0.5 [&>p]:my-3">{children}</blockquote>
  ),
  code: ({ children }) => <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.85em] break-words">{children}</code>,
  pre: ({ children }) => (
    <pre className="my-5 overflow-x-auto rounded-lg border bg-muted/50 p-4 font-mono text-[13px] leading-relaxed [&_code]:rounded-none [&_code]:bg-transparent [&_code]:p-0 [&_code]:text-[13px] [&_code]:break-normal">
      {children}
    </pre>
  ),
  table: ({ children }) => (
    <div className="my-5 overflow-x-auto rounded-lg border">
      <table className="w-full border-collapse text-left">{children}</table>
    </div>
  ),
  thead: ({ children }) => <thead className="bg-muted/50 [&:not(:has(th:not(:empty)))]:hidden">{children}</thead>,
  th: ({ children, style }) => (
    <th style={style} className="border-b px-3 py-2 font-medium whitespace-nowrap">
      {children}
    </th>
  ),
  td: ({ children, style }) => (
    <td style={style} className="border-b px-3 py-2 align-top [tr:last-child_&]:border-b-0">
      {children}
    </td>
  ),
  hr: () => <hr className="my-8" />,
};

function editLink(slug: string) {
  return `${REPO}/edit/main/docs/${slug}.md`;
}

function Nav({ current, onNavigate }: { current?: string; onNavigate?: () => void }) {
  return (
    <nav aria-label="Documentation" className="grid gap-6 text-sm">
      {GROUPS.map(({ group, pages }) => (
        <div key={group}>
          <div className="mb-1.5 px-2.5 text-xs font-medium text-muted-foreground">{group}</div>
          <div className="grid gap-0.5">
            {pages.map((p) => (
              <NavLink
                key={p.slug}
                to={pagePath(p.slug)}
                end
                onClick={onNavigate}
                className={cn(
                  'rounded-md px-2.5 py-1.5 text-foreground/75 transition-colors hover:bg-accent hover:text-foreground',
                  p.slug === current && 'bg-accent font-medium text-foreground',
                )}
              >
                {p.slug === 'index' ? 'Introduction' : p.title}
              </NavLink>
            ))}
          </div>
        </div>
      ))}
    </nav>
  );
}

function SearchDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [query, setQuery] = useState('');
  const navigate = useNavigate();
  const results = useMemo(() => search(query), [query]);
  return (
    <CommandDialog open={open} onOpenChange={onOpenChange} title="Search the Documentation" description="Find a page or a section">
      <CommandInput placeholder="Search the documentation" value={query} onValueChange={setQuery} />
      <CommandList className="max-h-[60vh]">
        {query.trim() && <CommandEmpty>Nothing matches.</CommandEmpty>}
        {!query.trim() &&
          PAGES.map((p) => (
            <CommandItem
              key={p.slug}
              value={p.slug}
              onSelect={() => {
                onOpenChange(false);
                navigate(pagePath(p.slug));
              }}
            >
              {p.slug === 'index' ? 'Introduction' : p.title}
            </CommandItem>
          ))}
        {results.map((r) => (
          <CommandItem
            key={`${r.slug}#${r.id ?? ''}`}
            value={`${r.slug}#${r.id ?? ''}`}
            onSelect={() => {
              onOpenChange(false);
              setQuery('');
              navigate(`${pagePath(r.slug)}${r.id ? `#${r.id}` : ''}`);
            }}
            className="flex-col items-start gap-0.5"
          >
            <span className="font-medium">
              {r.page}
              {r.heading && <span className="text-muted-foreground"> › {r.heading}</span>}
            </span>
            <span className="line-clamp-2 text-xs text-muted-foreground">{r.snippet}</span>
          </CommandItem>
        ))}
      </CommandList>
    </CommandDialog>
  );
}

function PrevNext({ page }: { page: DocPage }) {
  const i = PAGES.findIndex((p) => p.slug === page.slug);
  const prev = PAGES[i - 1];
  const next = PAGES[i + 1];
  return (
    <div className="mt-14 grid gap-3 border-t pt-6 sm:grid-cols-2">
      {prev ? (
        <Link to={pagePath(prev.slug)} className="rounded-lg border p-3 transition-colors hover:bg-accent">
          <div className="flex items-center gap-1 text-xs text-muted-foreground">
            <ArrowLeft className="size-3" /> Previous
          </div>
          <div className="font-medium">{prev.slug === 'index' ? 'Introduction' : prev.title}</div>
        </Link>
      ) : (
        <span />
      )}
      {next && (
        <Link to={pagePath(next.slug)} className="rounded-lg border p-3 text-right transition-colors hover:bg-accent">
          <div className="flex items-center justify-end gap-1 text-xs text-muted-foreground">
            Next <ArrowRight className="size-3" />
          </div>
          <div className="font-medium">{next.title}</div>
        </Link>
      )}
    </div>
  );
}

function useScrollToHash(key: string) {
  const { hash } = useLocation();
  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll again whenever the page or the hash changes
  useEffect(() => {
    const id = decodeURIComponent(hash.slice(1));
    const target = id ? document.getElementById(id) : null;
    if (target) target.scrollIntoView();
    else window.scrollTo(0, 0);
  }, [key, hash]);
}

/** Public documentation, rendered from the markdown files in `docs/`. No sign-in needed. */
export function DocsPage() {
  const { '*': rest } = useParams();
  const page = findPage(rest?.replace(/\/$/, ''));
  const { theme, toggle } = useTheme();
  const [menuOpen, setMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const toc = useMemo(() => (page ? headings(page.markdown) : []), [page]);
  useScrollToHash(page?.slug ?? '');

  useEffect(() => {
    document.title = page ? `${page.slug === 'index' ? 'Documentation' : page.title} · OpenSign` : 'Not Found · OpenSign';
  }, [page]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && e.target.closest('input, textarea, [contenteditable]');
      if ((e.key === 'k' && (e.metaKey || e.ctrlKey)) || (e.key === '/' && !typing)) {
        e.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <div className="min-h-svh bg-background">
      <header className="sticky top-0 z-20 border-b bg-background/90 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-7xl items-center gap-3 px-4">
          <Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setMenuOpen(true)} aria-label="Open Navigation">
            <Menu />
          </Button>
          <Link to="/docs" className="flex items-center gap-2.5">
            <Logo />
            <span className="text-sm font-semibold">
              OpenSign <span className="font-normal text-muted-foreground">Docs</span>
            </span>
          </Link>
          <div className="flex-1" />
          <Button variant="outline" className="h-9 w-9 justify-start text-muted-foreground sm:w-60" onClick={() => setSearchOpen(true)}>
            <Search />
            <span className="hidden sm:inline">Search</span>
            <Kbd className="ml-auto hidden sm:inline-flex">/</Kbd>
          </Button>
          <Button variant="ghost" size="icon" onClick={toggle} aria-label={theme === 'dark' ? 'Light Theme' : 'Dark Theme'}>
            {theme === 'dark' ? <Sun /> : <Moon />}
          </Button>
          <Button asChild size="sm" className="hidden sm:inline-flex">
            <a href="/">Open OpenSign</a>
          </Button>
        </div>
      </header>

      <Sheet open={menuOpen} onOpenChange={setMenuOpen}>
        <SheetContent side="left" className="w-72 overflow-y-auto p-4">
          <SheetTitle className="sr-only">Documentation</SheetTitle>
          <Nav current={page?.slug} onNavigate={() => setMenuOpen(false)} />
        </SheetContent>
      </Sheet>
      <SearchDialog open={searchOpen} onOpenChange={setSearchOpen} />

      <div className="mx-auto flex max-w-7xl gap-10 px-4">
        <aside className="sticky top-14 hidden h-[calc(100svh-3.5rem)] w-56 shrink-0 overflow-y-auto py-8 lg:block">
          <Nav current={page?.slug} />
        </aside>

        <main className="min-w-0 flex-1 py-8 lg:py-10">
          {page ? (
            <article className="mx-auto max-w-3xl text-[15px]">
              <Markdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHeadingIds]} components={components}>
                {page.markdown}
              </Markdown>
              <div className="mt-10 text-sm">
                <a href={editLink(page.slug)} target="_blank" rel="noreferrer" className="text-muted-foreground hover:text-foreground">
                  Edit this page on GitHub
                </a>
              </div>
              <PrevNext page={page} />
            </article>
          ) : (
            <div className="mx-auto max-w-3xl py-16 text-center">
              <h1 className="text-2xl font-semibold">Page not found</h1>
              <p className="mt-2 text-muted-foreground">This page does not exist, or it has moved.</p>
              <Button asChild className="mt-6">
                <Link to="/docs">Go to the Documentation</Link>
              </Button>
            </div>
          )}
        </main>

        {page && toc.length > 1 && (
          <aside className="sticky top-14 hidden h-[calc(100svh-3.5rem)] w-52 shrink-0 overflow-y-auto py-10 xl:block">
            <div className="mb-2 text-xs font-medium text-muted-foreground">On This Page</div>
            <nav aria-label="On this page" className="grid gap-1 text-[13px]">
              {toc.map((h) => (
                <a key={h.id} href={`#${h.id}`} className={cn('text-muted-foreground hover:text-foreground', h.depth === 3 && 'pl-3')}>
                  {h.text}
                </a>
              ))}
            </nav>
          </aside>
        )}
      </div>
    </div>
  );
}
