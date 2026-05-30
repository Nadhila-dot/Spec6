# DESIGN.md — Cntrl Panel Design Language

> Audience: another LLM porting this aesthetic to a different dashboard.
> Goal: capture the *why* behind every choice so the look survives reimplementation in a different stack.

The visual identity sits between a **terminal-print zine** and a **modern SaaS console**. It is dense, calm, monochrome, and tactile. The signature is the **135° diagonal hatch** repeated across every elevation level (button, card, sidebar item, footer strip, workspace avatar). Hatching is the project's "fingerprint" — if you only port one thing, port the hatch.

---

## 1. Foundations

### 1.1 Stack assumptions
- Tailwind CSS (v3-style with v4 `@theme inline` block). Most colors are HSL CSS variables, so the same classes work in any framework that supports CSS vars.
- Class merging via `cn()` (clsx + tailwind-merge).
- shadcn/ui primitives as the component skeleton. Card / Button are *re-skinned* (do not use vanilla shadcn — see §4).
- React + TanStack Router, but the design language is framework-agnostic.

### 1.2 Color tokens (HSL triplet `var(--token)` pattern)
All colors are declared as HSL channel triplets (`0 0% 98%`) and consumed as `hsl(var(--token))`. This is what makes the runtime theme switcher work.

**Light mode (`:root`):**
```
--background:   0 0% 100%
--foreground:   0 0% 3.9%
--shell:        0 0% 98%    /* the inner workspace panel — one step away from page bg */
--card:         0 0% 100%
--muted:        0 0% 96.1%
--muted-foreground: 0 0% 45.1%
--border:       0 0% 89.8%
--sidebar-background: 0 0% 98%
--radius: 0.9rem
```

**Dark mode (`.dark`):**
```
--background:   0 0% 3.9%
--foreground:   0 0% 98%
--shell:        0 0% 8.24%   /* the workspace panel lifts ~4% from page bg */
--card:         0 0% 3.9%
--muted:        0 0% 14.9%
--muted-foreground: 0 0% 63.9%
--border:       0 0% 14.9%
```

Notes for the porting LLM:
- **The palette is monochrome by design.** Saturated color is reserved for *semantics* (rose for counts/warnings, amber for role labels, emerald for success, violet for unread dot, red-400 for sign out). Never decorate with color.
- `--shell` is a distinct layer between `--background` and `--card`. Page = background; the rounded workspace panel inside main = shell; nested surfaces = card. Three depth levels, never more.
- `--radius: 0.9rem` is large. Cards use `rounded-xl` (0.75rem) and toplevel containers use `rounded-2xl`. The roundness is what keeps the dense hatching from feeling brutalist.

### 1.3 Typography
- **Body / UI:** Inter (fallback: Avenir, Helvetica, Arial). `font-feature-settings` are default — no special ligatures.
- **Display / brand:** **Chillax** variable (200–700), loaded via `@font-face` at `/images/_unknown/Chillax-Variable.ttf`. Used for the brand name in the sidebar and for `<h1>` headings inside hero cards. Class: `font-chillax`.
- **Mono:** IBM Plex Mono — used **only** for commit hashes, voucher codes, and other identifier-like values. Class: `font-mono`.
- **Numerics:** *always* `tabular-nums` for credits, counts, versions, dates, badges. Non-negotiable.

**Size scale is tiny and precise.** This is a dashboard, not a marketing site.
- Hero `h1` inside header card: `text-[1.4rem]` (~22.4px)
- Section `h2` on the dashboard body: `text-6xl font-bold` (these are intentionally oversized to break the otherwise small UI — a deliberate scale contrast)
- Card titles: `text-[13.5px] font-semibold`
- Nav items / inputs: `text-[13px]`
- Helper text / descriptions: `text-[12.5px]`, `text-[12px]`
- Badges, kbd, version pills: `text-[10–11px]`
- Footer meta strip: `text-[10–10.5px]`

