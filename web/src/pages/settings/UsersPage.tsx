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
import { useSession } from '@/lib/session';
import { SettingsCard } from './SettingsLayout';

export const ROLES: Record<Role, string> = {
  admin: 'Admin: everything, including settings and users',
  manager: 'Manager: all documents, templates and users',
  sender: 'Sender: their own documents and templates',
  auditor: 'Auditor: reads every document and the audit trail',
  viewer: 'Viewer: documents shared with them',
};

function InviteDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [email, setEmail] = useState('');
  const [name, setName] = useState('');
  const [role, setRole] = useState<Role>('sender');
  const invite = useApiMutation('users.invite', {
    invalidates: ['users.'],
    onSuccess: (out) => {
      toast.success(`Invitation sent to ${out.email}`);
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
            <DialogTitle>Invite a User</DialogTitle>
            <DialogDescription>They get an email with a link to choose a password. The link works for 7 days.</DialogDescription>
          </DialogHeader>
          <Field label="Email" htmlFor="invite-email">
            <Input id="invite-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoFocus />
          </Field>
          <Field label="Name" htmlFor="invite-name">
            <Input id="invite-name" value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <Field label="Role">
            <Select value={role} onValueChange={(v) => setRole(v as Role)}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(ROLES).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={invite.isPending}>
              Send Invitation
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
    onSuccess: () => toast.success('Two-factor authentication reset'),
  });
  const revoke = useApiMutation('users.revoke_invite', { invalidates: ['users.'] });
  const [inviting, setInviting] = useState(false);

  return (
    <div className="grid gap-6">
      <SettingsCard title="Users" description="Everyone who can sign in. Roles decide what each person may see and do.">
        {manage && (
          <Button className="mb-4" onClick={() => setInviting(true)}>
            <UserPlus /> Invite
          </Button>
        )}
        {users.isPending ? (
          <Spinner />
        ) : (
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead className="hidden md:table-cell">Last Sign-In</TableHead>
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
                        {!u.active && <Badge variant="outline">Deactivated</Badge>}
                        {u.twoFactorEnabled && <Badge variant="secondary">2FA</Badge>}
                      </div>
                    </TableCell>
                    <TableCell className="hidden text-muted-foreground md:table-cell">{formatDateTime(u.lastLoginAt) || 'Never'}</TableCell>
                    <TableCell className="text-right">
                      {manage && u.id !== me?.id && (
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm">
                              Change
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuLabel>Role</DropdownMenuLabel>
                            {(Object.keys(ROLES) as Role[]).map((role) => (
                              <DropdownMenuItem
                                key={role}
                                disabled={role === u.role}
                                onSelect={() => update.mutate({ id: u.id, role })}
                                className="capitalize"
                              >
                                {role}
                              </DropdownMenuItem>
                            ))}
                            <DropdownMenuSeparator />
                            {u.twoFactorEnabled && (
                              <DropdownMenuItem onSelect={() => reset.mutate({ id: u.id })}>Reset Two-Factor</DropdownMenuItem>
                            )}
                            <DropdownMenuItem
                              onSelect={() => update.mutate({ id: u.id, active: !u.active })}
                              className={u.active ? 'text-destructive' : undefined}
                            >
                              {u.active ? 'Deactivate' : 'Reactivate'}
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
        <SettingsCard title="Pending Invitations">
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
                  Revoke
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
