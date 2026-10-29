import { UserPlus } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { toast } from 'sonner';
import { Field, Spinner } from '@/components/common';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { type Role, useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime } from '@/lib/format';
import { msg, t } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { SettingsCard } from './SettingsLayout';

const role = (label: string, description: string) => ({
  get label() {
    return t(label);
  },
  get description() {
    return t(description);
  },
});

export const ROLES: Record<Role, { label: string; description: string }> = {
  admin: role(msg('Admin'), msg('Everything, including settings and users')),
  manager: role(msg('Manager'), msg('All documents, templates and users')),
  sender: role(msg('Sender'), msg('Their own documents and templates')),
  auditor: role(msg('Auditor'), msg('Reads every document and the audit trail')),
  viewer: role(msg('Viewer'), msg('Documents shared with them')),
};

function InviteDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [email, setEmail] = useState('');
  const [name, setName] = useState('');
  const [role, setRole] = useState<Role>('sender');
  const invite = useApiMutation('users.invite', {
    invalidates: ['users.'],
    onSuccess: (out) => {
      toast.success(t('Invitation sent to {email}', { email: out.email }));
      onOpenChange(false);
      setEmail('');
      setName('');
    },
  });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    invite.mutate({ email: email.trim(), name: name.trim() || null, role });
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{t('Invite a User')}</DialogTitle>
            <DialogDescription>{t('They get an email with a link to choose a password. The link works for 7 days.')}</DialogDescription>
          </DialogHeader>
          <Field label={t('Email')} htmlFor="invite-email">
            <Input id="invite-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoFocus />
          </Field>
          <Field label={t('Name')} htmlFor="invite-name">
            <Input id="invite-name" value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <Field label={t('Role')}>
            <Select value={role} onValueChange={(v) => setRole(v as Role)}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(ROLES).map(([value, { label, description }]) => (
                  <SelectItem key={value} value={value} description={description}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t('Cancel')}
            </Button>
            <Button type="submit" disabled={invite.isPending}>
              {t('Send Invitation')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function UsersPage() {
  const { can, user: me } = useSession();
  const manage = can('users.manage');
  const users = useApi('users.list');
  const invites = useApi('users.list_invites', undefined, { enabled: manage });
  const update = useApiMutation('users.update', { invalidates: ['users.'] });
  const reset = useApiMutation('users.reset_two_factor', {
    invalidates: ['users.'],
    onSuccess: () => toast.success(t('Two-factor authentication reset')),
  });
  const revoke = useApiMutation('users.revoke_invite', { invalidates: ['users.'] });
  const [inviting, setInviting] = useState(false);

  return (
    <div className="grid gap-6">
      <SettingsCard
        help="users-and-roles"
        title={t('Users')}
        description={t('Everyone who can sign in. Roles decide what each person may see and do.')}
      >
        {manage && (
          <Button className="mb-4" onClick={() => setInviting(true)}>
            <UserPlus /> {t('Invite')}
          </Button>
        )}
        {users.isPending ? (
          <Spinner />
        ) : (
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('Name')}</TableHead>
                  <TableHead>{t('Role')}</TableHead>
                  <TableHead className="hidden md:table-cell">{t('Last Sign-In')}</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.data?.map((u) => (
                  <TableRow key={u.id} className={u.active ? undefined : 'opacity-60'}>
                    <TableCell>
                      <div className="font-medium">{u.name}</div>
                      <div className="text-xs text-muted-foreground">{u.email}</div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-1.5">
                        <span className="capitalize">{u.role}</span>
                        {!u.active && <Badge variant="outline">{t('Deactivated')}</Badge>}
                        {u.twoFactorEnabled && <Badge variant="secondary">{t('2FA')}</Badge>}
                      </div>
                    </TableCell>
                    <TableCell className="hidden text-muted-foreground md:table-cell">
                      {formatDateTime(u.lastLoginAt) || t('Never')}
                    </TableCell>
                    <TableCell className="text-end">
                      {manage && u.id !== me?.id && (
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm">
                              {t('Change')}
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuLabel>{t('Role')}</DropdownMenuLabel>
                            {(Object.keys(ROLES) as Role[]).map((role) => (
                              <DropdownMenuItem
                                key={role}
                                disabled={role === u.role}
                                onSelect={() => update.mutate({ id: u.id, role })}
                                className="grid gap-0.5"
                              >
                                <span className="font-medium">{ROLES[role].label}</span>
                                <span className="text-xs text-muted-foreground">{ROLES[role].description}</span>
                              </DropdownMenuItem>
                            ))}
                            <DropdownMenuSeparator />
                            {u.twoFactorEnabled && (
                              <DropdownMenuItem onSelect={() => reset.mutate({ id: u.id })}>{t('Reset Two-Factor')}</DropdownMenuItem>
                            )}
                            <DropdownMenuItem
                              onSelect={() => update.mutate({ id: u.id, active: !u.active })}
                              className={u.active ? 'text-destructive' : undefined}
                            >
                              {u.active ? t('Deactivate') : t('Reactivate')}
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </SettingsCard>
      {manage && !!invites.data?.length && (
        <SettingsCard title={t('Pending Invitations')}>
          <ul className="grid gap-2">
            {invites.data.map((i) => (
              <li key={i.id} className="flex items-center gap-3 rounded-md border px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate">{i.email}</div>
                  <div className="text-xs text-muted-foreground capitalize">
                    {i.role} · expires {formatDate(i.expiresAt)}
                  </div>
                </div>
                <Button variant="ghost" className="text-destructive" onClick={() => revoke.mutate({ id: i.id })}>
                  {t('Revoke')}
                </Button>
              </li>
            ))}
          </ul>
        </SettingsCard>
      )}
      <InviteDialog open={inviting} onOpenChange={setInviting} />
    </div>
  );
}
