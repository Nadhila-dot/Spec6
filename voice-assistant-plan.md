# Spec6 Voice Assistant Plan

## Goal

Build a **voice-to-voice company copilot** inside Spec6.

This is not just speech-to-text on the existing chat box. It is a dedicated company tab where a founder can talk to Spec6 naturally, hear answers back, and keep the assistant running in the background while doing other work.

Core idea:

- Each company gets a **Voice Assistant** tab.
- The assistant has access to the same company context as chat:
  - onboarding profile
  - company overview
  - TriggerWare alerts
  - Watchtower scans
  - Cognee memory
  - live web research through Bright Data
- The user can interrupt, ask follow-up questions, and get spoken answers in real time.

This should feel like:

- "How are we doing today?"
- "Anything bad happening with customer sentiment?"
- "Did Nike launch anything important this week?"
- "Read me the top 3 risks right now."
- "Set an alert if our Trustpilot score drops again."

---

## Product Shape

### New company sub-tab

Inside each company, add a new tab:

- `Chat`
- `Overview`
- `Voice Assistant`

The `Voice Assistant` tab is a dedicated always-on workspace, not a modal and not a tiny mic button bolted onto chat.

### User experience

The user opens a company and sees:

- assistant status
  - listening
  - thinking
  - speaking
  - idle
- current transcript
  - partial live transcript while speaking
  - finalized transcript after turn end
- current spoken answer
  - partial or chunked answer text as it is being prepared
- recent voice history
- quick actions
  - mute
  - stop speaking
  - push to talk / continuous mode
  - "summarize what changed"
  - "read latest alerts"
  - "create trigger"

### Background mode

The assistant should support a background mode:

- user keeps the voice tab open while doing other work
- user can ask quick questions on the fly
- assistant can surface proactive spoken alerts if enabled
- assistant can also remain silent and only respond when asked

This should behave more like a **personal operator** than a demo chatbot.

---

## What the Voice Assistant Actually Does

The voice assistant is the same Spec6 intelligence engine wrapped in a conversational audio loop.

For every spoken turn it should be able to:

1. transcribe speech in real time
2. understand the company context
3. decide whether existing memory is enough
4. call live tools when needed
5. synthesize an analyst answer
6. speak the answer back
7. save the exchange to company memory

That means voice must plug into:

- company onboarding context
- saved overview dossier
- Bright Data tool loop
- TriggerWare change signals
- Cognee memory graph
- Watchtower autonomous refreshes

---

## Architecture

## 1. Audio input

Use browser mic capture inside the frontend.

Preferred path:

- `getUserMedia`
- streaming audio chunks to backend over WebSocket

Requirements:

- low-latency partial transcript updates
- clear start/stop controls
- interruption support
- support for push-to-talk first, continuous listening later

## 2. Speech recognition

Use **Speechmatics real-time transcription** as the primary speech-to-text provider.

Why:

- enterprise-grade realtime STT
- fits the "always-on assistant" concept
- avoids trying to build a voice product around batch transcription

Backend responsibility:

- proxy Speechmatics requests through Rust
- keep API keys server-side
- normalize transcript events for the frontend

Speechmatics output should be split into:

- partial transcript events
- final transcript events

Only final transcript events become company memory by default.

## 3. Voice session loop

Add a voice session orchestrator in the backend.

Responsibilities:

- maintain session state per company/user
- receive finalized transcript turns
- call the existing agent loop
- stream assistant text back incrementally
- trigger text-to-speech playback
- handle interruptions

This should reuse as much of the existing chat agent loop as possible.

The voice assistant is not a separate intelligence product. It is a new interaction layer.

## 4. Intelligence layer

Voice uses the same backend intelligence stack as chat:

- Cognee for long-term memory
- Bright Data for live research
- TriggerWare for deltas and scheduled triggers
- Watchtower for autonomous company refreshes
- saved overviews for baseline context

Extra voice behavior:

- short spoken answers by default
- optional "expand on that"
- optional "read me the evidence"
- optional "go deeper"

## 5. Speech output

Text-to-speech should be added after the speech-to-text loop is stable.

Requirements:

- interruptible playback
- chunked playback for long answers
- different response styles
  - concise briefing
  - executive summary
  - evidence mode

If Speechmatics TTS is used, keep the same rule as STT:

- backend handles provider credentials
- frontend only receives stream/audio playback artifacts

---

## Conversation Design

### Default assistant behavior

The voice assistant should speak like a sharp operator, not a generic AI demo.

Desired behavior:

- lead with the answer
- keep first response short
- mention uncertainty clearly
- only go deep when asked
- use current date and current company state
- mention sources when making concrete claims

### Example turn

User:

> "How are we doing this week?"

Assistant:

> "This week looks mixed. Customer trust is still the main weakness, but there is no major new compliance event. The biggest live risk is returns and review friction. I can read the latest alert or compare you to Nike and Adidas if you want."

### Voice-native commands

Add strong first-class commands:

