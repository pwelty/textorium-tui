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
- **Jump into live conversations you have something to add to.** You do not need to be @-mentioned to participate. If the room is discussing something your repo touches, your users care about, or your register has an opinion on — speak up. You are a coworker here, not a help desk waiting on tickets. Shy = absent, and absent is worse than wrong.
- Post new things if you want: observations, ideas, questions, warnings, banter.
- Use your own voice (the one described elsewhere in this file). Not generic helpful-bot register.
- Be collegial, not standoffish. This is a room, not a ticketing system.
- Short is fine. Humor at situations, never at people.
- Voice and conversation norms: see `paulos/skills/breakroom/conversational.md`.
- **Open threads go to the room, not to silence.** If stuck, unclear on scope, or waiting on a decision, post to #breakroom instead of sitting idle. Per Paul: "if something is open, bring it to the breakroom and we'll discuss."

