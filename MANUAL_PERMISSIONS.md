# Granting Permissions Manually (macOS)

SpeechX needs three macOS permissions to work: **Accessibility**, **Input Monitoring**, and
**Microphone**. Each has a "Grant Access" button in the dashboard's Permissions tab that
should walk you through it automatically — but macOS's own permission dialogs don't always
appear reliably (this is a known macOS quirk, not something SpeechX can fully control; see
the note on Input Monitoring below for the specific reason it happens most often there). If
a button doesn't seem to do anything, use the manual steps here instead.

All three live in the same place: **System Settings → Privacy & Security**, then the
specific category below.

---

## Accessibility

**Why SpeechX needs it:** Accessibility access is what lets SpeechX watch for the Right ⌘
dictation key globally — while any other app is focused, not just while SpeechX's own
window is open.

**Manual steps:**
1. Open **System Settings → Privacy & Security → Accessibility**.
2. Find **SpeechX** in the list and turn its toggle on.
   - If SpeechX isn't listed at all, click the **+** button beneath the list, navigate to
     `/Applications/SpeechX.app`, and add it — then turn its toggle on.
3. If SpeechX was already running, quit and reopen it once — a permission granted to an
   already-running process doesn't always take effect until relaunch.

---

## Input Monitoring

**Why SpeechX needs it:** this is a separate permission from Accessibility, even though
both gate the same underlying capability. Global keyboard listening on macOS is checked
against Input Monitoring specifically, not just Accessibility — SpeechX needs both.

**Why the automatic prompt is the least reliable of the three:** macOS's Input Monitoring
prompt can silently fail to appear if certain other permission checks have already run
earlier in the same app session — and SpeechX's dashboard checks Accessibility's status
continuously in the background the whole time it's open, which is exactly the kind of
check that can trigger this. In practice this means the "Grant Access" button here is the
one most likely to need the manual steps below instead.

**Manual steps:**
1. Open **System Settings → Privacy & Security → Input Monitoring**.
2. Find **SpeechX** in the list and turn its toggle on.
   - If SpeechX isn't listed, click the **+** button beneath the list, navigate to
     `/Applications/SpeechX.app`, and add it — then turn its toggle on.
3. Quit and reopen SpeechX once after granting.

---

## Microphone

**Why SpeechX needs it:** this is the actual audio SpeechX transcribes — without it,
holding the dictation key records nothing.

**Manual steps:**
1. Open **System Settings → Privacy & Security → Microphone**.
2. Find **SpeechX** in the list and turn its toggle on.
   - If SpeechX isn't listed, it hasn't tried to use the microphone yet — hold the
     dictation key once inside SpeechX to trigger the first request, then check this list
     again.
3. Unlike Accessibility and Input Monitoring, macOS treats microphone access as a
   one-time decision: if you previously clicked "Don't Allow," the system won't prompt
   again automatically — the toggle here is the only way back.

---

## Still not working after granting all three?

Quit SpeechX completely (the red "Quit SpeechX" button in the dashboard, or Activity
Monitor if the window isn't open) and relaunch it. Permissions granted while SpeechX was
already running don't always apply until the next launch, for all three categories above.