- "What changed since yesterday?"
- "Summarize the latest alerts."
- "What should I worry about right now?"
- "Compare us to our top 3 competitors."
- "Create a trigger for bad reviews."
- "Tell me when competitor pricing changes."
- "Read the latest Watchtower scan."
- "Pause alerts."
- "Stop."

---

## Company Tab Design

The `Voice Assistant` tab should include these sections:

### Top state bar

- session status
- selected mic/input device
- selected voice/output
- company name

### Center pane

- large transcript area
- live assistant response area
- current alert / tool activity summary

### Side panel

- latest company alerts
- trigger health
- recent scans
- quick prompts

### Bottom controls

- mic button
- push-to-talk
- continuous listening toggle
- interrupt / stop speaking
- mute output
- send typed fallback input

---

## TriggerWare Role In Voice

TriggerWare is not the main scanner.

TriggerWare should be used for:

- recurring structured change detection
- business-system deltas
- trigger polling
- webhook/notification workflows

Voice assistant usage:

- "Read my active triggers."
- "What fired this morning?"
- "Create a trigger for new negative reviews."
- "Pause that trigger."

If TriggerWare has no usable connector coverage for a company use case:

- Spec6 says so directly
- falls back to Spec6-managed monitoring
- still lets the user continue the conversation

This is important for enterprise trust.

---

## Cognee Role In Voice

Cognee should become the memory backbone of voice.

Store:

- finalized spoken user turns
- assistant answers
- trigger events
- major overview findings
- company risk summaries
- follow-up decisions

Voice-specific memory examples:

- "The founder cares most about pricing pressure and customer trust."
- "Last week the founder asked to prioritize Nike and Adidas."
- "The founder prefers short spoken summaries unless asked for evidence."

That turns the assistant into a real company-side operator over time.

---

## Phased Delivery

## Phase 1 — Voice input MVP

Scope:

- new `Voice Assistant` company tab
- browser mic capture
- Speechmatics real-time STT
- live transcript
- finalized transcript sent into existing Spec6 chat loop
- typed answer still rendered as text

Success:

- user can speak a question and get a text answer inside the voice tab

## Phase 2 — Full duplex assistant

Scope:

- assistant speaks back
- interrupt assistant while speaking
- short-answer voice style
- transcript + answer history saved per company

Success:

- usable 1-on-1 conversational company assistant

## Phase 3 — Proactive company operator

Scope:

- background listening mode
- optional spoken alert summaries
- TriggerWare alert reading
- "what changed" briefing mode
- persistent voice preferences in memory

Success:

- feels like a personal assistant for company health, not just voice input

## Phase 4 — Enterprise polish

Scope:

- device selection
- noise handling
- session reconnection
- audit log of voice actions
- role-aware controls
- rate limits / spend controls

Success:

- safe to demo as a serious enterprise product

---

## Technical Workstreams

## Backend

- add `speechmatics.rs`
- add voice session WebSocket route
- add transcript event types
- add assistant voice session state manager
- wire voice turns into existing agent loop
- save final turns into Cognee
- expose TriggerWare alert reads for voice

## Frontend

- new `Voice Assistant` tab in company view
- mic capture and WebSocket streaming
- transcript rendering
- live assistant response rendering
- playback controls
- quick voice prompt chips

## Intelligence

- adapt prompt style for spoken answers
- shorten default answers
- support "expand" and "evidence mode"
- prioritize current alerts and latest overview deltas

## Ops / governance

- usage caps
- session timeouts
- provider health fallback
- provider config validation
- logging and audit trail

---

## Risks

### Latency

If speech recognition + tool calls + synthesis + TTS all stack, the experience becomes sluggish.

Mitigation:

- short spoken responses
- partial transcript instantly
- partial answer text before TTS
- only run live tools when needed

### Interruption complexity

Full duplex voice is much harder than text chat.

Mitigation:

- push-to-talk first
- continuous mode later
- explicit interrupt button

### Provider brittleness

Speechmatics, TriggerWare, and Bright Data are all external dependencies.

Mitigation:

- backend abstraction layer
- clear fallback states
- direct error messaging

### Cost

Always-on voice can burn credits fast.

Mitigation:

- bounded sessions
- aggressive caching through Cognee
- tool loop guardrails
- only trigger live scans when evidence is stale or missing

---

## Success Criteria

This feature is successful if:

- a founder can open a company and speak naturally
- Spec6 answers from real company context, not generic memory
- it can pull live data when needed
- it can read changes, risks, and competitor shifts out loud
- it remembers prior voice conversations
- it feels useful while the founder is doing other work

If it feels like "chat, but with a microphone," it failed.

If it feels like "a personal company operator in my browser," it worked.

---

## Best Demo Version

For the hackathon/demo version, the strongest story is:

- open company
- switch to `Voice Assistant`
- ask:
  - "How are we doing today?"
  - "What changed this week?"
  - "Compare us to Nike."
  - "Create a trigger for negative reviews."
- assistant responds instantly with:
  - live transcript
  - clear short spoken brief
  - visible source-backed reasoning
  - company memory continuity

That is much stronger than a generic chatbot mic addon.
