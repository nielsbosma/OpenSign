# Share links and QR codes

A share link lets anyone start a document from a template on their own: a membership form on your website, a waiver at the front desk, a consent form printed as a QR code on a poster. Whoever opens the link gives their name and email, confirms the email with a code, and then signs their own copy. Nothing is made for an address that was not confirmed.

## Turning on a share link

1. Open the template. You need to be able to change it and to send documents.
2. Under **Public Link**, turn on **Share With a Link or QR Code**.
3. In **Filled In by the Visitor**, choose the template party whoever opens the link becomes, such as "Customer". It must sign or approve. Every other party stays as it is in the template, so a counter-signer from your side still signs after them.
4. Copy the link, or choose **QR Code** and download it as PNG (for screens and documents) or SVG (for print).

The link looks like `https://sign.example.com/s/Xq3…`. It is off until you turn it on, and turning it off stops it at once. Documents made through the link are sent from the user who turned it on and appear in their documents list, titled after the template and the visitor's name.

Share links need email to be set up, since every visitor confirms their address. See [Settings](settings.md).

## What the visitor sees

1. A start page with your logo, the template's title and message, in the template's language. They can switch to any of the [11 signing languages](signing-page.md#languages).
2. They enter their name and email (and phone number, when the party gets SMS codes or SMS links). A name holds letters only, not numbers or addresses, since it goes into the document's title and the emails about it. Text messages for a link's documents count against the SMS limits in [Identification](identification.md).
3. OpenSign emails them a six-digit code that works for 15 minutes. They enter it.
4. OpenSign makes a document from the template with them as the party and sends it. The signing page opens straight away; the same link is also emailed to them, so they can come back to it later.

If other parties sign before them in a document that signs in order, they are told they will get an email when it is their turn.

The identification set on the template party still applies. If the party must identify with BankID or an SMS code, the visitor does that on the signing page as any signer would. See [Identification](identification.md).

## Limits

Each link has limits you set under **Public Link**:

| Limit | What it does |
| --- | --- |
| **Per Hour** | At most this many documents per hour (1 to 1000, default 10). |
| **In Total** | At most this many documents in all. Leave empty for no limit. |
| **Last Day** | The link stops working after this day. |

On top of these, OpenSign protects every link on its own:

- Each address gets at most 3 codes an hour, and each link at most three times its hourly limit (at least 20) in codes, so a link cannot be used to flood someone's inbox.
- A code allows 5 attempts and is used once.
- Each IP address is rate limited on the start page and when asking for or entering codes, as on the signing page.
- Unconfirmed requests are deleted after a day.

## Changing the address

**New Address** gives the link a new address. The old address and every QR code printed from it stop working at once. Use it when a link was shared further than you meant.

## With the API

- `GET /api/v1/templates/{templateId}/share-link`: the link, its limits and how many documents it made.
- `PUT /api/v1/templates/{templateId}/share-link` with `enabled`, `partyId`, `perHour`, `maxTotal` and `expiresAt`.
- `POST /api/v1/templates/{templateId}/share-link/rotate`: a new address.

These are also MCP tools (`share_links_get`, `share_links_set`, `share_links_rotate`). See [REST API](api.md) and [MCP](mcp.md).

Documents made through a share link fire `document.created` with `"source": "share_link"` and the link's `shareLinkId`. See [Webhooks](webhooks.md#events).

## What is recorded

The template's audit trail records every change to the link, each code sent (with the email address), and each document made. The new document's audit trail starts with its creation from the share link.