Letterspacing is **negative** on display text (`tracking-tight`) and **wide** on uppercase eyebrows (`tracking-[0.14em]`, `tracking-[0.13em]`).

### 1.4 Radii rhythm
- Avatar/icon tiles: `rounded-lg` (with one extra px on dropdown identity avatar: `rounded-[10.5px]` — micro-adjustment)
- Nav items, tab buttons, secondary chips: `rounded-lg` / `rounded-md`
- Cards, header cards, workspace card: `rounded-xl`
- Dropdowns/popovers: `rounded-2xl`
- Pills, badges, search bar: `rounded-full`

### 1.5 Elevation rhythm
There are essentially **four** shadow presets. Use these; do not invent new ones.
```
/* hairline lift, for active nav items, tabs, search field, product cards */
shadow-[0_1px_2px_rgba(0,0,0,0.05)]

/* product card hover */
hover:shadow-[0_4px_16px_rgba(0,0,0,0.07)]

/* hero header card */
shadow-[0_6px_28px_rgba(0,0,0,0.24)]

/* popovers / dropdown menus (with inner highlight) */
shadow-[0_24px_56px_-10px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,255,255,0.04)]
```
Surfaces are also always **ringed** (`ring-1 ring-border` or `ring-border/60`) instead of using full borders. Ring + shadow + radius = the "tactile chip" feel.

### 1.6 Global motion
```css
* { transition: background-color 0.2s ease, color 0.2s ease, border-color 0.2s ease; }
```
Every element fades color changes. Combined with the diagonal hatch under hover/active states, hovers feel mechanical (snap into a hatched chip) rather than glowy.

App entry uses an `app-container` fade-in (0.3s) to prevent FOUC. Route changes animate via `transition-transform duration-200`.

For richer animations the app uses `framer-motion` with a single house easing curve:
```ts
ease: [0.16, 1, 0.3, 1]   // expo-out-ish, do not substitute
duration: 0.18 – 0.26      // never longer for UI feedback
stagger: 0.055             // grid item entrance
```

---

## 2. The Diagonal Hatch System (the signature)

The 135° pinstripe is everywhere. Internalize it.

### 2.1 The canonical recipe
```css
background-image: repeating-linear-gradient(
  135deg,
  currentColor 0,
  currentColor 1px,
  transparent 1px,
  transparent 6px
);
```
- **135°** always (never 45°, never 90°). 135° reads as "diagonal print pattern", 45° reads as "construction tape".
- **1px line, 5–8px gap.** Spacing of `6px` is the default. `4–5px` for ultra-tight (corner accents). `8px` for ultra-loose (large flat surfaces like the purchase button).
- Drawn with `currentColor` plus a low-opacity container, OR with hardcoded white at 0.02–0.18 alpha. The `currentColor` trick is preferred: it lets the same `<DiagonalAccent />` element inherit foreground in light mode and white in dark mode for free.

### 2.2 Variants and where each is used
There are three named utility classes plus several inline variants:

| Class / inline | Recipe | Used on |
|---|---|---|
| `.diagonal-line-field` | `hsl(--foreground)/0.16`, 5px gap, base `--shell` | Large flat fields (rare; reserved for empty/empty-state backdrops) |
| `.diagonal-line-card` | `hsl(--foreground)/0.14`, 5px gap, base `--shell` | The **outer wrapper** of every Card. Goes *behind* an inner solid card. |
| `.diagonal-line-corner` | `rgba(255,255,255,0.11)`, 4px gap, **radial mask** at top-right | Decorative corner-fade accent on hero panels |
| inline `currentColor / 6px` | low-opacity container, hatch by `currentColor` | Active nav items, active tabs, footer meta strip, button hover ::before |
| inline `rgba(255,255,255,0.16–0.18) / 6px` | hardcoded white | Workspace avatar tile, product icon tile, identity-header avatar |
| inline `rgba(255,255,255,0.04) / 8px` | ultra-faint white | Purchase / primary CTA hatched fill |

