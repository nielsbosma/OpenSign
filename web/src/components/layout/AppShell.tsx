import { useQueryClient } from '@tanstack/react-query';
import {
  ArrowUpCircle,
  Braces,
  FileStack,
  FileText,
  LogOut,
  Menu,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  ShieldCheck,
  Sparkles,
  Sun,
} from 'lucide-react';
import { useState } from 'react';
import { NavLink, Outlet, useNavigate } from 'react-router';
import { AssistantPanel } from '@/components/AssistantPanel';
import { Logo } from '@/components/common';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Sheet, SheetContent, SheetTitle } from '@/components/ui/sheet';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { call, useApi } from '@/lib/api';
import { useSession } from '@/lib/session';
import { useTheme } from '@/lib/theme';
import { useMediaQuery } from '@/lib/use-media-query';
import { cn } from '@/lib/utils';

const linkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    'flex h-8 items-center gap-2.5 rounded-md px-2.5 text-[13px] font-medium text-sidebar-foreground/75 transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground',
    isActive && 'bg-sidebar-accent text-sidebar-foreground',
  );

function UserMenu() {
  const { user } = useSession();
  const { theme, toggle } = useTheme();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const signOut = async () => {
    await call('auth.logout').catch(() => {});
    queryClient.clear();
    navigate('/login', { replace: true });
  };
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button type="button" className="flex h-10 w-full items-center gap-2.5 rounded-md px-2 text-left hover:bg-sidebar-accent">
          <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-primary/15 text-[11px] font-semibold text-primary">
            {user?.name.slice(0, 1).toUpperCase()}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px] font-medium">{user?.name}</span>
            <span className="block truncate text-[11px] text-muted-foreground">{user?.email}</span>
          </span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-56">
        <DropdownMenuLabel className="font-normal text-muted-foreground capitalize">{user?.role}</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={toggle}>
          {theme === 'dark' ? <Sun /> : <Moon />} {theme === 'dark' ? 'Light Theme' : 'Dark Theme'}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => navigate('/settings')}>
          <Settings /> Settings
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={signOut}>
          <LogOut /> Sign Out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const SIDEBAR_KEY = 'opensign-sidebar';

function storedCollapsed() {
  try {
    return localStorage.getItem(SIDEBAR_KEY) === 'collapsed';
  } catch {
    return false;
  }
}

function Sidebar({ onNavigate, onClose }: { onNavigate?: () => void; onClose: () => void }) {
  const { user, can } = useSession();
  const version = useApi('system.version', { refresh: false }, { enabled: can('settings.manage'), staleTime: 60 * 60_000 });
  return (
    <div className="flex h-full flex-col bg-sidebar">
      <div className="flex h-14 shrink-0 items-center gap-2.5 px-3">
        <Logo />
        <span className="text-sm font-semibold">OpenSign</span>
        {user?.version && <span className="text-[11px] text-muted-foreground tabular-nums">{user.version}</span>}
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close Sidebar" className="ml-auto size-8 text-muted-foreground">
          <PanelLeftClose />
        </Button>
      </div>
      <nav className="flex flex-1 flex-col gap-0.5 px-2 py-1" aria-label="Main">
        <NavLink to="/documents" className={linkClass} onClick={onNavigate}>
          <FileText className="size-4 opacity-80" /> Documents
        </NavLink>
        {can('documents.create') && (
          <NavLink to="/templates" className={linkClass} onClick={onNavigate}>
            <FileStack className="size-4 opacity-80" /> Templates
          </NavLink>
        )}
        <NavLink to="/api-explorer" className={linkClass} onClick={onNavigate}>
          <Braces className="size-4 opacity-80" /> API Explorer
        </NavLink>
        <a href="/verify" target="_blank" rel="noreferrer" className={linkClass({ isActive: false })}>
          <ShieldCheck className="size-4 opacity-80" /> Verify a Document
        </a>
      </nav>
      <div className="grid gap-0.5 border-t border-sidebar-border px-2 py-2">
        {version.data?.updateAvailable && (
          <NavLink to="/settings/system" className={linkClass} onClick={onNavigate}>
            <ArrowUpCircle className="size-4 text-primary" /> Update to {version.data.latest?.version}
          </NavLink>
        )}
        <UserMenu />
      </div>
    </div>
  );
}

/** Navigation on the left, content in the middle, the optional assistant on the right. */
export function AppShell() {
  const docked = useMediaQuery('(min-width: 1024px)');
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [collapsed, setCollapsed] = useState(storedCollapsed);
  const setSidebar = (value: boolean) => {
    setCollapsed(value);
    try {
      localStorage.setItem(SIDEBAR_KEY, value ? 'collapsed' : 'open');
    } catch {}
  };
  const showSidebar = docked && !collapsed;
  const [chatOpen, setChatOpen] = useState(false);
  const ai = useApi('ai.status', undefined, { staleTime: 5 * 60_000 });
  const assistant = !!ai.data?.configured && ai.data.canUse;

  return (
    <div className="flex h-svh overflow-hidden">
      {docked ? (
        showSidebar && (
          <aside className="w-60 shrink-0 border-r border-sidebar-border">
            <Sidebar onClose={() => setSidebar(true)} />
          </aside>
        )
      ) : (
        <Sheet open={drawerOpen} onOpenChange={setDrawerOpen}>
          <SheetContent side="left" className="w-72 p-0 [&>button]:hidden">
            <SheetTitle className="sr-only">Navigation</SheetTitle>
            <Sidebar onNavigate={() => setDrawerOpen(false)} onClose={() => setDrawerOpen(false)} />
          </SheetContent>
        </Sheet>
      )}

      <main className="relative min-w-0 flex-1 overflow-y-auto">
        {!showSidebar && (
          <div className="absolute top-2.5 left-3 z-10">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => (docked ? setSidebar(false) : setDrawerOpen(true))}
              aria-label="Open Navigation"
              className="text-muted-foreground"
            >
              {docked ? <PanelLeftOpen /> : <Menu />}
            </Button>
          </div>
        )}
        {assistant && !chatOpen && (
          <div className="absolute top-2.5 right-3 z-10">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setChatOpen(true)}
                  aria-label="Assistant"
                  className="text-muted-foreground"
                >
                  <Sparkles />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Assistant</TooltipContent>
            </Tooltip>
          </div>
        )}
        <div
          className={cn(
            'mx-auto w-full max-w-6xl pt-6 pb-10 lg:pt-10 pr-14 sm:pr-16',
            showSidebar ? 'pl-4 sm:pl-6 lg:pl-8' : 'pl-14 sm:pl-16',
          )}
        >
          <Outlet />
        </div>
      </main>

      {assistant && chatOpen && docked && (
        <aside className="flex w-[380px] shrink-0 border-l xl:w-[420px]" aria-label="Assistant">
          <AssistantPanel onClose={() => setChatOpen(false)} />
        </aside>
      )}
      {assistant && !docked && (
        <Sheet open={chatOpen} onOpenChange={setChatOpen}>
          <SheetContent side="right" className="w-full gap-0 p-0 sm:max-w-md [&>button]:hidden">
            <SheetTitle className="sr-only">Assistant</SheetTitle>
            <AssistantPanel onClose={() => setChatOpen(false)} />
          </SheetContent>
        </Sheet>
      )}
    </div>
  );
}
