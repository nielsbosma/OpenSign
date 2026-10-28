# In-person signing

In-person signing lets a party sign on your own computer or tablet, for example at a counter, in a meeting, or at a customer's home. The party does not need an email address or a phone.

## Setting it up

In the party dialog, set **Send Link By** to **In Person (on your device)**. OpenSign then sends this party nothing. You can still ask them to identify with an SMS code or an eID; they do that on your device.

## Signing

1. Send the document as usual.
2. When it is the party's turn, open the document and choose **Sign in Person** on their party card.
3. OpenSign opens the signing page with a hand-over screen: "Hand the device to …". It explains that everything they do is recorded in the document's evidence.
4. The party confirms with **I Am …**, reads the document, identifies if asked, and signs.
5. When they are done, the page asks them to hand the device back to you.

## What is recorded

- Starting in-person signing issues a fresh link for the party and retires every earlier link, so a link you started before cannot be reused.
- The audit trail records who started the in-person session and for which party. The evidence pages show the party's identification, IP address and browser, as for any other signer.

## Notes

- **Sign in Person** is only offered for parties whose delivery is In Person, while the document is pending and it is that party's turn.
- Starting in-person signing needs a signed-in user. It is not available with an API key or through MCP.
- Scrive's `pad` delivery maps to in-person signing. See [Scrive compatibility](scrive.md).
