# Roadmap - Textorium TUI

## Current Quarter (Q1 2026: Jan-Mar)

**Theme:** Ship quality
**Big Bet:** Someone installs via Homebrew and thinks "this is a real tool"

### Milestones
- [ ] TUI on Homebrew (Target: Feb 21)
  Distribution pipeline complete: GitHub Actions builds, Homebrew tap, landing page updated. Already 86% done — remaining work is landing page updates (SYN-99, SYN-103).
- [ ] Visual overhaul (Target: Feb 28)
  Better layout, colors, status indicators, responsive panes. The TUI should look polished, not like a prototype. Typography, alignment, and information density should feel intentional.
- [ ] Bulletproof editing (Target: Feb 28)
  Edge cases handled (arrays, nested frontmatter, empty fields). Undo support, confirmation on destructive actions, clear feedback on all operations. No data loss scenarios.
- [ ] CLI essentials (Target: Mar 15)
  `textorium new "Title"` and `textorium list` working end-to-end. These are the two commands someone would actually use alongside the TUI.
- [ ] First external user (Target: Mar 31)
  Someone outside the author installs it, uses it on their site, and gives feedback. Real validation that the tool delivers value.

### Why this quarter

Ship quality is about crossing the threshold from "working prototype" to "real tool." The TUI already scans 700+ posts instantly and handles search/sort/filter, but first impressions matter. A Homebrew user who hits a rough edge in the first 30 seconds won't come back.

The through-line: distribute (Homebrew) → polish (visual + editing) → extend (CLI) → validate (external user). Each milestone builds on the previous one.

---

## Next Quarter (Q2 2026: Apr-Jun)

**Theme:** Content operations
**Big Bet:** Textorium becomes faster than any other way to manage a static site

### Open questions
- [ ] What workflows do external users actually want?
- [ ] Should `textorium serve` and `textorium build` wrap SSG commands?
- [ ] Is there demand for bulk operations (batch publish, batch tag)?
- [ ] Does the macOS app need feature parity, or do they serve different users?

---

## Past Quarters

*No past quarters yet — this is the first roadmap.*

---

## Roadmap principles

**This roadmap is:**
- Quarterly themes, not rigid project plans
- Big bets, not comprehensive feature lists
- Strategic direction, not tactical execution
- Living document, updated as you learn

**How to use this:**
- Review quarterly: What theme for next 3 months?
- Plan weekly: Which milestones are active?
- Reference daily: Does this TODO align with current theme?
- Learn continuously: Update "Learnings" when quarter ends