### 2.3 The `<DiagonalAccent />` pattern
For interactive items, the hatch is a *separately positioned* absolute layer with `pointer-events-none`, so it never intercepts clicks and it can hold its own opacity independent of the parent text:

```tsx
function DiagonalAccent({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("pointer-events-none absolute inset-0", className)}
      style={{
        backgroundImage:
          "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
      }}
    />
  );
}
```
Then on the active state:
```tsx
{item.isActive && <DiagonalAccent className="text-foreground opacity-[0.045] rounded-lg" />}
```
**Opacity ~0.04–0.06** for active states is the sweet spot — the hatch must be felt, not seen.

### 2.4 The corner radial mask
For hero/header cards, a partial diagonal fades from the top-right corner:
```css
mask-image: radial-gradient(circle at top right, black 0, black 38%, transparent 72%);
```
This breaks the otherwise rectangular grid feel and matches the printed-poster vibe.

### 2.5 Buttons get hatch automatically
The Button component bakes a hatched `::before` overlay into its base class:
```
before:opacity-[0.18]
before:[background-image:repeating-linear-gradient(135deg,currentColor_0,currentColor_1px,transparent_1px,transparent_6px)]
hover:before:opacity-[0.26]
```
Every button — default, secondary, outline, ghost — has hatch underneath its label, intensifying on hover. Keep this when porting; it is what makes buttons feel "stamped".

---

## 3. Layout system

### 3.1 The 3-layer page
```
page bg (--background)
 └── flex h-screen overflow-hidden
      ├── sidebar (--sidebar-background, w-[268px])
      └── main (--background, p-3 sm:p-4)
           └── workspace panel (--shell, rounded-xl border)
                └── <Outlet /> (page content)
                └── Footer (sibling, outside the panel)
```
Note the workspace panel is *inset* from the page edges by `p-3 / p-4`, with `rounded-xl border`. This creates the "card inside the desktop" effect — the whole page is itself a card. Use `min-h-[calc(100vh-1.5rem)]` (or `-2rem` at `sm`) to fill the viewport minus the inset.

The footer lives **outside** the workspace panel but inside the main scroll area. It is its own pill, separated by `mt-3 px-1 pb-1.5`.

### 3.2 Sidebar anatomy (top → bottom)
1. **Brand row** — `px-4 pt-4 pb-3`, brand icon (h-7 w-7, rounded-lg) + app name in `font-chillax text-[15px] font-semibold tracking-tight text-sidebar-foreground/90`.
2. **Search button** — `h-9 rounded-full bg-card`, kbd hint (⌘K) right-aligned, ring + hairline shadow. Opens a command palette.
3. **Section eyebrow row** — small uppercase wide-tracked label (often empty as a spacer).
4. **Main nav** — `flex-1 overflow-y-auto px-2.5`, item spacing `space-y-0.5`. Each item is `h-9 rounded-lg`, icon `17×17`, label `text-[13px]`.
5. **Collapsible groups** ("Links", "Admin") — same shape as nav items but with a `IconChevronDown` rotating 90° when closed. Sub-items live inside `ml-[18px] pl-3 border-l border-border/80`, are `h-8 rounded-md`, and each has a tiny `5px × 2px` rounded "tick" pin to the left of the label.
6. **Workspace card** (footer of sidebar) — sticky to bottom inside a `border-t pt-2 pb-2.5`. Pill-style trigger with hatched avatar tile, name + email, and a sidebar-icon affordance on the right. Opens a `DropdownMenu` *upward* (`side="top"`) containing identity header, credits row, dual action pills (Theme / Account), divider, red sign-out row.

