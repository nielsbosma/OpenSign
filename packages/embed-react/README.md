# @opensign/embed-react

Shows an OpenSign signing page inside a React app. Turn on embedded signing and list your site under
**Admin settings → Security** first. See [Embedded signing](../../docs/embedding.md).

```tsx
import { OpenSignSigning } from '@opensign/embed-react';

<OpenSignSigning
  url={signingUrl}
  style={{ height: 800 }}
  onSigned={({ documentId, completed }) => console.log('signed', documentId, completed)}
  onDeclined={() => console.log('declined')}
/>;
```

`signingUrl` comes from `POST /api/v1/documents/{documentId}/parties/{partyId}/signing-link` on your own server.
Never create it in the browser with an API key.
