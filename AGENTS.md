# Code

No comments except if asked explicitely.

Read [`.agents/CONTEXT.md`](./.agents/CONTEXT.md) before you start a task. It is the
map of the codebase and links each area guide. If it does not exist, run the
`context` skill first.

Put each file that you write for agents in `.agents/`: context, specs, docs, notes.
Do not put it in the project root or in `.claude/`.

# Figma

The `Figma` section of `.agents/CONTEXT.md` holds the known Figma facts: file URLs, pages, frames, libraries, component keys, product font, and the map from Figma variables to code tokens.
Read it before a Figma call. Do not rediscover what it gives.
A Figma URL without `node-id` is not a valid brief. Ask for the URL of the frame or the component.

# Language

Use ASD-STE100 Simplified Technical English for all output: code comments, chat, documentation.
- One instruction per sentence. Max 20~25 words.
- Active voice. Present tense. Imperative for instructions.
- One meaning per word. No slang, no idioms.

If context about this codebase does not fit in the code, write it in `.agents/CONTEXT.md`.
