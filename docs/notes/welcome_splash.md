# Notes: Welcome splash for the GitHub Pages build

Status: **notes only, not implemented.** Captured while building the theme
system (2026-07-02). This is a sketch of the approach, not a committed design.

## Goal

Show a first-run welcome/overview to users hitting the public GitHub Pages
deployment. Dismissible, and it must **stay dismissed** across reloads — but we
want the ability for a future release to force it to reappear once (e.g. after
a milestone worth re-announcing) without nagging users who already dismissed
the *current* message.

## The mechanism: a monotonic "seen version"

The clean pattern is a single integer constant in the source plus one
localStorage key.

```js
// somewhere like lib/ui/welcome.js
export const WELCOME_VERSION = 1;         // bump to force the splash to reappear
const KEY = 'waffle:welcome-seen';

export function shouldShowWelcome() {
  try {
    const seen = parseInt(localStorage.getItem(KEY) || '0', 10);
    return seen < WELCOME_VERSION;
  } catch { return true; }   // storage blocked → show it (harmless)
}

export function dismissWelcome() {
  try { localStorage.setItem(KEY, String(WELCOME_VERSION)); } catch {}
}
```

- **Dismiss** writes the *current* `WELCOME_VERSION` into the key. As long as
  `WELCOME_VERSION` is unchanged, `shouldShowWelcome()` stays false → the user
  never sees it again.
- **Force a reset for a release**: bump `WELCOME_VERSION` to `2` in source. Every
  user's stored `1 < 2` → splash shows once more; dismissing writes `2`.

This is deliberately a version *number*, not a boolean, so "reset on this
release" is a one-line source change (bump the constant) with no migration and
no per-user server state. It never resets on its own.

## Where it lives

- Best home is the landing page (`src/routes/home/+page.svelte`) — that's the
  GitHub Pages first impression. Render a modal overlay when
  `shouldShowWelcome()` is true, gated on `onMount` (never during SSR/prerender).
- Component `WelcomeSplash.svelte`: backdrop + card, an explainer + a couple of
  links (repo, "what is this"), and a single "Get started" button wired to
  `dismissWelcome()`. Optionally a "don't show again" is redundant here since
  dismiss already persists — a plain close is enough.

## Interactions / gotchas

- **Prerender**: the static adapter (`adapter-static`, `fallback: index.html`)
  prerenders. Guard all `localStorage` access behind `onMount`/`browser` so the
  splash never renders into the prerendered HTML (which would flash for every
  visitor regardless of stored state).
- **Theme**: the splash should use the same CSS variables as everything else so
  it inherits whatever theme is active (including Retro). See the theme system
  in `app.css` / `lib/ui/theme.svelte.js`.
- **Don't couple to the theme key.** Welcome uses its own `waffle:welcome-seen`;
  theme uses `waffle:theme`. Keep them independent.
- **Testing**: a GUI spec should (1) see the splash on a fresh context, (2) see
  it gone after dismiss + reload, and (3) see it reappear after simulating a
  `WELCOME_VERSION` bump (set the stored key to a lower number).

## Explicitly out of scope for now

- No "what's new" changelog feed, no remote config, no analytics. A source
  constant + one localStorage key is the whole design.
