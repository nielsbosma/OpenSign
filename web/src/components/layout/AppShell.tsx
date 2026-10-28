import { useQueryClient } from '@tanstack/react-query';
import { ArrowUpCircle, Braces, FileStack, FileText, LogOut, Menu, Moon, Settings, ShieldCheck, Sparkles, Sun } from 'lucide-react';
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
import { ROLE_LABELS } from '@/lib/format';
import { isRtl, t, useLanguage } from '@/lib/i18n';
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
        <button type="button" className="flex h-10 w-full items-center gap-2.5 rounded-md px-2 text-start hover:bg-sidebar-accent">
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
        <DropdownMenuLabel className="font-normal text-muted-foreground">
          {user && t(ROLE_LABELS[user.role] ?? user.role)}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={toggle}>
          {theme === 'dark' ? <Sun /> : <Moon />} {theme === 'dark' ? t('Light Theme') : t('Dark Theme')}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => navigate('/settings/account')}>
          <Settings /> {t('Your Account')}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={signOut}>
          <LogOut /> {t('Sign Out')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function Sidebar({ onNavigate }: { onNavigate?: () => void }) {
  const { can } = useSession();
  const version = useApi('system.version', { refresh: false }, { enabled: can('settings.manage'), staleTime: 60 * 60_000 });
  return (
    <div className="flex h-full flex-col bg-sidebar">
      <div className="flex h-14 shrink-0 items-center gap-2.5 px-3">
        <Logo />
        <span className="text-sm font-semibold">OpenSign</span>
      </div>
      <nav className="flex flex-1 flex-col gap-0.5 px-2 py-1" aria-label={t('Main')}>
        <NavLink to="/documents" className={linkClass} onClick={onNavigate}>
          <FileText className="size-4 opacity-80" /> {t('Documents')}
        </NavLink>
        {can('documents.create') && (
          <NavLink to="/templates" className={linkClass} onClick={onNavigate}>
            <FileStack className="size-4 opacity-80" /> {t('Templates')}
          </NavLink>
        )}
        <NavLink to="/api-explorer" className={linkClass} onClick={onNavigate}>
          <Braces className="size-4 opacity-80" /> {t('API Explorer')}
        </NavLink>
        <a href="/verify" target="_blank" rel="noreferrer" className={linkClass({ isActive: false })}>
          <ShieldCheck className="size-4 opacity-80" /> {t('Verify a Document')}
        </a>
      </nav>
      <div className="grid gap-0.5 border-t border-sidebar-border px-2 py-2">
        <NavLink to="/settings" className={linkClass} onClick={onNavigate}>
          <Settings className="size-4 opacity-80" /> {t('Settings')}
        </NavLink>
        {version.data?.updateAvailable && (
          <NavLink to="/settings/system" className={linkClass} onClick={onNavigate}>
            <ArrowUpCircle className="size-4 text-primary" /> {t('Update to {version}', { version: version.data.latest?.version ?? '' })}
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
  const [chatOpen, setChatOpen] = useState(false);
  const ai = useApi('ai.status', undefined, { staleTime: 5 * 60_000 });
  const assistant = !!ai.data?.configured && ai.data.canUse;
  const rtl = isRtl(useLanguage().language);

  return (
    <div className="flex h-svh overflow-hidden">
      {docked ? (
        <aside className="w-60 shrink-0 border-e border-sidebar-border">
          <Sidebar />
        </aside>
      ) : (
        <Sheet open={drawerOpen} onOpenChange={setDrawerOpen}>
          <SheetContent side={rtl ? 'right' : 'left'} className="w-72 p-0 [&>button]:hidden">
            <SheetTitle className="sr-only">{t('Navigation')}</SheetTitle>
            <Sidebar onNavigate={() => setDrawerOpen(false)} />
          </SheetContent>
        </Sheet>
      )}

      <main className="relative min-w-0 flex-1 overflow-y-auto">
        {!docked && (
          <div className="absolute start-3 top-2.5 z-10">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setDrawerOpen(true)}
              aria-label={t('Open Navigation')}
              className="text-muted-foreground"
            >
              <Menu />
            </Button>
          </div>
        )}
        {assistant && !chatOpen && (
          <div className="absolute end-3 top-2.5 z-10">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setChatOpen(true)}
                  aria-label={t('Assistant')}
                  className="text-muted-foreground"
                >
                  <Sparkles />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t('Assistant')}</TooltipContent>
            </Tooltip>
          </div>
        )}
        <div className={cn('mx-auto w-full max-w-6xl pt-3.5 pe-14 pb-10 sm:pe-16', docked ? 'ps-4 sm:ps-6 lg:ps-8' : 'ps-14 sm:ps-16')}>
          <Outlet />
        </div>
      </main>

      {assistant && chatOpen && docked && (
        <aside className="flex w-[380px] shrink-0 border-s xl:w-[420px]" aria-label={t('Assistant')}>
          <AssistantPanel onClose={() => setChatOpen(false)} />
        </aside>
      )}
      {assistant && !docked && (
        <Sheet open={chatOpen} onOpenChange={setChatOpen}>
          <SheetContent side={rtl ? 'left' : 'right'} className="w-full gap-0 p-0 sm:max-w-md [&>button]:hidden">
            <SheetTitle className="sr-only">{t('Assistant')}</SheetTitle>
            <AssistantPanel onClose={() => setChatOpen(false)} />
          </SheetContent>
        </Sheet>
      )}
    </div>
  );
}
