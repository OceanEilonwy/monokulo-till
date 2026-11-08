# Motion: the showcase

> The app was called Monokular until 2026-11-08, when it became Till
> ("Monokulo Till" in full). This record keeps the names it was written
> with; see [decisions/core.md](../decisions/core.md) 22 for the new ones.

What the motion work did to the Android till, for the owner. Written by
the lead designer on 2026-10-30. Round 2 comes first and is written
against `motion-main` at `1afc52b` (its code is `f2dd97f`, round 2's
final build; the commits after it are reports). Round 1, further down,
was written against the integrated `motion` branch at `5fcf41e` (code
`b03c864`). The spec is [`docs/motion.md`](../motion.md) (§12 and §13
for round 2), the numbers are [`motion/baseline.md`](motion/baseline.md)
and [`motion/after.md`](motion/after.md), and the calls are decisions
147 to 303 in [`decisions/android.md`](../decisions/android.md).

## Round 2: the waiting tray, Back, and the logo bubble

What round 2 changed, from the owner's decisions of 2026-10-30 and the
design he approved for them (the handoff canvas; `docs/motion.md` §12,
decisions 266 to 268). The canvas is the only design this section
describes. Where it is quoted, it is the approved artboard (`Main`,
`TrayOpen`, `TrayDark`, `TrayFew`, `BackSequence`, `TraySpec`,
`BubbleStates`, `BubblePrototype`, `BubbleSpec`), so an implementor has
one design to build to, not a choice. The build is `motion-main` at
`f2dd97f`, which includes every round-2 fix through decision 302; the
commits after it are reports.

### The clips

Each "after" clip in [`motion/after-r2/`](motion/after-r2/) was recorded
today on that build on the Pixel 9 Pro emulator (`emulator-5558`, light
theme), against the local Monokulo and the stand-in Square, with real
sales rung up and paid through the dev stack. Each "before" clip in
[`motion/before-r2/`](motion/before-r2/) is `65a81c7`, the till as round
1 left it, on the same emulator. All are 480 px wide (the bubble's crop
300 px) and under 115 KB. The caveat from round 1 still applies: the
emulator renders on the host's CPU and its recorder keeps a frame only
when the picture changes, so read the clips for *what* happens, not for
how smooth it is. The grey "This request could not be authorized · Sign
in to Square again" notice at the top of the shift is the test till's
Square connection on the dev stack. It is not part of the design.

| Change | Before | After |
| --- | --- | --- |
| Back with a sale waiting | [back-with-sale-waiting](motion/before-r2/back-with-sale-waiting.mp4) | [back-with-sale-waiting](motion/after-r2/back-with-sale-waiting.mp4) |
| Back into a folded row (the tray opens in place) | [back-with-sale-waiting](motion/before-r2/back-with-sale-waiting.mp4) (no tray) | [back-into-folded-row](motion/after-r2/back-into-folded-row.mp4) |
| The tray folded and opened | (no tray) | [tray-open-fold](motion/after-r2/tray-open-fold.mp4) |
| The paid fold and step-back | [paid-step-back](motion/before-r2/paid-step-back.mp4) | [paid-step-back](motion/after-r2/paid-step-back.mp4) |
| The bubble: idle, waiting, paid, idle | [bubble-states](motion/before-r2/bubble-states.mp4), [close](motion/before-r2/bubble-states-close.mp4) | [bubble-states](motion/after-r2/bubble-states.mp4), [close](motion/after-r2/bubble-states-close.mp4) |
| Animations off: Back into a folded row | | [reduced-motion-back](motion/after-r2/reduced-motion-back.mp4) |

The tracks' and reviews' own round-2 recordings are evidence for the
record: `bubble-states-r2*.mp4`, `bubble-live-paid-r2.mp4`,
`back-with-sale-waiting-r2.mp4`, `paid-step-back-r2.mp4`,
`tray-open-fold-r2.mp4`, and the review clips
`back-from-folded-row-r2-review.mp4`,
`paid-fold-and-step-back-r2-review.mp4` and
`tray-fold-label-r2-review.mp4` in [`motion/`](motion/). The last three
were recorded before fixes 296 to 302 went in.

### 1. Back lands on the shift, and what waits goes in the tray

**Before** (decision 258). On a sale, with another sale rung up in the
meantime, Back did not reach the shift. The first sale's words faded,
the waiting sale's code replaced the first sale's code in one frame, and
the shift was never drawn. A second Back then shrank that sale into the
plain list. The cashier pressed Back and got a different sale.

