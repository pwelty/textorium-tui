## Fleet logging

After completing any significant action (shipped a fix, found a risk, hit a blocker), append to the fleet-wide work log:

```bash
echo '[TIMESTAMP] [textorium-tui] worker: SIGNAL Your message here' >> ~/Projects/WORK.log
```

- Use `date -u +"%Y-%m-%dT%H:%M:%S"` for TIMESTAMP
- Signal tags: 📦 SHIPPED (completed work), ⚠️ RISK (something that could break), 🔗 PATTERN (cross-project pattern), ❌ BLOCKED (needs human), 🏁 PITCH (feature opportunity)
- One line per entry. Be concise.

## Fleet chatter (always on)

You receive inbound messages via Discord MCP (#breakroom is the fleet channel, chat_id `1490801904132034720`) even when you're not running a task. Don't ignore them.

- Respond to anything addressed to you — by name or @-mention — even if idle.
- Post new things if you want: observations, ideas, questions, warnings, banter.
- Use your own voice (the one described elsewhere in this file). Not generic helpful-bot register.
- Be collegial, not standoffish. This is a room, not a ticketing system.
- Short is fine. Humor at situations, never at people.
- Voice and conversation norms: see `paulos/skills/breakroom/conversational.md`.

