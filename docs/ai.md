# AI assistant and review

OpenSign has two optional AI features: an assistant that works with your documents for you, and a review that reads a document before you send it. Both are off until an admin picks a provider, and both act with the user's own permissions.

## Setting up a provider

In **Settings → AI**:

| Field | |
| --- | --- |
| Provider | Anthropic, OpenAI, Berget AI (EU), Opper (EU), a LiteLLM proxy, or any other OpenAI-compatible server. |
| API Key | The provider's key. Stored encrypted. |
| Base URL | Only for LiteLLM and other compatible servers. May be on your private network, but not a link-local or cloud metadata address. |
| Allow document text to be sent for reviews | Off by default. Covers the assistant's reading of field values too. See [What leaves the server](#what-leaves-the-server). |
| Monthly Token Budget | Tokens per calendar month across everyone. 0 means no limit. |
| Model | Once the provider accepts your key, a searchable list of the chat models it offers, newest first. If the list cannot be fetched, type the model name, such as `claude-sonnet-5-5`, and the field shows the provider's error. |

The same page has a test button that sends a tiny request to check the provider, model and key, and shows how many tokens were used this month.

Using the AI features needs the **ai.use** permission (admins, managers and senders).

## The assistant

Open the assistant from the sparkle button in the app's header. Ask in plain language, for example:

- "Which documents are waiting for signatures?"
- "Remind everyone who has not signed this week."
- "Create a draft NDA for Bo Berg."

The assistant reads with the same tools as the [MCP server](mcp.md), limited to what your role allows. The app shows which tools it used.

Text inside documents could try to steer it, so it does not change anything on its own. When it wants to create, send, remind, cancel, delete or change something, the panel shows **Needs Your Confirmation** with what would happen and every value that would be used. Nothing happens until you press **Confirm**, which makes the same call as doing it yourself in the app, and it is in the audit trail under your name. The assistant cannot press it for you.

Some things it cannot even propose: it cannot invite or change users or their email addresses, see or change webhooks or automations, change or revoke API keys, hand out, see or share signing links, share documents, place a legal hold, send test emails, change settings, open or rotate share links, start an update, or handle whole files. Upload files and start bulk sends in the app.

## Document review

On a draft, the **Review** panel can ask the AI to read the document. Optionally say whose side you are on or what to focus on, such as "We are the supplier". The review lists:

- **Key Terms**: parties, amounts, dates, notice periods and similar,
- **Risks**, each with how serious it is, the page, and a suggestion,
- **Possibly Missing** clauses,
- **Questions to Ask** before signing,
- **Before Sending**: the same setup checks as the [document page](documents.md#before-you-send).

The latest review is kept with the document. Reviews need **Allow document text to be sent for reviews** to be on, since the model must read the text.

## What leaves the server

- With **Allow document text** off, the text of your documents and the values in their fields never leave the server. The assistant still works, but only with what its tools return, such as titles, statuses and party names.
- With it on, the assistant can also read field values, exports and previews, reviews send the document's text (up to about 180,000 characters), [Detect Fields with AI](text-tags.md#with-ai) sends the pages' text with positions (up to 30 pages and about 40,000 characters), and bulk send's AI column matching may include a few sample rows.
- Bulk send's AI matching always sends the column names.

Pick a provider and region that fit your data protection needs. Berget AI and Opper run in the EU, and a LiteLLM proxy lets you route to a model you host yourself. A LiteLLM or other OpenAI-compatible endpoint is the one outbound address an admin may point at a private network, though never at a link-local or cloud metadata address.

## Signers' own AI

Separately from these features, signers can give their own assistant a read-only copy of the document. That needs no provider and sends nothing from your install to an AI service. See [The signing page](signing-page.md#review-with-your-ai).

## Budget

When the monthly budget is used up, AI features answer "This month's AI budget is used up" until the next month or until an admin raises it. Usage is also available as the `ai_usage` MCP tool.

Whether or not a budget is set, one person can make 400 model calls an hour. After that their AI features pause for a while, so that one account cannot run up the bill alone.
