## Cross-project communication via tmux

Each project runs in its own tmux session. You can ask another project's Claude agent to do something by sending keys to its session.

### Sessions

| Project | tmux session | What it does |
|---------|-------------|--------------|
| paulos | `paulos` | Orchestration, CLI tools, automation infrastructure |
| authexis | `authexis` | Content intelligence platform, social posting, outreach |
| polymathic-h | `polymathic-h` | Blog (paulwelty.com), Hugo site, podcast |
| synaxis-h | `synaxis-h` | Consulting firm site and operations |
| Textorium | `Textorium` | Native Mac editor for static sites |
| skillexis | `skillexis` | Conversation skills flight simulator |
| scholexis | `scholexis` | Academic task manager for neurodivergent students |
| textorium-tui | `textorium-tui` | Terminal interface for static site generators |
| phantasmagoria | `phantasmagoria` | AI-powered Stellaris event generator |
| eclectis | `eclectis` | Personal content intelligence / RSS |
| newsletter | `newsletter` | Weekly newsletter pipeline |

### How to send a request

```bash
tmux send-keys -t <session> 'your request here' Enter
```

**Always end with `Enter`** — without it the text sits in the prompt and nothing happens.

### Rules

- **Just send it.** The injected text lands in the target session's context naturally — no special coordination needed for most requests.
- **For long output, write to a file.** If you need structured data back, tell the target to write results to `/tmp/something` and read the file. Pane capture only shows what's visible on screen.
- **Check the session is idle first.** `tmux capture-pane -t <session> -p | tail -5` — look for a prompt, not a running command.
- **Don't send to sessions with low context.** Low-context sessions may fail silently.
- **Use the base session name** (e.g. `paulos`, not `paulos-7`). If the session uses groups, tmux routes to the right pane.

