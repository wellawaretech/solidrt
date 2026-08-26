---
title: Shell layout fixes found rendering NavShell beside SplitView
description: Navigation follows the pane count instead of its own breakpoint, and TextInput/Select share Button's vertical padding so controls in a row are one height.
created: 2026-08-26
completed: 2026-08-26
---

Both found by building a messenger shell (NavShell wrapping a SplitView) and
looking at it at several widths. Neither component had ever been rendered
together before.

## 1. Navigation follows the pane count

`defaultPolicyResolver` derived `navigation` and `layout` side by side from
the same `windowSizeClass`, as if they were independent. The middle band
(600-839 px) produced `rail` + `singlePane`: a 72 px vertical strip pinned to
the left of a window already too narrow for two panes, spending the axis that
is scarce. At 621 px the shell gave 72 px to navigation and still showed one
pane.

Now `layout` is computed first and `navigation` derives from it:
`twoPane -> sidebar`, `singlePane -> bottomTabs`. A side strip may only sit
beside a two-pane layout.

| width | layout | navigation before | navigation now |
|-------|--------|-------------------|----------------|
| < 600 | singlePane | bottomTabs | bottomTabs |
| 600-839 | singlePane | rail | bottomTabs |
| >= 840 | twoPane | sidebar | sidebar |

`rail` stays a valid `NavigationPolicy` but is no longer a default output; its
meaning is now "the narrow side nav an app picks for a content-dense two-pane
layout", set through `setPolicy`/`setPolicyResolver`. Apps with no list-detail
split lose the rail at 600-839 px and override the same way.

Deliberate non-goals:

- `rail` as the two-pane default instead of the 220 px `sidebar`. The 840 px
  arithmetic favours it (220 + 320 list leaves 300 px of detail, 72 + 320
  leaves 448) but with three size classes that would make `sidebar` never a
  default output either. Kept `sidebar`; a one-word flip if it proves cramped.
- `policy.interaction` in the rule (bottom tabs are a thumb idiom). One input
  keeps the resolver predictable.

## 2. Controls in a row share a height

A `TextInput` beside a `Button` rendered 27 px against 35 px: both center the
same 21 px body text, but the field and the `Select` trigger padded
vertically with `space("sm")` while `Button` and `SegmentedControl` used
`space("md")`. The fix is the smallest one: `TextInput`/`RichTextEditor`
(editor-field.tsx) and the `Select` trigger now pad with `space("md")` too, so
all four interactive controls are one height (35 px compact, 37 px
comfortable) and keep scaling together under density and `textScale`.

Deliberate non-goal: a shared control-height token or `controlPadY()` helper.
Uncommon in component libraries, and the four controls all derive their
height from body text plus the same padding token, so there is nothing to
share yet. A `minHeight` baseline becomes worth it only for controls whose
content is not body text (icon-only buttons, custom children).
