# Option B — Option A + per-user email opt-out in this slice

## Shape

Everything in Option A, plus:

- `users.notification_email` (or `user_settings.email_notifications`)
  column — DB migration + dialect accessors.
- `notification.updateEmailPrefs` / `notification.getEmailPrefs` RPC
  (bikeshed: `user.updateNotificationPrefs`) → `make rpc-gen` → api-client.
- Checkbox on the existing `/settings/notifications` page
  (`apps/web/src/routes/settings/notifications.tsrx`) — TanStack Query
  mutation, follows existing settings patterns.
- Dispatch filters recipients by the flag before addressing.

## Tradeoffs

+ Users can silence mail without unwatching — closes the loudest expected
  complaint for a self-host forge.
+ The settings page already exists and already edits watch prefs; the
  toggle rides an established surface.
− Adds a migration, RPC pair, codegen, and web UI to a slice whose roadmap
  text explicitly assigns "email prefs" to the NOT-04 tail — scope creep
  against the tranche plan.
− Once a `notification_email` column exists, NOT-04's per-type matrix will
  migrate/rename it anyway — paying migration cost twice.
− Web changes pull in `web-lint`/`web-format-check`/browser-coverage
  obligations for a single checkbox.

## When this wins over A

If the spec stage judges "email on by default with no off switch" as
unacceptable for cloud users, B is the cheapest compliant slice. If an env
master switch + watch prefs is deemed enough (matching GitHub defaults), A
ships less code now and leaves the pref model whole for NOT-04.