#### Sidebar nav item — exact spec
```tsx
// inactive
"group relative flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-[13px] overflow-hidden
 text-sidebar-foreground/65 hover:bg-card/60 hover:text-sidebar-foreground"

// active
"bg-card text-foreground font-semibold
 shadow-[0_1px_2px_rgba(0,0,0,0.05)] ring-1 ring-border"
// + <DiagonalAccent className="text-foreground opacity-[0.045] rounded-lg" />
```
Inactive icons fade to `/55`, active icons go full-strength. Notification dots are violet (`bg-violet-500`, 1.5×1.5). Numeric badges use a rose pill (`bg-rose-500/15 text-rose-600`).

### 3.3 Page header card (hero)
Every page opens with the same hero card, located in `components/Cards/Header.tsx`. It is one of the most distinctive elements:

```
- relative overflow-hidden rounded-xl
- bg-card with shadow-[0_6px_28px_rgba(0,0,0,0.24)]
- BACKGROUND: <HalftoneCmyk /> shader from @paper-design/shaders-react,
  filling absolute inset-0 at opacity-86. Theme variables drive the CMYK channels.
- OVERLAY: bg-gradient-to-r from-background/42 via-background/10 to-transparent
- CONTENT (z-10): icon tile (h-9 w-9 rounded-xl bg-background/22 backdrop-blur-sm
  ring-1 ring-white/10), then font-chillax title and small description.
  Title uses drop-shadow-[0_4px_12px_rgba(0,0,0,0.72)] so it stays legible on any shader.
```

Porting note: if you don't have the `HalftoneCmyk` shader available, fall back to a layered combo of:
- a dark gradient `bg-gradient-to-br from-zinc-700 via-zinc-900 to-zinc-950`,
- the `.diagonal-line-corner` class on top for the corner accent,
- a subtle grain noise PNG at 6% opacity if available.
Keep the same overlay gradient, icon tile, font, and drop-shadow on the heading — those are what make the hero feel like *this* product.

### 3.4 Section headings (between cards)
Inside the workspace panel, between the hero card and grids, the app uses **deliberately oversized** monochrome headings:
```tsx
<h2 className="text-6xl font-bold px-2">Your Servers.</h2>
<p className="px-4 mb-4">Your powerful servers on the internet</p>
```
This 6xl-against-13px scale jump is a signature move. Use it sparingly — one per scroll viewport — and always with a one-line description directly underneath at default size.

---

## 4. Component patterns

### 4.1 Card (the double-wrapper)
This is the most copied pattern. **Cards in this design are two layers**, not one:

```tsx
<div className="diagonal-line-card rounded-xl border border-border p-3 shadow-sm">
  <div className="flex h-full flex-col gap-6 rounded-lg border border-border bg-card py-6 text-card-foreground">
    {children}
  </div>
</div>
```
- **Outer:** hatched background (`.diagonal-line-card`), border, `p-3` of margin around the inner. This is the "matboard".
- **Inner:** solid `bg-card`, its own border, holds the actual content.

The result is a card that *appears* to float inside its own diagonally-hatched mat, like a print artifact in a frame. Padding (`px-6`, `py-6`) lives on the inner; the outer only provides the gap to the hatch. Never collapse into a single layer.

### 4.2 Product / list cards (single layer)
Lists of many cards (store products, server cards) drop the matboard and use:
```
rounded-xl bg-card/70 ring-1 ring-border/60 p-4
hover: ring-border, bg-card, shadow-[0_4px_16px_rgba(0,0,0,0.07)]
+ framer-motion whileHover={{ y: -2 }} (only when not disabled)
```
Disabled cards drop to `opacity-55`.

Inside each, a **hatched icon tile** establishes the brand:
```tsx
<div className="relative h-9 w-9 shrink-0 rounded-lg overflow-hidden ring-1 ring-white/10">
  <div className="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-950" />
  <div className="absolute inset-0" style={{
    backgroundImage:
      "repeating-linear-gradient(135deg,rgba(255,255,255,0.16) 0,rgba(255,255,255,0.16) 1px,transparent 1px,transparent 6px)",
  }} />
  <span className="absolute inset-0 flex items-center justify-center">
    <Icon className="h-[17px] w-[17px] text-white/75" />
  </span>
</div>
```
Memorize this — it is the canonical "branded icon chip" and is used for product types, workspace avatars (with initials), empty states, etc. Always `zinc-700 → zinc-950` gradient + 0.16-alpha white hatch + ring-1 ring-white/10. Even on light mode it stays dark; this is the one place where the design fixes a tone.

