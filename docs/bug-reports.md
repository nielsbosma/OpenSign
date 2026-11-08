# Reporting a bug

Found something that does not work? Report it from inside OpenSign. Your report becomes a public issue on GitHub, in your own name, so the people who build OpenSign can see what you saw.

## How to report

1. Click the bug icon, **Report a Bug**, in the top corner of the page, beside the assistant. It stays clickable while a dialog is open, so you can report a problem inside the dialog too.
2. A panel opens at the bottom of the page. Pick a tool:
   - **Comment**: click an element, say what is wrong with it, and press Enter. It gets a numbered box.
   - **Hide**: drag over anything private to black it out.
   - **Pause**: use the page as usual, for example to open a menu or dialog first. Esc pauses too.
3. Describe what went wrong and what you expected. A title is optional.
4. Press **Review the Report**. You see the final screenshot and every line that will be sent. Change or remove anything you do not want to share, or leave out the screenshot.
5. Tick the box that confirms the report may be public, then press **Continue on GitHub**.
6. A new window opens on the bug report service. It shows the report once more. Sign in with GitHub there and publish it. The window then links to your new issue.

You need a GitHub account that is at least a week old. If your browser blocks the new window, OpenSign shows a link to open it yourself.

## What is published

The issue shows:

- your title and description, and your comments with the element each one is about,
- the screenshot, if you kept it,
- the page as a pattern such as `/documents/:id`, the OpenSign version, your browser and system, the window size and the app language,
- open dialog titles, recent console errors and recent failed requests, if you kept those lines.

It never shows the address of your OpenSign, the real page address, your name or email address in OpenSign, or a document's contents. Technical lines lose email addresses, web addresses, ids, tokens, keys and long numbers before they leave OpenSign; each is replaced with `[removed]`.

The issue is created by your GitHub account, with the badge of the **OpenDeploy Bug Reports** app, which may only open issues. The service uses your sign-in for that one request and then revokes it.

## What is covered in the screenshot

Before the screenshot is taken, OpenSign covers in grey:

- everything typed into a field,
- PDF pages and thumbnails,
- names and email addresses of parties and users,
- document titles,
- audit trail entries,
- webhook addresses and allowed IP addresses of API keys.

The black boxes you draw with **Hide** are burned into the image as well. Nothing on the page itself changes. Check the screenshot in the review step: anything else private is yours to hide.

## Removing a screenshot

Screenshots are public and kept until someone removes them. Every issue ends with **Remove this screenshot**. Open it, sign in with GitHub, and the image is deleted and the issue says so. You can remove your own screenshots; the repository's maintainers can remove any.

GitHub keeps cached copies of images for a while, and anyone may have saved it, so removing a screenshot cannot take back every copy. That is why the review step asks you to check that nothing private is on screen.

## What leaves your OpenSign

Nothing is sent until you press **Continue on GitHub**. Then your OpenSign's server sends the report to the bug report service at `sliplane.opendeploy.cloud`, which keeps it for at most an hour until you publish it. OpenSign holds no GitHub credentials.

Each person can send 10 reports an hour, and an OpenSign 30 an hour.

## For admins

Bug reports are on by default. Under **Settings → Bug Reports** an admin can switch them off, which hides **Report a Bug** for everyone, or send reports to another repository, such as a fork. See [Admin settings](settings.md#bug-reports). Every report sent is recorded in the audit trail.

An install without outbound internet access cannot send reports.
