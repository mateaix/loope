# Loope Desktop Design Spec — Liquid Glass

The visual language for [[2026-06-29-loope-desktop-hub-spec]]. It defines **how Loope
Desktop looks** (a Liquid Glass material system) and pins each surface's **function to the
existing TUI**, so the desktop app is the TUI's capabilities re-expressed as glass — not a
new product with new behaviors.

## Design language

**Liquid Glass** (Apple's iOS 26 / macOS Tahoe material language, "Liquid Glass 2.0"):
translucent layered panels that blur and saturate what is behind them, lit by a thin
**specular edge** and a top sheen, with **continuous-corner** (squircle) radii, **vibrant**
text/icons that stay legible over moving content, and **fluid spring** motion. The product
feels like panes of real glass floating over a soft, colorful depth field.

Principles:

1. **Material, not chrome.** Surfaces are glass tiers, never flat gray boxes. Hierarchy
   comes from translucency + blur depth + elevation shadow, not borders and fills.
2. **Light reveals depth.** Every glass panel has a top specular sheen and a 1px light edge;
   elevation is a soft, large, low-opacity shadow. Layers read as stacked, not painted.
3. **Content is king, glass is quiet.** The diff, the reasoning, the pipeline are the focus;
   glass recedes. Accent color is a *tint*, used sparingly (active run, live node, brand).
4. **Calm, fluid motion.** State changes morph with springs; live work breathes (a gentle
   pulse / shimmer), never spins aggressively. Honor `prefers-reduced-motion`.
5. **One identity across surfaces.** The accent and semantics match the TUI palette so the
   terminal and the desktop app are obviously the same tool.
6. **One surface, not many cards.** A run's content is a *single scrollable transcript
   panel*, not a stack of bordered cards. Each step is a lightweight row — a colored gutter +
   a kind tag + its content — separated by hairlines and iteration dividers, with a quiet
   glass scrollbar. The content gets the space; the chrome stays out of the way.

## Material & token system (CSS variables)

Tiers (background translucency + backdrop blur/saturation):

| Token | Use | Backdrop | Fill |
| --- | --- | --- | --- |
| `--glass-window` | the app window | blur 34 · saturate 190% | white 10% |
| `--glass-panel` | sidebar, pipeline, command bar | blur 14 · saturate 150% | white 7% |
| `--glass-cell` | chips, tags, inset scrim blocks | blur 10 · saturate 140% | white 6% |
| `--glass-raise` | active/selected, hero card | — | white 14–16% |

Light & depth:

- **Specular edge:** `border: 1px solid rgba(255,255,255,.14–.18)` + `box-shadow: inset 0 1px 0 rgba(255,255,255,.30–.40)`.
- **Top sheen:** a panel `::before` of `linear-gradient(160deg, rgba(255,255,255,.20), transparent 32%)`.
- **Elevation:** `0 30px 70px rgba(0,0,0,.45)` (window) down to `0 8px 24px rgba(0,0,0,.22)` (cells).
- **Continuous corners:** radii 26 (window) · 18 (panel) · 15 (cell) · 999 (chips). (Use an
  SVG squircle clip where the platform supports it; large `border-radius` is the fallback.)

Color:

- **Base scene:** a dark depth field (a soft, mostly-dark radial gradient with faint
  brand-hued glows) so glass refraction reads; a light theme uses a bright frosted field.
  The themeable values are CSS tokens on `:root` (dark) with a `body.light` override, and a
  **☾/☀ toggle** switches and remembers the theme. Accent used *as text* has its own token
  (a darker blue in light) so code and links stay legible on white.
- **Brand accent / tint:** Loope blue (`#7FB4FF` family) — the active run, the running node,
  the command prompt. Used as a low-alpha `color-mix` tint, not a solid fill.
- **Agent colors** (match the TUI): Claude blue `#5BA8FF`, Codex orange `#FF9F45`, OpenCode
  violet `#C08CFF`.
- **Agent icons:** each tool carries a small monochrome **brand glyph** (inline SVG) tinted
  to its agent color with a soft glow, shown in its switcher chip and wherever the agent is
  named (pipeline node, step author). A neutral fallback glyph covers any future tool, and a
  dashed `+` chip affords adding one — so the registry stays visibly extensible.
- **Semantics:** ok/converged green `#43E08F`, blocker/failed red `#FF6B6B`, reasoning
  violet, dim text via white-alpha tiers.

Vibrancy (text over glass): primary `rgba(255,255,255,.92)`, secondary `.62`, tertiary `.45`.
Minimum AA contrast is enforced against the *darkened* glass, with a solid scrim behind code
blocks (`rgba(0,0,0,.26)`) so monospace output never loses legibility.

Motion: spring morph on selection/route change; a 1.6s pulse ring on the live run dot; a
1s shimmer on running spinners; all suppressed under reduced-motion (state shown by color).

Fallback: where `backdrop-filter` is unavailable, glass degrades to a solid tinted panel of
equivalent contrast — layout and legibility are identical, only the blur is dropped.

## Functional mapping — TUI → Liquid Glass GUI

Every desktop surface is a TUI capability restyled; behavior and vocabulary are inherited.

| TUI today | Desktop surface (glass) | Inherited behavior |
| --- | --- | --- |
| Home prompt (`loope`) | **Command bar** — a `--glass-panel` bar **docked at the bottom of the window** (a focusable prompt pill + `⌘K`); it is part of the layout, never floats over the cells | type a requirement → run; same default `LoopConfig` |
| `/` slash palette | **Command palette** in the command bar | the same commands (`/iters`, `/preset`, `/implementer`, `/reviewers`, `/verify`, …) |
| Agents status line | **Agent switcher** — tinted chips in the title bar, each with the **tool's brand icon** + ✓/✗/version and a switch caret | pick implementer + reviewer roles; install hint when missing |
| Workspace line (📁 path · ⎇ branch) | **Title bar context** | same project path + git branch/worktree |
| Run list (left pane) | **Runs sidebar** — glass list, selected = `--glass-raise` | up/down select; grouped by project |
| Detail steps grouped by iteration | **Pipeline strip** — a compact sticky header (implement→review→verify nodes + iteration badge) atop the transcript | the same steps/iterations; live node tinted + pulsing |
| Preview: result / diff / transcript | **Transcript panel** — *one* scrollable glass surface; each step is a lightweight row (colored gutter + kind tag), grouped by iteration dividers, long output in a scrim block | the same per-step content; rows expand ⇄ collapse |
| Convergence highlight card | **Convergence hero** — `caught & fixed`, green-tinted glass | the same engine highlight (`Codex flagged → Claude fixed`) |
| Live header (iteration k/N · spinner) | **Title bar live state** + pipeline shimmer | same live stream over the `StepObserver` channel |
| Esc to stop | **Stop control** (`⌫` hint / button) | the engine's cooperative cancel (stops at the next boundary) |
| `d` diff / `t` transcript / `a` activity | **Cell expanders** + a transcript drawer | same artifacts, shown as expandable glass |

The data and events are unchanged — this is the same view model the TUI builds, rendered as
glass instead of cells of text.

## Acceptance (design)

- Each glass tier, the specular/sheen/elevation treatment, the accent/agent/semantic colors,
  the vibrancy text tiers, and the motion set are implemented as reusable tokens/components
  (no per-screen ad-hoc styling).
- Light and dark themes both pass AA contrast for primary/secondary text and code blocks.
- Reduced-motion and no-`backdrop-filter` fallbacks are in place.
- Every surface in the mapping table exists and behaves as its TUI counterpart.
- No external product is referenced by name anywhere in the tree (Apple's "Liquid Glass"
  design language may be named in design docs; nothing else).

## Related

- [[2026-06-29-loope-desktop-hub-spec]] — the app this styles.
- [[2026-06-29-loope-tui-spec]] · [[2026-06-29-loope-tui-commands-spec]] — the functionality
  this mirrors.
- [[2026-06-29-loope-convergence-highlight-spec]] — the highlight behind the convergence hero.