### 4.3 Tabs / pill filter row
Tab buttons are not underlines. They are pills with optional count chips:
```
inactive: text-muted-foreground hover:text-foreground hover:bg-card/60
active:   bg-card text-foreground ring-1 ring-border shadow-[0_1px_2px_rgba(0,0,0,0.05)]
          + an absolute-positioned diagonal at color rgba(255,255,255,0.038)
count chip (active): bg-foreground/8 text-foreground/55
count chip (idle):   bg-foreground/5 text-muted-foreground/55
```

### 4.4 Buttons
shadcn variants, but the base class adds the hatched `::before` (see §2.5). Sizes:
- `default`: h-9 px-4
- `sm`: h-8 px-3 text-xs
- `lg`: h-10 px-8
- `icon`: h-9 w-9

When a button sits inside a hero / shader card, swap to a glass variant: `bg-background/20 backdrop-blur-sm ring-1 ring-white/10`. Same hatched overlay still applies.

### 4.5 Inputs / search
- Standard input: `rounded-lg bg-card ring-1 ring-border/60 h-8 text-[12.5px]`, `focus:ring-border` (no glow ring, just border densifies).
- Sidebar search: full-width `rounded-full bg-card ring-1 ring-border`, with a small kbd inside (`border bg-sidebar-accent/40 px-1.5 py-0.5 text-[10px] font-semibold`).

### 4.6 Dropdowns / popovers
```
rounded-2xl border border-border/60 bg-popover
p-2 (with inner items having their own rounded-xl/rounded-lg)
shadow-[0_24px_56px_-10px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,255,255,0.04)]
```
The shadow has an **inner white highlight ring** (`0 0 0 1px rgba(255,255,255,0.04)`) — important on dark mode for the floating-glass feel.

Item hover state uses `bg-foreground/[0.08]`. The destructive ("Sign out") row swaps to `text-red-400` and `focus:bg-red-500/[0.08]`.

### 4.7 Toasts (Sonner)
Override Sonner's defaults to match the design (see `index.css`):
- All four severities (success/error/warning/info) share the same `--card` bg and 0.6-alpha border. The only differentiator is the **icon color** (`#4ade80`, `#f87171`, `#fb923c`, `muted-foreground`).
- `border-radius: 10px`, `font-size: 13px`, custom shadow with inner highlight.
- Close button is its own pill — `bg-card`, `border 1px`, `top-8 right-8`.

---

## 5. Footer (this is a "studied" footer — replicate carefully)

The footer is a horizontal "meta strip" pill. It is not centered, not paragraph-style, not boilerplate. It is **info-dense**, **monospace where appropriate**, and **hatched on the left**.

```
<footer className="mt-3 px-1 pb-1.5">
  <div className="relative flex items-stretch overflow-hidden rounded-xl ring-1 ring-border/35">

    {/* LEFT: meta strip — px-4 py-2.5, has the diagonal */}
    <div className="relative flex shrink-0 items-center gap-2.5 px-4 py-2.5">
      <div className="absolute inset-0 bg-foreground/[0.025] text-foreground/[0.06]"
           style={DIAGONAL_STYLE} />

      {/* version pill: rounded-full bg-foreground/[0.08] px-2.5 py-0.5
           text-[10px] font-bold tabular-nums text-muted-foreground/55
           ring-1 ring-white/[0.07]  → reads "v1.2.3" */}

      {/* commit: font-mono text-[10px] with <IconGitCommit /> 3×3,
           muted-foreground/40, hover/70 if linked. Hash is sliced to 7 chars. */}

      {/* date: text-[10px] text-muted-foreground/30, locale "MMM D, YYYY". */}
    </div>

    {/* hairline vertical divider: w-px self-stretch bg-border/35 */}

    {/* COPYRIGHT: shrink-0, text-[10px] tabular-nums, "© 2025" auto-extends to "© 2025–2026" */}

    {/* hairline vertical divider */}

    {/* RIGHT: free-form message at text-[10.5px] text-muted-foreground/35
         + a tiny parser that converts <a>url</a> tokens to real links
         with IconExternalLink 2.5×2.5. */}
  </div>
</footer>
```