**After** (decisions 266, 282, 289, 296, 297, 299, 300). Back always
lands on the shift. The sale's words fade (`fadeQuick`), its card shrinks
on `surface` into its row in the amber "Waiting for payment" tray, the
code stays whole on the card while the card can hold it, and the row's
words fade in at their own size. A sale that arrived meanwhile is
already a row in the tray. Its bubble and notification still call, but
it never takes over the screen
(`after-r2/back-with-sale-waiting`: #1856 arrives while the cashier is on
#1855, and Back lands #1855 in the tray's second row under it). A sale
that arrives while the shift is on screen and at rest still rises by
itself, because that is what the till is for (§12.6 4).

- **Back into a folded row.** The sale being left can be the fourth or
  later live sale, whose row is folded away. The tray then opens in the
  composition that starts the Back (289), so the row exists on the first
  frame, and the card lands in it with the code carried inside (299). In
  `after-r2/back-into-folded-row`, #1874 rose by itself, four more were
  rung while it showed, and Back lands it in the fifth row, with "Show
  fewer" under the open tray. The tray stays open until "Show fewer" or
  until the cashier leaves the shift.
- **A row tapped far down the list.** Back brings the list back to where
  it was when the row was tapped (296, 300), so the card lands where the
  finger left it. A row that would then be off screen is put back under
  the finger instead. A cancelled back peek keeps that place (297).
- **A second Back while the shift is still arriving** lands on the shift
  and never leaves the app (260 c, decision 283).
- **Interruption.** A tap on a tray row during the landing starts the
  next grow from where the stage is (249). A sale that arrives during
  the landing goes into the tray under the moving card and does not take
  the move over.
- **Animations off.** A 120 ms crossfade with no movement. The tray
  snaps open when it must (`after-r2/reduced-motion-back`).

### 2. The waiting tray (A4, opens in place)

**Before.** There was no tray. Waiting sales were rows among the rest of
the shift, and a sale that waited unseen had nowhere to be seen.

**After** (decisions 266, 269, 280, 281, 291, 292, 295). Under the chips
are the header "Waiting for payment" with its count, then an amber
`warnGround` tray holding a white `card` of every live sale, newest
first. After three rows the tray folds into a 48 dp row, "Show N more
waiting", in `ink` with a chevron. Tapped, the tray opens in place and
the row reads "Show fewer". Everything else is under "Earlier". With no
live sale there is no tray and no headers, and the shift reads as it
always did. There is no "New" tag and no bottom bar.

- **Motion.** Opening is the extra rows fading in where they will sit
  (`fade`) while "Earlier" moves down on `settle`. Folding is the
  reverse on `fadeQuick`. The tray has no height animation of its own:
  it is its rows (`after-r2/tray-open-fold`). The fold's label fades
  through, never across (291), and the chevron turns 180° in the layer.
- **Under the finger.** Folding a scrolled tray keeps the fold row at the
  pixel it was tapped at (292). A filter chosen after that still moves
  its rows (295).
- **Animations off.** Opening and folding jump, and the labels swap.

### 3. The paid step-back is Back

**Before** (decision 259). After the paid page's 5 s dwell, the page's
words faded and the shift's list faded in. No surface moved, so the paid
sale went nowhere.

**After** (decisions 266, 285, 300). After the dwell, the paid page
leaves exactly as a Back pressed on it does. The same `RowGrow` reverse,
with the sale's course, shrinks the page into its row, which by then
sits at the top of "Earlier". The hero's tick becomes the row's badge.
The list comes back at the top, so the title, the chips and the tray
are in view and the paid row is the first under "Earlier" (300). On the
test clock it is the Back frame for frame, within half a pixel
(`thePaidStepBackIsTheBack`, light and dark). In
`after-r2/paid-step-back` the emulator shows only four frames of the move
because of the two stalls in the performance section. The landing is the
point: #1861 paid, and its row at the top of Earlier with the tray above.

- **Interruption.** Back during the dwell runs the same exit at once.
- **Animations off.** A 120 ms crossfade, as Back is.

### 4. The frames tidied (260 a, b, c)

- **(a) The code is never missing from its card** (289, 296, 299). On a
  Back from a live sale and on "Close it for now", the code is drawn
  whole on its card on every frame the row's ground holds the card. On a
  Back into a low row, the card carries its code inside it, moved by
  whole pixels so the modules stay sharp and the QR still scans, until
  the shift's words are in. A code with no row to go to fades out over
  the move. No frame shows bare ground. Round 1's evidence is
  `review-r6/phone-back-into-row.webp`. The fix is in
  `after-r2/back-into-folded-row` and in device tests that check every
  frame (`backIntoAFoldedRowOpensTheTray`,
  `backAfterASelfRiseIntoARowUnderTheFoldIsNeverBare`, light and dark).
- **(b) One glyph in the card at a time** (286, 290). In the sale
  screen's paid fold, the leaving code fades on the fold's own progress
  and is gone by 27 % closed. The tick comes in over the fold's last
  share (`crossfadeOnTrack`), so it is never drawn over the code. Round
  1's evidence is `review-r6/phone-paid-fold-on-sale.webp`, frames 3 to
  6. The fix is at the start of `after-r2/paid-step-back`.
- **(c) A second Back stays in the till** (283): see change 1.

### 5. The bubble is the logo (B5 c, the border chases)

**Before.** The idle bubble was the grey mark. A waiting sale lit it with
the loading mark's glints and a pulse. A payment closed a green ring
round a tick on a pale green ground.

**After** (decisions 267, 270 to 275, 293). One mark is drawn by `mark()`
in every state, and only its colours and its border change
(`after-r2/bubble-states`, `bubble-states-close` at 2×):

- **Idle.** A grey mark with a grey border, the whole bubble at 55 %, on
  a white ground at 90 %.
- **Waiting.** A still orange lens with no glints. The border becomes a
  pale #D8D2C7 track, and a black #1A1917 segment, 30 % of the border,
  chases clockwise round it from 12 o'clock at constant speed, one turn
  per 1250 ms (`Motion.chase`). The pulse rings out behind it, as
  before, only while waiting and with the panel down.
- **Paid.** The glass fills green from the core out, in four rings 60 ms
  apart (`Motion.spread`). A white tick draws over it (`Motion.tick`)
  once the core is green, and the border is whole and black. There is
  no spinner, no green ring and no pulse. After `Dwell.PAID_MS` the
  bubble returns to Idle the same way: green to grey, and the tick fades
  out whole on `fadeQuick`, never un-drawn (293).
- **Colours.** All are `Fixed`, the same in light and dark, because the
  bubble sits over other apps. The two greens and the idle grey moved
  into `Fixed` beside `Paid`. The ground is white in every state, and
  `paidGround` left the bubble.
- **TalkBack.** The bubble's window carries what the border no longer
  says: "Monokular, $12.00 waiting for payment", then "seen on the
  network", "confirming", "paid", or "nothing waiting".
- **Interruption.** A payment that lands mid-turn stops the chase where
  it is and fades the arc out as the track darkens (274). A sale that
  ends mid-spread retargets each ring from where it is. A payment that
  lands again during the tick's fade brings the tick back whole.
- **Animations off.** No chase, so a waiting bubble shows the whole
  border in ink. No pulse. The lens changes in one 120 ms fade, and the
  tick appears whole.
- **Cost.** Idle draws no frames. The chase's loop exists only while the
  bubble is lit and shown (`onlyTheLitBubbleRunsALoop`). One frame of
  the mark allocates nothing (`MarkOnDeviceTest`), and nothing
  recomposes through the chase, the spread or leaving Paid
  (`OverlayRecompositionsTest`).

### Round 2's performance

All of this is from [`motion/after.md`](motion/after.md), round 2's
sections. Before is round 1's integrated build (`b03c864`) or round 2 as
first integrated (`03116a5`, then `b692f01`), measured alternately in
one session on the same emulator. Round 1's caveat applies: the
emulators render through SwiftShader, so a pair says whether something
is wrong, not how it feels.

| Flow (`MainActivity`, last 120 frames) | Round 1's build | Round 2 |
| --- | --- | --- |
| Shift scroll with a full tray, phone: p99 (frames over 33 ms) | 68 and 58 ms (53, 15) | 44 and 32 ms (11, 1) |
| Back into the tray ×10, phone: p99 | 78 to 85 ms (round 1's Back) | 73 to 80 ms |
| Back into the tray ×10, tablet: p99 | 84 to 104 ms | 74 to 84 ms |
| Tray opened and folded ×5, both emulators: p50 / p99 | (no tray) | 10 to 19 / 39 to 53 ms |
| Paid step-back, phone: frames, max | 12, 210 to 237 ms (round 1's crossfade) | 12 to 18, 159 to 246 ms |
| Cold start, phone, median of 3 | 1 502 and 1 553 ms | 1 507 to 1 585 ms (host noise) |

- **The fixes cost nothing measurable.** Decisions 289 to 297, measured
  against round 2 as integrated, were level pass for pass on both
  emulators. Decisions 299 to 302 were checked by their device tests
  only, as the owner asked.
- **The bubble.** Idle draws 0 frames in 5 s on every build. Waiting
  draws a frame a vsync (302 in 5 s, p99 16 to 24 ms), at the same cost
  as the pulse it replaced. Paid and the dwell draw 135 to 143 frames
  in 6.5 s, p50 5 to 10 ms. The process's first paid moment has one 200
  to 300 ms frame on SwiftShader, and later ones are 17 to 32 ms.
- **Memory.** The accepted step (+4.9 to +5.9 MB, decision 268) has not
  grown. Round 2's tracks add nothing at rest: the foundation build
  without them trims 0.8 to 1.2 MB *above* round 2. The extra +5 MB over
  round 1's build is `main`'s own growth since `f274ad7` (Bell, Clover,
  the wake-ups). The later fixes measured +0.1, −0.04, −0.23 (phone) and
  +0.77 MB (tablet, one pass), all inside the noise. The 28 MB
  screen-sized purgeable layer round 1 left in HWUI's cache after a Back
  is gone: 2.2 MB, the baseline's size. After a self-rise and Back, 17 MB
  of purgeable cache remains on both round-2 builds. It is not counted
  in PSS on the emulator and is for the phone (decision 298).
- **Tests.** The full device suite passed 220 of 220 at `63855a4`. At the
  head, `RunningMovesTest` passed 55 of 55 and `TrayTest` 15 of 15.
  `./gradlew build` is green.
- **Against the owner's rules.** Every new animated value is read in a
  layer, draw or layout lambda. Nothing allocates per frame. There is no
  `delay()`, frame loop or reflection. The presets are `Motion`'s only
  (`chase`, `spread` and `tick` joined them). There is one reduced-motion
  switch, colours come only through roles and `Fixed`, no dependency was
  added, and the code is always drawn whole, so it scans on every frame.

### Round 2's open items

None is a crash or a wrong state, and none blocks shipping.

- **Screens, minor: the restored row is judged by indices** (decision
  300). Whether the tapped row is "shown whole" is decided from item
  indices recorded at tap time. If a section above the row changes
  height after the payment (for example, the tray's fold changes), the
  restored window can leave the row slightly lower than expected or cut
  at the bottom edge. *Fix:* no change needed to ship. The code falls
  back to 296's finger place whenever the index leaves the window. If a
  phone run shows a cut row, compare the measured offset in a
  post-layout `SideEffect` and fall back to the finger place.
- **Screens, minor: `WholeNode.carry()` repeats fixed work.** It
  recomputes `Course.holding` on every frame, in both the layer block and
  draw, although the answer is fixed for the whole move. It allocates
  nothing, but the work is repeated. *Fix:* optional. Cache the answer
  per move, keyed on the located course, if a phone profile ever shows
  it.
- **Screens, minor, not a regression: the 4th row is an empty slot for a
  moment after the self-rise Back into the 5th row** (decision 299). The
  4th waiting row, just above where the card lands, shows as an empty
  slot for 1 frame in light and about 3 frames (about 50 ms) in dark,
  until the in-place unfold reaches it. The row under the fold reveals
  top-down while the landing row is already whole, so for a moment the
  4th row looks like a hole (about 1.2 s into
  `after-r2/back-into-folded-row`). The pre-fix clip shows the same
  reveal; the ground used to cover it, and now the carried code does
  until the words begin. The code is never dropped, and no frame shows
  bare ground. *Fix:* no change this round. If the owner notices it on
  his phone, start the unfold reveal of the rows under the fold when the
  code begins to leave the card, not when the ground lands, so the row
  above the landing row is already there when the code goes. Do it
  inside the existing unfold progress, read in draw, with no new
  animation. It is on decision 298's phone-read list.
- **Both, minor: unchanged items that need a real phone.** The first
  paid-bubble frame per process; the step-back's two stalls (the shift's
  first frame, then one render-thread frame, the same on round 1's
  crossfade); the 20-sale tray's first open (one 150 to 210 ms frame);
  `restingOn`'s same-snapshot gap (288); "Show fewer" tapped during a
  landing being ignored until the landing ends; and about 17 MB of
  purgeable HWUI cache after a self-rise (298). *Fix:* the owner reads
  these on his phone using decision 298's flow (`mem-r8c.sh`'s self-rises
  and Backs, then `dumpsys meminfo` and `gfxinfo`). The bubble's warm-up
  (the tick drawn once at alpha 0) is the fix only if the phone shows the
  first paid frame.

### What round 2 closed from round 1

Round 1's open items further down are marked where they stand. Closed:
Back with a sale waiting (major; 266, 282), the paid step-back as a
crossfade (major; 266, 285), the memory step (major; accepted by the
owner, decision 268, and not made worse), the code missing on a Back and
a close for now (260 a; 289, 296, 299), the tick over the fading code
(260 b; 286, 290), the second Back leaving the app (260 c; 283),
`TillStage`'s two collectors (284), the `aSaleRisesDark` flake (guarded,
with no recurrence in three full suites), the last `ModifierParameter`
warnings (the overlay track's rename; 288), and the screen-sized HWUI
layer after a row's Back (gone; the self-rise case is 298's). Still
open: the owner's phone, the phone on its side, and the panel's order
line by name.

Everything below this point is round 1's.

## Round 1: what it did

In round 1 the design did not change: the panel with its grabber, the full-screen
sale with the tinted X, the close sheet, the enlarged white code, the
icon-only chip with its phase ring, the theme's colour roles are all as
they were. What changed is how those states reach each other. Nothing
moves for its own sake; every move answers "where did that come from?"
or "where did it go?", and a cashier who never notices any of it is the
intended result.

## Round 1: the clips

Every "after" clip under [`motion/after/`](motion/after/) was recorded
today on this build, on the Pixel 9 Pro emulator (`emulator-5558`,
1280 × 2856, API 36) against the local Monokulo and the stand-in Square,
with real sales rung up for each one. Every "before" clip under
[`motion/before/`](motion/before/) is cut from the round's recordings
of `f274ad7` on the same emulator. Both are 480 px wide, each under
300 KB.

Read them for *what* happens between two states, not for how smooth it
is: the emulator renders through SwiftShader on the host's CPU and its
screen recorder keeps a frame only when the picture changes, so a
450 ms move shows as five to twelve frames here and as thirty to fifty
on a phone. The frame timings are in the performance section; the
feel of the moves on a real screen is still the owner's phone to
judge (open items).

To rerun these clips' scripts on round 2's build: `uiautomator dump`,
which the screen-reading steps use, times out while the bubble's border
chases (a code waiting keeps the window from going idle), so pay or
expire the waiting sale before a step that reads the screen, or tap by
coordinates (`motion/baseline.md`, emulator caveats; decision 294).

| | Before | After |
| --- | --- | --- |
| The paid moment in the panel | [panel-paid-next-sale](motion/before/panel-paid-next-sale.mp4) | [panel-paid](motion/after/panel-paid.mp4), [panel-paid-next-sale](motion/after/panel-paid-next-sale.mp4) |
| Paid on the sale screen | [paid-on-sale](motion/before/paid-on-sale.mp4) | [paid-on-sale](motion/after/paid-on-sale.mp4), [paid-on-code](motion/after/paid-on-code.mp4) |
| The code enlarging | [code-enlarge](motion/before/code-enlarge.mp4) | [code-enlarge](motion/after/code-enlarge.mp4), [code-enlarge-dark](motion/after/code-enlarge-dark.mp4) |
| The panel and the bubble | [panel-raise-lower](motion/before/panel-raise-lower.mp4), [bubble-throw](motion/before/bubble-throw.mp4), [panel-order](motion/before/panel-order.mp4) | [panel-raise-lower](motion/after/panel-raise-lower.mp4), [bubble-throw](motion/after/bubble-throw.mp4), [panel-order](motion/after/panel-order.mp4) |
| A sale and its row | [row-to-sale](motion/before/row-to-sale.mp4), [sale-arrives](motion/before/sale-arrives.mp4) | [row-to-sale](motion/after/row-to-sale.mp4), [sale-arrives](motion/after/sale-arrives.mp4), [close-for-now](motion/after/close-for-now.mp4), [expired](motion/after/expired.mp4) |
| The shift's rows | | [shift-chips](motion/after/shift-chips.mp4) |
| Animations off | | [reduced-motion](motion/after/reduced-motion.mp4) |

The longer round-1 to round-4 recordings in `motion/after/` (`phone-*`,
`r3-*`, `r4-*`) and the seeked frames in [`motion/frames/`](motion/frames/)
and [`motion/review-r6/`](motion/review-r6/) are the evidence the reviews
rested on; they stay for the record.

## Round 1: the improvements, in the order a cashier feels them

### 1. The paid moment is one moment

**Before.** The payment arrived as a different page. In the panel the
code was replaced by a tick in one frame and the docked bubble went
green; four seconds later the bubble went back to orange, a second
before the panel it was docked to moved on or vanished. On the sale
screen the live sale was swapped for the Paid page in one frame, and
five seconds later the page was swapped for the shift. A cashier saw it
if they happened to be looking.

**After.** The payment lands on the thing the cashier is looking at and
closes it. In the bubble the ring's arc sweeps round to a full green
circle over about a quarter of a second and the pupil becomes the tick
(`after/panel-paid`, the docked bubble at the top right). In the panel
the white code card shrinks into the 88 dp paid circle as one surface,
the code fading out inside it and the tick fading in; the status words
fade through to "Paid"; the two buttons settle into one. On the sale
screen the code card folds into the page's hero the same way
(`after/paid-on-sale`); on the enlarged code the whole white screen
closes into the hero (`after/paid-on-code`), so a payment that lands
while the customer's phone is still up never cuts the code away under
them. One `CONFIRM` haptic, from whichever surface is in front, and no
other buzz anywhere in the app.

The moment then ends once, everywhere, after the same five seconds. With
nothing waiting, the panel folds into the green bubble and the bubble
fades to idle only once that collapse has landed (`after/panel-paid`,
9 s: the grey idle bubble is the last frame). With another sale waiting,
the next code grows back out of the tick in the same panel, its header
and amount following the code, never ahead of it
(`after/panel-paid-next-sale`, 7 s). The chip's tick leaves at the same
dwell. The sale screen steps back to the shift at the same dwell.

**Interrupted.** Done on the panel or Back on the page ends the dwell
early and runs the same exit. A payment that lands mid-enlarge or
mid-shrink takes the move over from the drawn size: the white retargets
into the hero from wherever it is. A tap on the panel during its collapse
raises it again from where it is.

**Reduced motion.** The dwell is unchanged (it is reading time); the fold
and the collapse become 120 ms fades; the ring's arc snaps to full.

**Still open.** The sale screen's step back after the dwell is a
crossfade, not the page settling into its row (decision 259; the Back
from the same page does settle, `after/row-to-sale` at 5 s). In the sale
screen's fold the tick is drawn over the fading code for three frames
(260 b). Both are the screens track's, below.

### 2. The code enlarges from the code

**Before.** The harshest cut in the app: a tap on the code replaced a
dark screen with a white one in one frame, brightness and bar icons
flipping with it, and the shrink did the same backwards
(`before/code-enlarge`, in the dark theme). The code was encoded and
laid out again on each side.

**After.** The white card is the thing that grows: its white spreads from
the card's bounds to the screen and the code scales inside it as one
layer, the sale's amount, pill and buttons fading out under it and the
enlarged screen's words fading in once the white has passed them
(`after/code-enlarge` at 7 s, `after/code-enlarge-dark`). The bar icons
turn dark at the move's half; the brightness override is set once the
white has landed and given back as the shrink starts, so the system's
own ramp does the rest and nothing writes window attributes per frame.
On every frame the code is whole, square, inside its white with the
rest margin (20 to 26 px measured on every recorded frame against 22 to
24 at rest), and never smaller than its 264 dp rest size: a camera that
catches any frame of the move catches a valid code. In split screen the
white fills the app's half with the code whole.

**Interrupted.** A second tap at any point reverses from the drawn size;
taps 80 ms apart and an interruption at 150 ms were measured
(`after/code-enlarge` at 14 s: the white never reaches the top before it
is on its way back). The back gesture scrubs the shrink with the finger
and springs home when let go short of the rule (`after/code-enlarge`
at 19 s); a committed gesture carries on from where the finger left it.
A payment during the enlarge is case 1's fold.

**Reduced motion.** A 120 ms crossfade, white to white, with the code
whole on both sides; the gesture shows no preview and Back still works.

### 3. The panel is the bubble opened

**Before.** A tap on the bubble put the panel on screen in one frame,
with a second bubble appearing docked at its top corner while the free
one vanished; lowering it was the same backwards. A fling down the
grabber ended in the panel vanishing. The bubble, thrown, slid to the
edge on a timer with no memory of how hard it was thrown
(`before/panel-raise-lower`, `before/bubble-throw`).

**After.** The card scales up out of the bubble's circle wherever the
bubble is: from a bubble thrown to the bottom left, the card opens
around it and rides with it to the dock, so the panel is visibly the
bubble, opened (`after/panel-raise-lower`, 1 s). Lowering it folds the
card back into the docked bubble, which then flies home to the edge
(4 s). There is one bubble on screen on every frame of the handoff
between the two windows. A short drag on the grabber springs back
(10 s); a fling past the rule keeps going in the finger's direction for
the first frames after release, then the collapse takes the card (12 s),
so a thrown panel reads as thrown. The free bubble, let go, springs to
the near edge from the finger's own speed (`after/bubble-throw`). The
panel's order line opens into the order with the card growing to the
window and the order's words already on it, and the order shrinks back
into the line (`after/panel-order`). On the tablet the floating panel
grows from its corner nearest the bubble and shrinks back to it.

**Interrupted.** A tap on the bubble, the grabber or beside the card
mid-move retargets the one spring that runs it, from its current value
and speed. A tap on the card's place while it is still growing lowers it
and opens nothing. The docked bubble tapped mid-collapse raises the
card again from where it is, and a flung card raised again comes back to
its place. A touch on the bubble during its snap takes the bubble. A
configuration change that swaps the panel's kind (fold, split screen)
snaps: that is the system moving things, not the cashier.

**Reduced motion.** The panel snaps open and closed; the bubble jumps to
the edge; the window handoff keeps its frames so there is still one
bubble at every moment.

### 4. A sale is its row, opened

**Before.** Every screen change in the app was a cut: a row tapped was
replaced by the sale, Back replaced it with the shift, a sale coming up
by itself replaced whatever was there (`before/row-to-sale`,
`before/sale-arrives`).

**After.** A row's card grows into the sale's screen as one surface and
Back shrinks it back into its row (`after/row-to-sale`). The surfaces
are empty grounds while they move and each screen's words fade through
at their own size, so no text is drawn scaled and two screens' words are
never readable together. A sale that comes up by itself has no row yet,
so it rises a little as it fades in over the shift, which is also how
the cashier can tell they did not open it (`after/sale-arrives`). Back
from a risen sale shrinks it into the row that now exists. "Close it for
now" keeps the code on its card while the sale's words fade, and the
row's ground then takes the screen and shrinks to the row
(`after/close-for-now`); a sale that expires on screen folds its card
into the clock hero, and Back carries that glyph into the row's badge
(`after/expired`). The predictive back gesture scrubs the shrink with
the finger (`after/row-to-sale`, 12 s). The order card grows into the
order page and back.

**Interrupted.** Back pressed mid-grow turns the grow round; a sale
arriving under a Back takes the move over from where it is. Each is a
device test on the test clock
(`RunningMovesTest`, `InterruptTest`: three moves by five interruptions
at a quarter, a half and three quarters).

**Reduced motion.** Every pair is a 120 ms crossfade; the gesture shows
no preview.

**Still open.** Back from a sale while another sale is waiting unseen
never shows the shift: the next sale's code replaces this one's in a
frame (decision 258, the one real regression against the baseline's
cut). A Back into the row begins with one or two code-less frames
(260 a). A second Back pressed while the shift is still arriving goes to
the launcher, as it did before (260 c). All three are the screens
track's, below.

### 5. The shift moves rows, words crossfade, numbers hold still

Rows arriving, leaving or re-sorting under a filter chip move to their
new places and fade in or out instead of the whole list jumping by a row
(`after/shift-chips`); a row's badge crossfades between the ring, the
tick and the clock. The status words ("Waiting for payment", "Seen on
the network", "Confirming") crossfade and the pill settles to the new
width; the phase ring's arc sweeps each quarter instead of jumping,
wherever it is drawn (bubble, panel, pill, code foot, row badge, close
sheet), and a second step mid-sweep just retargets. Every countdown is
set in tabular figures so the digits tick without the text shifting.
None of the clips shows the Seen and Confirming sweeps: the stand-in
Square only has "paid" and "expired", so the one sweep on camera is
the arc closing green at the payment.

### 6. Animations off

The animator duration scale is read in one place and every preset honours
it: loops stop, every spatial move becomes a snap, every crossfade a
120 ms fade (a hard cut between a white screen and a dark one is harsher
than a short fade for the people who ask for this). `after/reduced-motion`
is cases 1, 2 and 4 with the scale at 0; the dwell is the same five
seconds.

### 7. What a cashier feels as battery and heat

The things the round took away are felt as much as the things it added.
A panel up and still now draws five frames in five seconds (the
countdown's tick) where it redrew itself forty to sixty times a second
for as long as it was up. The lit bubble's pulse runs only while a code
waits for its money and the panel is down, never under the panel, never
on the tablet beside it. One clock ticks per screen instead of one timer
per countdown text. A code is encoded once per sale and shared by the
sale, the enlarged screen and the panel. The windows are added once per
open and removed after their exit lands, so an open no longer composes
the panel from nothing.

## Round 1: performance

Before is `f274ad7`'s debug build, after is this branch, measured by the
tech lead before and after *alternately in one session on one emulator*
so the pair sits under the same host load, three pairs per device, with
the devs' own pairs on their emulators beside them. The full tables are
[`motion/after.md`](motion/after.md), round 6; the baseline's are
[`motion/baseline.md`](motion/baseline.md).

**Read the caveat first.** Both emulators render through SwiftShader on
the host's CPU, shared with three other emulators and the builds. Their
frame times are three to five times a Pixel's and move by a third
between runs, so only a pair from the same session says anything, and
it says whether something is *wrong*, not how it *feels*. The screens
track's round-2 run on the same AVD with the host's GPU
(`after.md`, "Round 2, screens track") is the one set of numbers close
to a device: there every grow's median frame was the display's 16.7 ms
and no frame of any flow was over twice the baseline's p95. The owner's
Pixel 9 Pro was not measured (open items).

### Frames, phone emulator, the quiet pair (`after.md` round 6, pair 3)

| Flow | Before | After | Reading |
| --- | --- | --- | --- |
| Shift scroll, 12 swipes: window p50 / p90 / p99 / max | 9.7 / 16.8 / 20.1 / 33.4 ms | 11.7 / 19.0 / 31.0 / 33.5 ms | level; `animateItem` and the per-row transitions cost nothing a scroll can see |
| Code open and close ×3: frames, janky, p50 / p90 / p99 | 20 (cuts), 45 %, 16 / 65 / 69 ms | 131, 10.7 %, 19 / 30 / 81 ms | the grow draws 110 frames the cut did not, under the cut's p90 |
| First code enlarge after launch | 64 ms (layout 46 to 50) | 55 to 59 ms (animation 12, layout 31 to 36, then one frame waiting on the swap) | under the baseline's own cut for the first time; what is left is the emulator's swap |
| Shift to sale and back ×10: frames, janky, p50 / p90 / p99 | 61 (cuts), 47.5 %, 15 / 65 / 81 ms | 426, 15.3 %, 12 / 27 / 89 ms | seven times the frames at a third of the janky share; the p99 is the composing frame each way |
| Order open and Back ×5: janky, p50 / p99 | 0.54 %, 5 / 16 ms | 0.84 %, 5 / 16 ms | level |
| Panel up and still, 5 s: frames | 304 | 5 | the baseline redrew the still panel at 60 fps |
| Panel raise and lower ×5: frames, janky, p50 / p95 / p99 | 1493, 1.1 %, 12 / 17 / 28 ms | 853, 2.9 %, 16 / 16 / 42 ms | level to the p95; the p99 is the collapse's last frames under the free bubble's window being added |
| Bubble drag and snap ×2: janky, p90 / p99 | 0 %, 16 / 16 ms | 0 %, 16 / 16 ms | level; the spring costs what the timer did |
| Lit bubble alone, 5 s: frames | 301 | 302 | the pulse's loop, by design, on both |

Under the two loaded pairs every after is level with or better than its
before. The tablet reads the same: shift to sale and back better than the
cuts at every percentile, the first enlarge the emulator's swap alone
(layout 0), the still panel once a second, the raise and lower level.

### Memory

PSS in kB after the flows, ten seconds at rest and a
`RUNNING_CRITICAL` trim, the phone emulator, two clean pairs:

| | Before | After | Step |
| --- | --- | --- | --- |
| Pair 1 (Java / native heap) | 105 070 (17 092 / 14 840) | 111 010 (18 252 / 17 264) | +5.9 MB (Java +1.2, native +2.4) |
| Pair 2 | 103 073 (16 060 / 14 096) | 107 945 (17 028 / 15 588) | +4.9 MB (Java +1.0, native +1.5) |
| Release build, Welcome, not connected (baseline) | 23 916 | | the floor the debug figures sit 70 MB above |

The rule was within 2 MB of before; decision 202 accepted 2 to 3 MB at
start-up. The step is 5 to 6 MB, and the screens track's heap dumps
name it (decision 253): it is the shift's standing motion machinery,
there from launch (each row's badge transition and shared surface, the
rows' placement animation, the stage's transitions and the lookahead
pass's per-node delegates, about 27 000 more objects), not a session's
growth: a session adds half a megabyte more than the baseline's does.
Round 2's real leak, text rasterised at every scale a surface passed
through (+10 to +19 MB), is gone since round 3. The other thing in the
tables is HWUI's GPU cache holding one or two screen-sized purgeable
textures (14 to 28 MB) for about fifteen seconds after a screen change:
a cache entry Skia evicts on the next big draw or at idle, on both
builds' terms a non-standing cost, but on a device it is Graphics PSS
for those seconds. The emulator cannot price either on a phone; both
are open items.

### Start-up, size, tests

Cold start of the debug build is within the host's noise on every quiet
pair (+1.6 % on the phone). The release APK is 8 085 234 B, +133 KB over
the baseline (download +93 KB), over the 50 KB rule since round 1 with
no dependency added: it is the new code and `SharedTransitionLayout`
and `PredictiveBackHandler` being reached for the first time, so R8
keeps them. `./gradlew build` (compile, lint, unit tests) is green at
the head; the device suites (`RunningMovesTest` 29, `OverlayFramesTest`
16, `MotionFramesTest` 19, `StageBackTest` 8, `StageTest` 4 on 5558;
45 overlay tests on 5554; 161 on 5556) are green at the heads
integrated, with one load-time flake noted below.

### The code, against the owner's rules

Every animated value in the new code is read in a `graphicsLayer {}`,
`drawBehind {}`, `offset {}` or `layout {}` lambda, a `Canvas` draw or a
`Modifier.Node`; the stage's grounds and codes are nodes that read one
progress per frame, keep their geometry in plain floats and draw the
code's bitmap whole with no filtering. Nothing is allocated per frame
(paths are rewound, offsets are value classes, the one per-frame
placement is a `Long` stamp). There is no `delay`-chained choreography
(the three `delay(Dwell.PAID_MS)` are the one dwell in its three owners),
no `Handler.postDelayed`, no `ValueAnimator`, no reflection, no frame
loop of the app's own. Every spring and tween is a named preset in
`ui/Motion.kt`; no call site holds a number. Reduced motion is read once
in `rememberMotion()`. No dependency was added; compose-bom 2026.09.00
already had everything at minSdk 26.

## Round 1: what was deliberately not animated, and why

- **The QR modules, a fade-in of the code, a redraw as it scales, a
  "scan me" shimmer.** The code is a bitmap scaled as one layer and
  nothing else, ever; at rest it is never below its designed size.
- **The brightness, by hand.** A `window.attributes` write is a relayout
  request; per frame it is exactly the code the owner ruled out. It is
  set once when the white lands and the system ramps it.
- **The countdown.** A number is read, not watched; it ticks once a
  second in tabular figures and nothing else moves.
- **The status bar chip.** Android draws it from the four vector
  drawables; animating it would mean posting a notification per frame.
- **The close sheet.** Material's own spring was kept; it is already
  right and it is the platform's.
- **The panel's kind changing on a fold, rotation or split screen.** A
  snap: the system moved things, not the cashier.
- **Settings.** A different place, so it fades through rather than
  growing out of anything.
- **Chip selection, the "Needs action" count, the Watching dot, amounts,
  the order's lines, onboarding's features.** Discrete states and
  numbers a cashier reads once; a fade or a stagger or a count-up would
  only make them look slower.
- **Button presses, ripples, a shaking refund button, a pulsing "Paid",
  confetti.** Material's ripples are enough; a louder paid moment would
  be noticed, which is the opposite of the goal.
- **The idle shift's faded logo.** A placeholder, not a state the
  cashier waits on.
- **Window animations (`windowAnimations`), `animateContentSize` on the
  panel card, `makeScaleUpAnimation` from the panel, a true origin at
  the tablet's bubble, `dynamicanimation` or Material motion.** Each
  considered and cut in `docs/motion.md` section 5: not interruptible,
  not seeded by velocity, re-measures the code per frame, ignored by
  Android 12+, clipped by the window, or a dependency.

## Round 1: open items

Everything round 1 left, by track and severity, with the fix each
one pointed at. Round 2 closed most of them; each is marked
**Closed in round 2** or **Still open** with where it went. Nothing here is a crash or a wrong state; they are the
places where a move is still a cut, a rule is not yet met, or a number
does not exist because only the owner's phone can give it.

### Major

- **Closed in round 2** (266, 282: Back always lands on the shift; the
  waiting sale goes in the tray). **Screens: Back with a sale waiting
  never shows the shift** (decision 258). The stage runs `RowGrow Sale → Till` for three frames at p = 0,
  then the model's `follow` goes to the waiting sale and a `Rise` takes
  the move over from 0: the first sale's words fade, its code is replaced
  by the next sale's in one frame, the shift is never drawn
  (`review-r6/phone-back-with-sale-waiting.webp`, `trace-back.log`). Two
  Backs 250 ms apart chain through two sales the same way. The baseline
  at least cut to the shift first. *Fix:* a leave lands before a follow
  starts: run `AppViewModel.follow` on the leaving move's completion
  (the stage reports landing to the model, as the panel's collapse
  reports to the bubble) instead of on the sales update; keep 249's
  takeover for everything else; a `RunningMovesTest` case that holds the
  `RowGrow` to p = 1 with the shift drawn before any `Rise` begins.
- **Closed in round 2** (266, 285: the step-back is the Back, into the
  row at the top of Earlier). **Screens: the paid page's step back after
  the dwell is a crossfade** (decision 259). The trace shows `RowGrow Sale → Till` with no
  `sale/<id>` course, so the words fade and the shift fades in with no
  surface moving (`review-r6/phone-paid-stepback-crossfade.webp`); a Back
  on the same page a moment later leaves with the course and the ground
  shrinks into the row. *Fix:* give the dwell's step back the same ends
  as a Back from the paid page (the leaving page's `sharedGround` and
  the glyph between hero and badge) on the `cameUpByItself` path, not
  only on `model.back`; test that the step back's trace carries the
  course and the hero glyph lands in the row's badge.
- **Closed in round 2** (268: the owner accepted the step; round 2 does
  not grow it). **Screens: memory after a trim is +4.9 to +5.9 MB over
  the baseline on the phone emulator** (105 070 → 111 010 and 103 073 → 107 945 kB; Java
  +1.0 to +1.2, native +1.5 to +2.4) where decision 202 accepted +2 MB.
  Decision 253 names it (the shift's standing motion machinery) and
  accepts it; the rule is not met and only the owner's phone can price
  it. *Fix:* either the owner accepts the standing cost (amend 202 and
  253 with the phone's number once measured) or the screens track takes
  the shared-surface and transition machinery off rows at rest (compose
  it on a change's first frame, or share one transition across the
  list) and re-measures with `mem-r6.sh`.
- **Still open** (decision 298's phone-read list). **Both: the owner's
  Pixel 9 Pro was not used.** The authorisation
  reached every agent only inside the workflow's computed brief, which
  by their operating rules is not the user's consent, and the step
  installs builds over the owner's onboarded data on a personal device
  (decisions 219, 238, 239, 251, 256, 257). So the phone A/B, the first
  enlarge's frame on a real GPU, the Graphics PSS reading and the 2 MB
  rule on a device are still open, and the brief's deciding numbers do
  not exist. *Fix:* the owner says so in their own session, or runs
  `flock /home/henry/.claude/jobs/a4700d66/tmp/phone.lock bash
  /home/henry/.claude/jobs/a4700d66/tmp/lead/ab-r6.sh
  adb-52161FDAP000RD-2T8G7d._adb-tls-connect._tcp 2` themselves (the
  devs' `heap.sh` and `ab.sh` take a serial the same way), then restores
  `tmp/phone-backup/base.apk` with `adb install -r`.

### Minor

- **Closed in round 2** (289, 296, 299). **Screens: the code is not
  drawn on the frames where the row's ground holds the whole screen** on a Back from a live sale and on a close for
  now (260 a): one or two bare-ground frames on the emulator, three or
  four at 120 Hz (`review-r6/phone-back-into-row.webp`). *Fix:* keep the
  code's end alive through the leaving screen's frames and draw the code
  whole while the card fits inside the ground's drawn bounds; test that
  every `RowGrow Sale → Till` frame whose ground contains the card's
  rest bounds draws the code.
- **Closed in round 2** (286, 290). **Screens: in the sale screen's
  paid fold the tick is drawn over the fading code** while the card is still four fifths of its size, three
  frames of two glyphs in one card (260 b;
  `review-r6/phone-paid-fold-on-sale.webp`). *Fix:* fade the tick in over
  the last `crossfadeOnTrack` share of the fold, the code out over the
  first `fadeOutOnTrack`, as the code screen's fold already does; test
  that no frame draws both above a threshold alpha.
- **Closed in round 2** (283). **Both: a second Back while the shift
  is still arriving goes to the launcher** (260 c; the baseline does the same, so not a regression).
  A cashier who pressed Back twice meant the shift, and leaving the till
  is the one thing a stray press must not do. *Fix:* `StageBack` stays
  enabled while `screen == Till && stage.currentState !=
  stage.targetState` (`model.back(Till)` is already a no-op); a
  `StageBackTest` case with two Backs 250 ms apart from a sale ending on
  the shift with the activity never finishing.
- **Closed in round 2** (284: one collector). **Screens: `TillStage`
  runs two `snapshotFlow`s on the same predicate** (`currentState != targetState` for the frame-stamp loop,
  `==` to forget the held move): two collectors where one would do.
  *Fix:* one `LaunchedEffect(transition, geometry)`: on `false` call
  `held.during(current, current)`; on `true` run the `withFrameNanos`
  loop with the `finally` resetting `geometry.frame` to 0.
- **Closed in round 2** (the `HeldMove` guard with its slow-frame
  tests; no recurrence in round 2's full suites).
  **Screens: `RunningMovesTest.aSaleRisesDark` failed once** on 5558
  under host load 13 to 16 (from frame 11 both screens read the move as
  `RowGrow` where `Rise` was asked, then the stage did not land before
  `SETTLE_FRAMES`); run alone twice it passes in 10 s. The symptom is a
  new `HeldMove` generation starting while the first still ran, which a
  100 ms frame under the test clock could do. *Fix:* read `aSaleRises*`
  under a forced slow frame (a `mainClock` advance of 100+ ms mid-rise)
  and check `HeldMove` cannot start a new generation while
  `currentState != targetState`; guard it if it can, with the slow-frame
  test as the regression.
- **Closed in round 2 for a row's Back** (2.2 MB after the trim, the
  baseline's size); **still open after a self-rise** (17 MB purgeable,
  decision 298, for the phone). **Screens: HWUI's GPU cache holds one
  or two screen-sized purgeable textures after a screen change** (16.3 to 30.4 MB against the
  baseline's 2.2; 14.2 MB is one 1280 × 2856 RGBA screen). It is there
  right after launch, survives a round trip, is evicted by the code
  enlarge and by HWUI's idle purge about 15 s after the last draw on
  both builds: a cache entry, not a standing cost; on a device it is
  Graphics PSS for those seconds. Cause: a whole screen drawn through an
  offscreen layer when a change gives it an alpha. *Fix:* if the phone's
  Graphics PSS shows it mattering, find the whole-screen fade (the
  rising sale page's or the shift's), fade per part or clip the layer to
  the moving surface, or release the layer when the change lands;
  otherwise leave it, noted here.
- **Closed in round 2** (the overlay track's rename; 288 for the last
  one). **Overlay: the remaining `ModifierParameter` lint warnings** are in
  `overlay/Faces.kt` (`PanelCard` and friends take a `wordsLayer`
  modifier not as the first optional `modifier` parameter); idiom only.
  *Fix:* name the parameter for what it is as `SaleDetail` did (`words`)
  with a `@Suppress("ModifierParameter")` and a one-line reason, or
  reorder to the convention.
- **Still open.** **Both: not reached this round, so not judged.** The phone on its side
  (the AVD stayed in portrait under `settings put system user_rotation
  1`; the rig, not the app), the panel's order line by name (overlay
  nodes are absent from the accessibility dump; the fallback tap opened
  it, `review-r6/phone-panel-order-page.webp`), the grabber fling's
  carry-on frames (dev1's host-GPU recording,
  `review-r5/gpu-grabber-fling-carries.webp`, stands), and the owner's
  phone (above). *Fix:* rotate the AVD with `adb shell wm user-rotation
  lock 1` before the landscape walk; give the panel's order line a
  `testTag` or content description so the overlay flows can be driven by
  name; the phone measurements wait for the owner's own word in their
  own session.

### Carried from earlier rounds, for completeness

The stash `r4-flat` for a person to drop; the rebase of `motion` onto
`origin/main` past #16 to #22 and the measuring rigs under
`tools/motion-rig` (the lead's); the `secondsLeft` deletion in the
overlay's `StatusRow` (the one caller left of the old per-text timer);
`fadeThrough`'s 120 ms wait under animations off (a ring or a label is
blank for 120 ms between faces; acceptable, to be seen once on a
device); the paid fold's and the first enlarge's acceptance on a real
GPU; the APK's 133 KB over the 50 KB rule. None of these changes what a
cashier sees today.
