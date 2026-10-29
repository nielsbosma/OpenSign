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

## Signing a document yourself

For a document only you need to sign, such as a declaration, an order confirmation or a form you fill in for your own records, choose **Sign Myself** on the draft instead of **Send**.

1. Choose how you identify: **My OpenSign sign-in** (the default), or an eID such as BankID when an admin has set one up.
2. Choose **Sign Now**. You become the only signer: signers and approvers already on the draft are removed and their fields become yours. People who receive a copy keep getting one.
3. OpenSign sends the document and opens the signing page on your device, with no hand-over screen. Your [saved signature](account-security.md#saved-signature) is offered if you have one.
4. When you sign, the document is sealed and verifiable like any other.

With **My OpenSign sign-in**, the evidence pages say "Identified by: Signed-in OpenSign account" with your name and email, and the audit trail records the user who started it. If you leave before signing, **Sign in Person** on your party card opens the signing page again.

## Notes

- **Sign in Person** is only offered for parties whose delivery is In Person, while the document is pending and it is that party's turn.
- Starting in-person signing, or signing a document yourself, needs a signed-in user. Neither is available with an API key or through MCP.
- Scrive's `pad` delivery maps to in-person signing. See [Scrive compatibility](scrive.md).
