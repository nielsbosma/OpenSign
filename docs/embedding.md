# Embedded signing

Show the OpenSign signing page inside your own web app, so customers sign without leaving it. It is off until an admin turns it on, and then only the sites you list can show the page.

## Turning it on

1. Go to **Settings → Security** (Admin settings).
2. Turn on **Allow Embedded Signing**.
3. Under **Sites Allowed to Embed**, enter each site that will show the signing page, one per line, such as `https://app.example.com`. Use the exact origin: `https`, the host and a port if it is not 443, with no path and no wildcards. `http://localhost` is accepted for development.

Both can be set with environment variables instead, which locks them in the app:

```sh
OPENSIGN_SECURITY_EMBED_SIGNING=true
OPENSIGN_SECURITY_EMBED_ORIGINS="https://app.example.com https://crm.example.com"
```

Turning the setting off stops every frame at once: the next page load refuses to be framed.

## Getting a signing link

Embedding uses the links you hand over yourself, so OpenSign sends nothing to the party.

1. Add the party with **Send Link By: Link** (`"delivery": "link"` in the API). See [Parties](parties.md#delivery).
2. Send the document. The response to `POST /documents/{id}/send` holds each link party's `url`.
3. Later, or for a fresh link, call `POST /documents/{documentId}/parties/{partyId}/signing-link`. Issuing a link is written to the audit trail.

Create links on your server with your API key, and pass only the link to the browser. The link is the party's key to the document, so give it only to the person who signs.

Links of parties who get their link by email or SMS are refused inside a frame, even on a listed site. The page asks them to open the link in their browser instead.

## Showing the page

Load `embed.js` from your OpenSign server and place the element with the link:

```html
<script src="https://sign.example.com/embed.js"></script>

<opensign-signing id="signing" url="https://sign.example.com/sign#…" style="height: 800px"></opensign-signing>

<script>
  const signing = document.getElementById('signing');
  signing.addEventListener('opensign:signed', (e) => console.log('Signed', e.detail.documentId, e.detail.completed));
  signing.addEventListener('opensign:declined', (e) => console.log('Declined', e.detail.documentId));
</script>
```

The element creates an iframe with the camera allowed, for [webcam photos](identification.md#webcam-photo). You can also write the iframe yourself and listen for the messages below.

React and Vue wrappers live in `packages/embed-react` and `packages/embed-vue` in the repository. They load `embed.js` from the server that issued the link:

```tsx
import { OpenSignSigning } from '@opensign/embed-react';

<OpenSignSigning url={signingUrl} style={{ height: 800 }} onSigned={(d) => refresh(d.documentId)} />;
```

## Events

The signing page tells your page what happened with `postMessage`, sent only to your listed origins, so the browser delivers each message only to the site that is framing it. A message never holds the signing link or document content.

| Event | When | Detail |
| --- | --- | --- |
| `opensign:loaded` | The page has opened the link | `documentId`, `partyId` |
| `opensign:signed` | The party signed or approved | `documentId`, `partyId`, `completed` (everyone has signed) |
| `opensign:declined` | The party declined | `documentId`, `partyId` |
| `opensign:error` | Signing or declining failed | `documentId`, `partyId`, `message` |

Raw messages have `source: "opensign"` and `type` set to the event name without the prefix. Check `event.origin` against your OpenSign address before trusting one; `embed.js` does this for you.

Use the events to update your page, and [webhooks](webhooks.md) or the API to update your records: a webhook is signed and arrives even if the person closes the tab.

## Redirect after signing

Add `redirectUrl` when you issue the link, and the page goes there after the party signs or declines, with `event=signing_complete` or `event=decline` added, as DocuSign does:

```sh
curl -X POST https://sign.example.com/api/v1/documents/$DOC/parties/$PARTY/signing-link \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"redirectUrl": "https://app.example.com/contracts/42/done"}'
```

The redirect must be on a listed site, or the call is refused. The returned link carries it as `?redirect=`, and the page checks it again with the server before following it. Inside a frame, the frame itself goes to the address.

## Identification in a frame

- **SMS codes** work as usual.
- **BankID** shows its QR code in the frame. **Open BankID on This Device** starts the app from the frame.
- **Freja eID** shows its QR code in the frame.
- **e-ID through a broker** and **BankID through TIC Identity** open in a new window, because identity providers refuse to run in frames. The window first says what the identification is for. When the person finishes, it shows a six-digit code to enter in the frame, so a forwarded window link cannot identify someone on behalf of whoever holds the frame. If the browser blocks the window, the frame shows a button to open it.

## What stays the same

- Every other page of OpenSign (sign-in, the app, the verify page, review links) can never be framed, by any site.
- The page shows your organisation's name and "Secured by OpenSign" in the frame, so signers can see who they are signing with.
- Signing still needs a real click inside the frame, and everything is recorded in the audit trail and sealed into the evidence as for any other signer.
- The signing page keeps its session in the frame's own storage, not in cookies, so it works where browsers block third-party cookies.

## DocuSign clients

`POST /envelopes/{id}/views/recipient` returns the link for a recipient added with `clientUserId`. Its `returnUrl` is followed when its site is listed under **Sites Allowed to Embed** and embedding is on; otherwise the page shows its own confirmation. See [DocuSign API](docusign.md).
