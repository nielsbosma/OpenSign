# AI assistant and review

OpenSign has two optional AI features: an assistant that works with your documents for you, and a review that reads a document before you send it. Both are off until an admin picks a provider, and both act with the user's own permissions.

## Setting up a provider

In **Settings → AI**:

| Field | |
| --- | --- |
| Provider | Anthropic, OpenAI, Berget AI (EU), Opper (EU), a LiteLLM proxy, or any other OpenAI-compatible server. |
| API Key | The provider's key. Stored encrypted. |
| Base URL | Only for LiteLLM and other compatible servers. |
| Allow document text to be sent for reviews | Off by default. See [What leaves the server](#what-leaves-the-server). |
| Monthly Token Budget | Tokens per calendar month across everyone. 0 means no limit. |
| Model | Once the provider accepts your key, a searchable list of the chat models it offers, newest first. If the list cannot be fetched, type the model name, such as `claude-sonnet-5-5`, and the field shows the provider's error. |

The same page has a test button that sends a tiny request to check the provider, model and key, and shows how many tokens were used this month.

Using the AI features needs the **ai.use** permission (admins, managers and senders).

## The assistant

Open the assistant from the sparkle button in the app's header. Ask in plain language, for example:

- "Which documents are waiting for signatures?"
- "Remind everyone who has not signed this week."
- "Create a draft NDA for Bo Berg."

The assistant uses the same tools as the [MCP server](mcp.md), limited to what your role allows, and every action it takes is in the audit trail under your name. The app shows which tools it used.

Because text inside documents could try to steer it, the assistant never gets tools that change who has access or that send things to people you did not pick: it cannot invite or change users, add, remove or test webhooks, change or revoke API keys, hand out signing links, share documents, or send test emails.

## Document review

On a draft, the **Review** panel can ask the AI to read the document. Optionally say whose side you are on or what to focus on, such as "We are the supplier". The review lists:

- **Key Terms**: parties, amounts, dates, notice periods and similar,
- **Risks**, each with how serious it is, the page, and a suggestion,
- **Possibly Missing** clauses,
- **Questions to Ask** before signing,
- **Before Sending**: the same setup checks as the [document page](documents.md#before-you-send).

The latest review is kept with the document. Reviews need **Allow document text to be sent for reviews** to be on, since the model must read the text.

## What leaves the server

- With **Allow document text** off, the text of your documents never leaves the server. The assistant still works, but only with what its tools return, such as titles, statuses and party names.
- With it on, reviews send the document's text (up to about 180,000 characters), and bulk send's AI column matching may include a few sample rows.
- Bulk send's AI matching always sends the column names.

Pick a provider and region that fit your data protection needs. Berget AI and Opper run in the EU, and a LiteLLM proxy lets you route to a model you host yourself. A LiteLLM or other OpenAI-compatible endpoint is the one outbound address an admin may point at a private network.

## Signers' own AI

Separately from these features, signers can give their own assistant a read-only copy of the document. That needs no provider and sends nothing from your install to an AI service. See [The signing page](signing-page.md#review-with-your-ai).

## Budget

When the monthly budget is used up, AI features answer "This month's AI budget is used up" until the next month or until an admin raises it. Usage is also available as the `ai_usage` MCP tool.
