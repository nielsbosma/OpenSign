import { useApi } from './api';
import type { Permission } from './api-types.gen';

/** The signed-in user, or null when signed out. */
export function useSession() {
  const me = useApi('account.me', undefined, { retry: false, staleTime: 60_000 });
  const user = me.error?.status === 401 ? null : (me.data ?? null);
  const can = (permission: Permission) => !!user?.permissions.includes(permission);
  return { user, loading: me.isPending, can };
}
