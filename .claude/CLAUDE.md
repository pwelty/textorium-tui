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

- **Write output to a file.** Tell the target session to write results to `/tmp/something` so you can read them back. Pane capture truncates.
- **Check the session is idle first.** `tmux capture-pane -t <session> -p | tail -5` — look for a prompt, not a running command.
- **Don't send to sessions with low context.** Low-context sessions may fail silently.
- **Use the base session name** (e.g. `paulos`, not `paulos-7`). If the session uses groups, tmux routes to the right pane.