Why this works:
- The opacity descends across the row (`/55` → `/40` → `/30` → `/35`). The eye scans from most-important (version) to least (the prose). The hatch reinforces that the left is "weighty" metadata.
- Vertical hairlines at `border/35` (not full opacity) tie the strip to the surrounding ring-border without competing with it.
- The text is intentionally tiny (`10–10.5px`). The footer is *furniture*, not content; it is dense the way the back of a vinyl record sleeve is dense.

Port this *exactly* — change the data but keep the geometry.

---

## 6. Iconography & micro-elements

- Mix of **Tabler Icons** (`@tabler/icons-react`, side-loaded via `dist/esm/icons/IconName.mjs`) and **Lucide** (for cases Tabler doesn't cover or where the user-defined link icon system uses a `lu` prefix).
- **Icon size scale:** `h-3 w-3` (10–12px, footer/badges), `h-3.5 w-3.5` (14px, chevrons/kbd), `h-[17px] w-[17px]` (17px, nav and product icons — exactly 17, not 16, not 18), `h-[18px] w-[18px]` (hero card icon), `h-5 w-5` (page-level icons).
- Always stroke icons; never filled. Stroke-width matches Tabler default (1.5).
- A consistent **counter badge** pattern: `inline-flex h-[18px] min-w-[18px] rounded-full px-1.5 text-[10.5px] font-semibold tabular-nums`. Tones: rose (alerts/unread) or neutral (`bg-sidebar-accent text-sidebar-foreground/70`).
- A **pulse / dot** indicator: `h-1.5 w-1.5 rounded-full bg-violet-500` for "new" / unread.

---

## 7. Accent colors (semantic only)

| Use | Token / class |
|---|---|
| Unread / "new" dot | `bg-violet-500` |
| Notification count | `bg-rose-500/15 text-rose-600` (light) / `dark:bg-rose-500/20 dark:text-rose-400` |
| Role / privilege label | `text-amber-400/90`, `text-[9px] uppercase tracking-[0.13em] font-bold` |
| Success state / toast | `text-emerald-400`, `bg-emerald-500/10 ring-emerald-500/20` |
| Error state / sign-out | `text-red-400`, `bg-red-500/[0.08]` |
| Warning icon | `#fb923c` |

These are the *only* sanctioned saturated hues. Do not introduce blues, greens-for-decoration, or gradients-as-accents.

---

## 8. Theme switcher / runtime theming

The CSS uses a `var(--cp-x, fallback)` pattern on every token:
```css
--background: var(--cp-background, 0 0% 100%);
```
This means a runtime theme designer can inject `--cp-*` variables on a `<style>` tag or root element and the entire app re-skins without rebuild. Preserve this pattern if your target app needs themability; otherwise drop the fallback wrapper and just declare `--background` directly.

Hero cards also read a `ThemeConfig` from server-side (`ssr.get('ThemeConfig')`) to drive the `HalftoneCmyk` shader's CMYK channels and source image. If porting, expose those as theme tokens too.

---

## 9. Voice / copy

The product writes with a wink. Examples:
- Dashboard description: *"Your safe space away from; hope its not the fbi"*
- Empty product state: *"No products in this category"* — terse, no exclamation marks.
- Action labels: imperative verbs, never gerunds. ("Redeem voucher", "Try again", "Visit Link".)

Keep tone: short, slightly irreverent, never marketing-y. Periods optional on labels; required on descriptions.

---

## 10. Replication checklist for a new dashboard

Tick these off in order. Skipping any breaks the identity.

- [ ] Add `--background / --foreground / --shell / --card / --muted / --border` HSL tokens with the same value ladder; wire light + dark.
- [ ] Set `--radius: 0.9rem`; expose `rounded-xl/2xl/lg/md` rhythm.
- [ ] Load **Inter**, **Chillax** (variable), **IBM Plex Mono**. Apply Inter to `body`, expose `font-chillax` and `font-mono`.
- [ ] Globally apply the `transition: bg/color/border 0.2s ease` rule to `*`.
- [ ] Implement `.diagonal-line-card`, `.diagonal-line-corner` utility classes; build a reusable `<DiagonalAccent />` component.
- [ ] Re-skin Button so its base class includes the hatched `::before` overlay at `opacity-[0.18] → hover:0.26`.
- [ ] Re-skin Card as the **double wrapper** (outer hatched, inner solid).
- [ ] Build the layout: page (`--background`) → sidebar (`w-[268px]`, own bg) + main (`p-3 sm:p-4`) → inset workspace panel (`bg-shell rounded-xl border`, fills viewport-minus-inset) + Footer below it.
- [ ] Sidebar: brand row (Chillax 15px), full-width rounded-full search with ⌘K kbd, nav items at `h-9 rounded-lg text-[13px]` with active state = hatched card chip, collapsible "Links/Admin" groups with `border-l` rail and "tick" sub-item indicators, **workspace card** at the bottom that opens an upward dropdown with identity header / credits pill / dual action pills / red sign-out.
- [ ] Page header card with shader (or layered gradient fallback) + corner-masked diagonal + glass icon tile + Chillax title with hard drop-shadow.
- [ ] Section heading pattern: `text-6xl font-bold` headline + small description, used sparingly between grid sections.
- [ ] Lists/grids: single-layer cards with the **hatched zinc icon chip** as their visual anchor; framer-motion `y: -2` hover and staggered entrance using the house easing `[0.16, 1, 0.3, 1]`.
- [ ] Tabs as **pills with count chips**, active = card + ring + sub-0.04 hatch.
- [ ] Dropdowns: `rounded-2xl`, big soft shadow + inner white highlight ring.
- [ ] Toasts: Sonner overrides — single card bg, only icon color varies.
- [ ] Footer: horizontal pill, hatched left segment with version pill / mono commit / date, vertical dividers at border/35, copyright, and a parsed message strip with inline `<a>` tokens.
- [ ] Semantic colors only — violet dot, rose count, amber role, emerald success, red destructive. No decorative color.
- [ ] All numerics `tabular-nums`. Always.

---

## 11. Anti-patterns (do NOT do these)

- ❌ Glow rings (`shadow-[0_0_24px_color]`, `ring-blue-500/50` halos). The design uses sharp ring + hatch, never soft glow.
- ❌ Vertical gradients on cards. Card surfaces are flat or hatched, never gradient.
- ❌ Rotated/45° diagonal hatch. Always 135°.
- ❌ Filled icons. Stroke only.
- ❌ Adding accent color to decorate ("make the title blue"). Saturated color = semantic meaning only.
- ❌ Replacing the double-wrapper Card with a single-layer one when used in info panels — the hatched mat is the identity.
- ❌ Auto-uppercasing UI labels. Uppercase is reserved for the small `tracking-[0.13em]` eyebrows.
- ❌ Centered, paragraph-style footers. The footer is a left-aligned, info-dense pill.
- ❌ Single-font designs. The Inter / Chillax / Plex Mono trio must coexist.
- ❌ Replacing `text-[12.5px]` etc. with `text-sm`. The half-pixel sizes (`12.5`, `13.5`, `10.5`) are deliberate; Tailwind's defaults will fatten the UI.
