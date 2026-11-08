# Motion: the Android till

How the till moves, and why. Written by the lead designer for the motion
round (branch `motion`, base `f274ad7`), after reading every screen in
`android/app/src/main/kotlin/com/monokulo/till/` and running the build on a
Pixel 9 Pro and a Pixel Tablet emulator: onboarding, the shift, a live
sale, the enlarged code, the close sheet, the order page, paid, ended, the
bubble and panel over another app, the tablet's floating panel, a phone on
its side and split screen. `docs/reports/motion/before/` holds clips of the
flows as they are today; the showcase puts the "after" beside them.

The design itself does not change (`docs/decisions/android.md` 60 to 89,
the till tweaks canvas): the panel with its grabber, the full-screen sale
with the tinted X, the close sheet, the enlarged white code, the icon-only
chip with its phase ring, the theme's colour roles. Motion refines how
those states reach each other.

Contents: [principles](#1-principles) · [presets](#2-the-motion-presets) ·
[spatial model](#3-the-spatial-model) · [opportunities](#4-opportunities)
· [rejected](#5-rejected) · [haptics](#6-haptics) · [reduced
motion](#7-reduced-motion) · [measuring](#8-measuring) · [order of
work](#9-order-of-work-and-tracks) · [performance rules](#10-performance-rules)
· [tech lead amendments](#11-tech-lead-amendments) · [round 2: the
waiting tray, Back and the logo bubble](#12-round-2-the-waiting-tray-back-and-the-logo-bubble)
· [round 2 build](#13-round-2-build).

Round 2 (2026-10-30, §12) supersedes parts of §4, §8 and §10; each is
marked where it stands, with the decision that replaced it.

---

## 1. Principles

Each is tied to one of the owner's goals (no motion for its own sake;
blazing fast and memory efficient; the code always scans; the design stays).

1. **Motion explains where things come from and go to, and nothing else.**
   A panel grows out of the bubble because it *is* the bubble opened; the
   enlarged code grows out of the code because it is the same code, bigger.
   If a movement doesn't answer "where did that come from?" or "where did
   it go?", it isn't made. Decoration (staggered lists, count-ups, shimmer,
   bouncing buttons) is out.
2. **The cashier never waits for it.** Every transition is interruptible
   at any frame and never blocks input: a second tap reverses a panel
   mid-grow, a fling carries its velocity into the collapse, Back during
   the enlarge shrinks from wherever it got to. Nothing is sequenced with
   delays; a motion that would need "wait, then" is two motions, or one
   with the second part driven by the first's progress.
3. **The code is sacred.** The QR bitmap is laid out once per sale and
   never re-laid out, re-encoded or redrawn module by module during a
   transition. It only ever moves or scales as one layer, so any frame a
   camera catches is a valid code at a valid size. At rest it is never
   smaller than designed (264 dp on the sale, 264/280 dp on the panel).
4. **Cheap by construction.** Animated values are read in the draw or
   layout phase (`graphicsLayer {}`, `drawBehind {}`, `offset {}`,
   `Modifier.Node`), never in composition per frame; nothing is allocated
   per frame; there is exactly one frame clock (Compose's) and two loops
   in the whole app (the bubble's pulse and the loading mark's glints),
   both only while something is being waited for. No `Handler.postDelayed`
   choreography, no `ValueAnimator`, no reflection, no hand-rolled loops.
5. **One system.** Every spring and duration is a named preset in
   `ui/Motion.kt`, picked by purpose, so two things of the same kind move
   the same way and a timing is changed in one place. No numbers at call
   sites.
6. **Reduced motion is one switch.** The animator duration scale is read
   in one place; loops stop, spatial moves become short fades or snaps.
7. **Haptics confirm, never decorate.** One `CONFIRM` when a payment
   lands, on whichever surface the cashier is looking at. Nothing on taps,
   drags, snaps, phase steps or errors.
8. **Hierarchy of timing.** Big surfaces take longest (a window, a
   screen); small things settle fast (a ring arc, a row); a fade is
   quicker than a move; exits are quicker than entries. A cashier reads
   the hierarchy without seeing it: what moved slowly was big.

## 2. The motion presets

`ui/Motion.kt`, beside `ui/Theme.kt`. Springs for anything that moves
through space or can be interrupted (they retarget from the current value
and velocity, which tweens can't); durations only for opacity, where there
is no position to carry. `Spring.StiffnessMedium` is 1500 and
`StiffnessMediumLow` 400 in Compose; these sit around them.

| Preset | Spec | Settles in about | Used for |
| --- | --- | --- | --- |
| `Motion.surface` | `spring(stiffness = 380f, dampingRatio = 0.86f)` | 450 ms | A window-sized thing arriving or leaving: the panel growing out of the bubble and collapsing back; the enlarged code growing from the sale's code; a screen's container transform (shift row to sale, order card to order page); the sale's code card turning into its paid or ended hero |
| `Motion.settle` | `spring(stiffness = 800f, dampingRatio = 1f)` | 250 ms | Small things finding their place with no bounce: a shift row moving when another arrives or leaves; the status pill resizing when its words change; the panel's buttons changing width; the docked bubble sliding to its slot |
| `Motion.flick` | `spring(stiffness = 600f, dampingRatio = 0.8f)` seeded with the release velocity | 300 ms | What the finger let go of: the bubble snapping to its edge; the grabber springing back; the close sheet settling (Material's own spring is kept) |
| `Motion.thrown` | `exponentialDecay()` (the platform's plain friction) seeded with the release velocity; null under Still | runs until the card it moves has landed | What the finger threw, running on while something else brings it home: the grabber's fling past the rule (decision 247) |
| `Motion.sweep` | `spring(stiffness = 170f, dampingRatio = 1f)` | 600 ms | The phase ring's arc growing a quarter: slow enough to be a sweep, overdamped so it never overshoots a quarter mark, and a second step arriving mid-sweep just retargets |
| `Motion.track` | `tween(450, LinearEasing)` | (a line, not a time) | The straight line every spatial part of a screen change follows (shared surfaces' bounds, rises, slides), so the stage at progress p has each part p of the way (decision 120) |
| `Motion.stage` | `Motion.surface.fraction` on the stage's progress, null under Still | 450 ms | What moves the stage's progress to a screen: an uninterrupted change and the release of a back gesture alike, so a seek and a spring share one physics (decision 177) |
| `Motion.fade` | `tween(200, LinearEasing)` | 200 ms | Crossfades between two faces of the same thing: the bubble's content, the status words, a row's badge, the paid hero's icon |
| `Motion.fadeQuick` | `tween(120, LinearEasing)` | 120 ms | Exits under something that is arriving, and every crossfade under reduced motion |
| `Motion.crossfadeOnTrack`, `Motion.fadeOutOnTrack` | `fade` and `fadeQuick` as shares of `track` (0.44, 0.27) | (shares) | On a screen change, a glyph surface's faces crossfade over `crossfadeOnTrack`; a screen's words fade out over the first `fadeOutOnTrack` and in over the last `crossfadeOnTrack`, so two screens' words are never drawn together (decisions 182, 195) |
| `Motion.fadeThrough` | `tween(200, delayMillis = 120, LinearEasing)` (120 ms, still after its 120 ms wait, under Still: decision 141) | 320 ms | An entrance in the place of a different thing leaving under `fadeQuick`: the panel's buttons and words when its face changes ("Copy address" to "Done"), so two labels are never drawn over each other (added in round 2, decision 115) |
| `Motion.pulse` | `tween(1600, LinearOutSlowInEasing)`, repeating | loop | The lit bubble's pulse, only while a code waits and the panel is down |
| `Motion.glints` | `tween(GLINT_MS, LinearEasing)`, repeating | loop | The loading mark (unchanged) |
| `Motion.chase` | `tween(CHASE_MS, LinearEasing)`, repeating (1250 ms) | loop | The waiting bubble's border: an ink segment chasing round the pale track at one speed, 12 o'clock clockwise; null under Still, where the border is whole ink (round 2, §12.5) |
| `Motion.spin` | `tween(SPIN_MS, LinearEasing)`, repeating (1600 ms) | loop | The phase ring's green arc turning while the payment is seen or confirming, read in the ring's draw; paid stops it where it is and the arc closes from there; null under Still, where only the arc's length steps (fixes canvas Board 5) |
| `Motion.spread[ring]` | `fade` with `delayMillis = ring × SPREAD_STEP_MS` (60), four rings | 380 ms | The bubble's lens changing colour from the core out: the core, the inner six, the middle twelve with the base, the rim, each a fade a step after the one inside it; under Still each ring is `fadeQuick` with no delay, so the lens changes in one quick fade (round 2, §12.5) |
| `Motion.tick` | `sweep` held at its start for `fade`'s length (`After`, the spring played from a later start) | 800 ms | The paid bubble's tick trimmed along its path once the core's ring is green: the start is in the spec, never a `delay()`; under Still `snap()`, the tick whole at once (round 2, §12.5) |
| `Motion.untick` | `snap()` held for `fadeQuick`'s length (`After`) | 120 ms | The paid bubble's tick leaving: its trim held while its alpha fades on `fadeQuick`, then rewound to 0 for the next paid; the same under Still (decision 293) |
| `Motion.Dwell.paid` | `5_000 ms` | — | How long a paid moment is held before the till moves on: the sale screen stepping back to the shift (`TillNav.PAID_STEP_BACK_MS`), the panel lowering, the bubble staying green and the chip its tick (`Bubble.PAID_MS`, today 4 s; see M9) |

Rules: `visibilityThreshold` is set for every spring on an `IntOffset`,
`Dp` or `Float` in pixels (`IntOffset.VisibilityThreshold`, 0.5 px), so a
spring doesn't run invisible tails. Any new preset is added to this table
and to `Motion.kt` together, with a purpose, not a value, as its name.
Nothing in a call site reads like `tween(300)`.

Reduced motion (section 7): `Motion.reduced` (a `CompositionLocal` set
once, under the theme) turns `surface`, `settle`, `flick` and `sweep` into
`snap()`, `fade` into `fadeQuick`, and the loops off.

Round 2 adds `Motion.chase` (the bubble's border, a loop) and
`Motion.spread` (the lens filling ring by ring): §12.5.

## 3. The spatial model

What the cashier's mind holds, from the back of the screen to the front.
A transition is right when it keeps this model true.

```text
Square (another app)
  └─ the bubble: lives on an edge; the cashier put it there
       └─ the panel: the bubble, opened. Phone: hangs from the top edge with the
          bubble docked at its top corner; tablet: a card beside the bubble
            └─ the order: the panel's order line, opened to fill the panel
Till (the app), in front of everything
  └─ the shift: the list; every sale is a row
       └─ a sale: its row, opened. Live: the code; otherwise its page
            ├─ the code, enlarged: the code card, grown to the screen
            ├─ the order: the order card, grown to the page
            └─ the close sheet: rises from the bottom over the sale
  └─ Settings: a different place, so it fades through, not out of anything
```

Continuity map, each thing's origin and destination:

| Thing | Comes from | Goes back to |
| --- | --- | --- |
| Phone panel | the docked bubble's circle (grows out of it, the free bubble having slid into the dock) | the docked bubble, which slides back to the edge |
| Tablet panel | its corner nearest the bubble | the same corner |
| The order, in the panel | the panel's order line (the card grows to the window) | the order line |
| A sale screen (cashier opened it) | its row in the shift | its row |
| A sale screen (a new sale came up by itself) | the shift, rising a little (no row exists yet) | its row, which now exists |
| Enlarged code | the sale's code card | the code card |
| Order page (app) | the sale's order card | the order card |
| Paid / ended page | the sale screen in place: the code card becomes the hero circle | its row, after the dwell |
| Close sheet | the bottom edge | the bottom edge |
| Settings | fades through | fades through to where it was opened |
| Onboarding steps | the next step slides in from the end, the last out to the start | reversed |

## 4. Opportunities

Numbered M1 onward; each says where (file and composable), the motion
from and to, what happens when it's interrupted, what it does under
reduced motion, and why a cashier is better off. "Track" names the
developer: dev1 is the overlay, dev2 the app's screens; the tech lead owns
`Motion.kt` and the measurements.

### Overlay (dev1)

**M1. The panel grows out of the bubble, and collapses back into it.**
`overlay/Overlay.kt` (`addPanel`, `removePanel`, `addBubble`,
`removeBubble`), `overlay/Faces.kt` (`PhonePanel`, `FloatingPanel`,
`PanelCard`).
- Today: `wm.addView` / `removeViewImmediate`; the panel pops in at full
  size and vanishes; on a phone the free bubble window vanishes and a
  docked bubble appears in another place in the same frame.
- Phone, raise: the panel window is added with its content at progress 0
  (invisible) on its first frame. The docked bubble starts where the free
  bubble was (the window hands the last `params` position across) and
  springs to its dock slot (`Motion.settle`); the panel card scales and
  fades up out of the docked bubble wherever it is on that frame
  (`graphicsLayer { scaleX; scaleY; alpha; transformOrigin }`, pivot at
  the sliding docked bubble's centre, `Panel.docked`, so from a free
  bubble far from the dock the card is the bubble opening on its way
  there and the lower folds it back into the bubble before it flies home,
  decision 246), scale from
  `Panel.SMALLEST` (0.3), alpha reaching 1 by a third of the way out of the
  bubble, `Motion.surface`). The raise's first tenth (`Panel.IN_BUBBLE`) is
  the card wholly in the bubble: `Panel.opened` measures the way out from
  there, a collapse lands there, and the bubble's last twentieth home has
  no card with it (decision 227). While the card moves, a tap on its place
  lowers it and opens nothing (decision 230). The free
  bubble window is removed on the frame the panel window has drawn, never
  before, so there is no frame with no bubble.
- Phone, lower: the reverse. The free bubble window is added back at the
  edge slot *before* the panel window is removed, and the panel window
  goes on the frame after the free bubble has drawn: the handoff is
  pixel-identical. Flung down the grabber, the drag runs on at the
  finger's speed (`Motion.thrown`, a decay) inside the card's scaling
  layer while the raise collapses, so the first frames after the release
  carry on down before the collapse takes the card (decision 247). The shadow, grabber and header collapse with the card;
  nothing inside the card animates separately.
- Tablet: the floating panel scales and fades up from the corner of the
  card nearest the bubble (pivot clamped to the card's bounds; see the
  rejected list for why not the bubble itself). The free bubble stays.
  When the bubble sits at the end edge at the panel's height, the panel
  covers it (`before-tablet-panel.webp`); the grow from that corner is
  then literally from the bubble, and a tap on the panel's grabber or
  beside it still lowers it, so nothing is lost; QA has the overlap as a
  finding (decision 147).
- Interruption: one `Animatable` `raise` in 0..1 per panel window; a tap on
  the bubble, the grabber or beside the panel mid-animation retargets it
  (`animateTo` from the current value and velocity). A sale ending or the
  app coming to the front while raising collapses from wherever it is. A
  configuration change (fold, split screen, rotation) that swaps the
  panel's kind (decision 89) snaps: that is the system moving things, not
  the cashier.
- Reduced motion: snap to open or closed; the bubble handoff is as above.
- Why: the panel is the bubble opened; today it is an unrelated thing that
  appears. A cashier glancing from the customer to the screen finds the
  panel where their eye already was, at the bubble.

**M2. The grabber's fling carries into the collapse.** `Faces.kt`
(`PanelCard`, `Grabber`), `Panel.lowers`.
- Today: the panel follows the finger (`offset {}` on an `Animatable`),
  springs back short of the rule, and vanishes past it.
- Past the rule: `raise.animateTo(0f, Motion.surface, initialVelocity =
  -velocity / height)` from the current drag offset: the panel keeps
  moving in the finger's direction for the first frames while it shrinks
  toward the dock, so a fling reads as the panel being thrown away. The
  drag offset runs on from the release at the finger's speed under
  `Motion.thrown` (`drag.animateDecay`), inside the card's scaling layer,
  so on screen it is the drag times the card's scale: the first two frames
  after the release carry on down, then the collapse takes the card into
  the bubble (decision 247). Raised again mid-collapse, the drag springs
  back to 0 under `Motion.flick`. Short of the rule: `drag.animateTo(0f, Motion.flick,
  initialVelocity = velocity)`.
- Tablet: the drag fades the card (decision 65); past the rule the fade
  completes with the same collapse spring, seeded the same way.
- Interruption: a new touch on the grabber during the spring-back stops
  it (`draggable` already does, since `snapTo` runs on the same
  `Animatable`).
- Reduced motion: no spring-back, the panel snaps to 0 or closes.
- Why: a fling today ends in a cut; the energy the cashier put in should
  go somewhere.

**M3. The bubble's edge snap is a spring with the finger's velocity.**
`Overlay.kt` (`BubbleFrame.snap`).
- Today: `ValueAnimator.ofInt` with `duration = 120 + distance / 6`, no
  velocity, and `updateViewLayout` per frame.
- A `VelocityTracker` (`android.view`, no dependency) on the drag; on
  release, a Compose `Animatable` on the window's x (in a coroutine on
  `AndroidUiDispatcher.Main`, which has the frame clock) with
  `Motion.flick` seeded with the x velocity; `updateViewLayout` stays the
  way the window moves (it is how every floating bubble on Android moves,
  and it already runs per `MOVE` event), but it is now driven by the
  frame clock, so it never runs a frame the display can't show. The y is
  clamped as today.
- Interruption: a touch down during the snap cancels the job and the
  finger has the bubble.
- Reduced motion: the window jumps to the edge.
- Why: the snap should feel like letting go of something, not a timer.

**M4. The bubble changes face without changing bubble.** `Faces.kt`
(`BubbleFace`, `Circle`).
*Superseded in part by §12.5 (decision 267): the bubble's three faces are
now one logo mark (B5 c), not the ring with its pupil, the loading mark
or a tick on `paidGround`; the bubble draws no `PhaseRing` and no
glints. M4's structure (one circle, alpha and ground in the layer, the
pulse, the appear and disappear) stands.*
- Today: three different `Box`es by `look`; idle to lit to paid are hard
  swaps, and the paid look is a different composable with a different
  shadow.
- One circle that stays; its alpha (0.55 to 1), elevation (4 to 6 dp) and
  ground colour (white to `paidGround`) animate (`Motion.fade`, read in
  `graphicsLayer`/`drawBehind`); its content crossfades (`Crossfade`-style
  with `Motion.fade`): the grey logo, the ring with its pupil, the tick.
  The appear and disappear of the whole bubble (shift start and stop, the
  app going to the front and back) scale and fade the circle inside its
  window (0.6 to 1, `Motion.surface`) rather than the window popping; the
  window is added at progress 0 and removed after the exit finishes.
- The paid moment (with M6): the ring's arc closes to the full circle in
  `paid` green and the pupil crossfades to the tick, ground to
  `paidGround`; after `Motion.Dwell.paid` it crossfades back to lit or
  idle. One `CONFIRM` haptic at the moment it lands (section 6).
- The pulse: runs only while the shown sale is Waiting or Partial and the
  panel is down (it is a call to raise the panel); it stops when the panel
  is up (the docked bubble doesn't pulse) and once the payment is seen.
  Its ring is drawn in `drawBehind` as today. The window's padding is
  raised from 12 to 16 dp so the pulse's outer edge (28 + 16 dp) isn't
  clipped by the window, which it is today by 4 dp.
- Interruption: `Crossfade` retargets; a paid that arrives mid-pulse just
  stops the pulse at its current alpha and fades it.
- Reduced motion: no pulse; faces swap with `fadeQuick`.
- PR #18 note: #18 puts the loading mark's glints back in the lit bubble
  in place of the ring. If it lands first, the lit face is the mark and
  the paid face is mark-to-tick; the pulse and the glints must then share
  one `rememberInfiniteTransition` so the window has one invalidation
  stream, not two. M4's structure (one circle, crossfaded content) is the
  same either way; dev1 keeps `BubbleFace`'s signature and puts the new
  motion in private composables, so the merge is a content swap.
- Why: the bubble is one object the cashier placed; it mustn't look
  replaced.

**M5. The panel's faces, and its order page, come from where they were.**
`Faces.kt` (`Face`, `Buttons`, `PhonePanel`, `FloatingPanel`),
`Overlay.kt` (`PanelPage`).
- Code to order: today `PhonePanel` returns a different tree. Now one
  outer container (shadow, clip, ground) whose bounds animate from the
  card to the window (phone) or from the card's height to the order's
  (tablet) with `Motion.surface` (`AnimatedContent` with a
  `SizeTransform`, or `animateContentSize` on the container), the corner
  radius 32 to 0 dp on a phone read in `graphicsLayer { shape }`, the
  content crossfading (`Motion.fade`, the code out under `fadeQuick`); the
  docked bubble fades out as the order fills the window and back in on
  return. The order line is where the eye lands first after the swap
  (it is at the top of the order page's list in the same place).
- Live to paid, in place: the code card scales down into the 88 dp paid
  hero at the same centre (one layer: the white card with its corner
  radius growing to a circle in `graphicsLayer`, the QR image fading out
  under `fadeQuick` while the tick fades in; `Motion.surface`), the
  status row crossfades to "Paid" and "… XMR received"; the buttons row
  crossfades two to one with its width settling (`Motion.settle`). The
  sale is paid, so nothing scans during this; principle 3 still holds
  because the bitmap never re-lays out.
- Live to ended: the same collapse into the 88 dp ended circle on
  `warnGround` or `dangerGround`.
- Paid to the next waiting sale (seen in `before-overlay2.mp4`: with
  more sales waiting, the panel doesn't lower after the dwell, the focus
  moves on and the next code comes up in it, decision 85): the paid hero
  grows back out into the next sale's white code card at the same centre,
  the header, amount and order line crossfading; the container stays.
  Only with nothing waiting does the panel collapse into the bubble (M1).
- Interruption: all `Transition`-based, so a second change (paid during
  the order page, say) retargets from the current frame.
- Reduced motion: `fadeQuick` swaps, the container snaps to size.
- Why: the panel's content is three different layouts today; the cashier
  sees a page replace a page. With the container persisting and only the
  content changing, it's one panel showing the next thing.

**M6. The phase ring sweeps.** `ui/PhaseRing.kt`; every place it draws:
the bubble, the panel's status row, the sale's status pill, the enlarged
code's foot, a shift row's badge, the close sheet's "Yes" glyph.
- Today: `ringFill(phase)` jumps a quarter.
- `animateFloatAsState(ringFill(phase), Motion.sweep)` read inside the
  `Canvas` draw lambda only (the composable's parameters don't change per
  frame, so nothing recomposes). The first composition starts at the
  phase's own fill, never from 0, so a ring that appears already at Seen
  doesn't sweep in. At Paid the arc completes the circle in `paid` green
  (the fill colour crossfades over the last quarter, in `drawBehind`).
- Interruption: Seen then Confirming within the sweep retargets; the
  spring is overdamped so it never passes a quarter mark.
- Reduced motion: snaps, as today.
- Not the status bar chip: Android draws the promoted notification's icon
  itself from the four vector drawables (decision 68); it steps, and
  that's the system's.
- Why: the sweep is the one thing that says "something is happening on
  the network" without words, in a cashier's peripheral vision.

### The app's screens (dev2)

**M7. Navigation has direction, and things open from where they are.**
`MainActivity.kt` (the bare `when (screen)`), `ui/Shift.kt` (`SaleRow`),
`ui/Till.kt` (`LiveSale`, `Code`, `OrderCard`), `ui/CodeScreen.kt`,
`ui/OrderPage.kt`, `ui/SaleDetail.kt`.
- Today: every screen change is a cut.
- One `SharedTransitionLayout` around one `AnimatedContent(targetState =
  screen)` in `MainActivity`, keyed so a `Sale(id)` turning into
  `Code(id)` is a transition and `Sale(a)` to `Sale(b)` is one too. The
  transition by pair:
  - Shift to sale, cashier's tap: the row's card is the sale's surface
    (`sharedBounds("sale/$id")` on the row's rounded background and on the
    sale screen's root, `Motion.surface`), content crossfading; Back
    returns it into the row if the row is on screen, else the plain back
    below.
  - Shift to sale, a new sale coming up by itself (`TillNav.follow`): the
    sale fades in and rises from 4 % of the height below (`Motion.track`,
    enter only, the shift fading under it with `fadeQuick`; a slide, not a
    scale, so its code is never smaller than at rest: decision 198); there
    is no row to grow from, and it mustn't look like something the cashier did.
    *Narrowed by §12.1 (decision 266): a sale comes up by itself only
    while the shift is on screen; Back never follows a waiting sale (the
    follow after a leave, decision 258, is superseded).*
  - Sale to code (M8), sale to order page: the order card is the page's
    list card (`sharedBounds("order/$id")`), the rest crossfades.
  - Code or order back to sale, sale back to shift: the reverse of how
    they came.
  - Anything to Settings and back: fade-through (exit `fadeQuick`, enter
    `fade` from 0.96 scale). Settings is a different place.
  - Onboarding steps: the next step slides in from the end over 30% of
    the width with `fade`, the last slides out to the start by 10%; Back
    reverses. Done once per device, so no more than that.
- Predictive back: `PredictiveBackHandler` drives a
  `SeekableTransitionState` for the back transition of each pair, so the
  code shrinks toward its card, the sale toward its row, as the gesture
  progresses; commit finishes with `Motion.surface`, cancel springs back.
  The existing `BackHandler` rules (which screens handle Back, `back(from)`
  only if still on screen, decision 77) are unchanged; the gesture never
  changes state until commit.
- Interruption: `AnimatedContent` and `SeekableTransitionState` retarget;
  `TillNav` moving the screen mid-transition (a sale paid while opening)
  is just the next target.
- Reduced motion: every pair is a `fadeQuick` crossfade; predictive back
  shows no preview (the handler still commits).
- Why: the shift is a list of sales and a sale is a row opened; today
  nothing says so. Direction tells the cashier whether they went deeper or
  came back without reading the header.

**M8. The code enlarges from the code.** `ui/Till.kt` (`Code`),
`ui/CodeScreen.kt` (`CodeScreen`, `FullBrightness`), `ui/Qr.kt` (`QrCode`).
- Today: a cut to a white screen; brightness and bar icons change in the
  same frame. In the dark theme it is the harshest cut in the app, a
  near-black ground to full white (`before-code-paid.mp4` at 0:08), and
  the shrink is the same in reverse.
- The white card is the shared element: `sharedBounds("code-card/$id")`
  on the sale's `QrCode` card and on `CodeScreen`'s white root, so the
  white ground spreads from the card's bounds to the screen
  (`Motion.surface`); the QR image inside is `sharedElement("code/$id")`
  with `ScaleToBounds`, so it is laid out once at the target size and
  scaled as a layer between the two sizes: no re-layout, no re-encode, a
  valid code every frame (principle 3). The sale's amount, pill and
  buttons fade out under it (`fadeQuick`); the enlarged screen's header
  row and foot fade in (`fade`) once the ground is past them. The 32 dp
  expand badge fades out at once. Back and the shrink button reverse it;
  the tap-anywhere does too; predictive back seeks it (M7).
- Brightness: `screenBrightness` is set once, on the first frame, as
  today. Android's `DisplayPowerController` ramps a window brightness
  override itself; the tech lead measures on the device whether the ramp
  is visible, and only if it is a jump does `FullBrightness` animate the
  attribute, and then with at most six steps over the transition (each
  `window.attributes = …` is a relayout request; it must never be per
  frame). The status and navigation bar icons flip at the midpoint.
- The code never gets smaller than its rest size on either screen; the
  shrink ends exactly at 264 dp.
- Interruption: a paid or expiry during the enlarge is `TillNav` moving
  the screen: the shared element retargets into the sale's page (the
  card becomes the hero, M10).
- Reduced motion: `fadeQuick` crossfade, white to white.
- Why: the customer is already reaching for the code when the cashier
  taps it; growing it in place keeps their camera where it was.

**M9. The paid moment is one moment, everywhere, for the same time.**
`ui/Till.kt` (`SaleScreen`), `ui/SaleDetail.kt`, `Faces.kt`
(`SalePanel`'s `delay(5_000)`), `SaleText.kt` (`Bubble.PAID_MS`,
`TillNav.PAID_STEP_BACK_MS`), `TillService.kt`, `notify/SaleNotifications.kt`.
- Today: the bubble is green and the chip a tick for 4 s
  (`Bubble.PAID_MS`); the panel shows paid for 5 s then lowers, or moves
  on to the next waiting sale when the focus moves at 4 s; the sale
  screen steps back after 5 s. So the docked bubble goes back to lit a
  second before the panel it is docked to moves on (`before-overlay2.mp4`
  at 0:24), and a second before it would collapse into it.
- One `Motion.Dwell.paid` (5 s: the acceptance criteria's number) for all
  four. The panel's exit (M1's collapse, or M5's grow into the next
  sale) *is* the end of the moment: at the dwell the panel folds into
  the green bubble, and the bubble fades to lit or idle (`Motion.fade`)
  only once the collapse has landed, driven by the collapse's
  completion, not a second timer; when the next sale comes up instead,
  the bubble fades to lit as the next code grows in. The chip's tick
  goes at the same dwell. The sale screen (M10) steps back into its row
  at the dwell.
- Interruption: Done on the panel, or Back on the sale, ends the dwell
  early and runs the same exit.
- Reduced motion: the dwell is the same; the exits snap.
- Why: a cashier sees the paid bubble, the paid panel, the paid chip and
  the paid page as one event; they must start and end together.

**M10. A sale turns into its page in place, then settles into its row.**
`ui/Till.kt` (`SaleScreen`, `LiveSale`), `ui/SaleDetail.kt`
(`StateBadge`), `ui/Shift.kt` (`SaleRow`).
- Today: `SaleScreen` swaps `LiveSale` for `SaleDetail` in one frame.
- The live sale and its page are two states of one screen (an
  `AnimatedContent` inside `SaleScreen`, or shared elements between the
  two): the white code card collapses into the page's 72 dp hero circle
  at the top centre (`sharedBounds("code-card/$id")` again, its corner
  radius ending at a circle, the QR fading out under `fadeQuick`, the
  tick or clock fading in); the amount moves from under the top bar to
  under the heading (`sharedElement`); the status pill crossfades into
  the heading line; the order card stays where it is if the page keeps
  it. The top bar's X fades out (it isn't offered once the sale isn't
  live, decision 83).
- After `Motion.Dwell.paid` on a paid sale, the page settles into its
  row in the shift (M7's container transform in reverse: the page's
  surface shrinks to the row, which exists now, with the paid tick
  already in it, so the hero visibly becomes the row's badge). *Round 1
  shipped this as a crossfade (decision 259); §12.2 makes it the row
  shrink, identical to Back (decision 266).*
- A close from the sheet: the sheet hides first (as today, decision 83),
  then this.
- Interruption: a reopen ("Show the code again") is the reverse; a paid
  arriving during the ended morph retargets to the paid hero.
- Reduced motion: `fadeQuick` crossfade; the step back is a crossfade.
- Why: "the sale got paid" is the one event a cashier must not miss, and
  today it's a cut to a different-looking page. A code folding into a
  tick is unmistakable from across the counter.

**M11. The shift list moves rows, not the cashier's eye.** `ui/Shift.kt`
(`ShiftScreen`, `SaleRow`, `Chips`).
- `Modifier.animateItem(fadeInSpec = Motion.fade, placementSpec =
  Motion.settle, fadeOutSpec = Motion.fadeQuick)` on every row: a new
  sale slides the others down; a filter change moves the rows that stay
  and fades the rest; a row leaving (a sale carried off at a new shift)
  fades. Keys are already `orderId`.
- A row's badge (ring to tick, ring to clock) crossfades (`Motion.fade`);
  its ring sweeps (M6); the words change without motion (text is read,
  not watched); the row's corner shape, which depends on its index,
  snaps (animating it would need a layout per frame of every row for
  nothing).
- Interruption: `animateItem` handles retargeting.
- Reduced motion: `animateItem` specs are `snap()`.
- Why: with several sales up, a new row arriving at the top today
  displaces everything by a row in one frame, and the cashier loses the
  one they were reading.

**M12. Words that change under a steady eye crossfade, and numbers don't
jitter.** `ui/Till.kt` (`StatusPill`), `Faces.kt` (`StatusRow`),
`ui/CodeScreen.kt` (`CodeText.under`), `ui/OrderPage.kt` (`ShowCodeBar`),
`ui/CloseSheet.kt` (`Answer`), `ui/Theme.kt`.
- The status words ("Waiting for payment" to "Seen on the network" to
  "Confirming") crossfade (`AnimatedContent` with `Motion.fade`) and the
  pill they sit in settles to the new width (`animateContentSize(Motion.
  settle)`); the pill is a two-text subtree, so a layout per frame of it
  is nothing.
- Every countdown ("14:52 left") is set in tabular figures: one
  `TextStyle` in `Theme.kt` (`tabular`, `FontFeatureSettings "tnum"`),
  used wherever `SaleText.clock` is shown, so the digits tick without the
  text shifting. The tick itself is a state change once a second and
  recomposes that text alone (as today); it is never animated.
- Reduced motion: words snap.
- Why: a cashier watching the pill for "Seen" sees the moment it changes
  instead of a flicker, and the countdown stops wobbling beside it.

### Shared (tech lead)

**M13. `ui/Motion.kt` and one reduced-motion switch.** Section 2's
presets; `Motion.reduced` as a `CompositionLocal` provided in
`TillTheme` from a `ContentObserver` on
`Settings.Global.ANIMATOR_DURATION_SCALE` (so a change takes effect
without restarting the app, and the three composables that each call
`Settings.Global.getFloat` on first composition today (`LoadingMark`,
`Circle`, `PanelCard`) stop doing a Binder call each). The existing
`reducedMotion()` becomes a read of that local. Compose already scales
its frame clock by the animator duration scale on API 33+ (so "Remove
animations" snaps every spring and tween by itself); the local is what
turns loops off and swaps spatial transforms for fades on every API.

**M14. Measurements, before and after.** Section 8.

## 5. Rejected

Each was considered and cut, with the reason, so it isn't raised again
without a new argument.

- **Window animations (`windowAnimations` styles) for the panel and
  bubble.** Free of per-frame cost, but not interruptible, not seeded by
  velocity, and they can't start from the bubble's position. M1 does it in
  the window's own Compose tree instead.
- **A true origin at the tablet's bubble.** The floating panel's window
  is only as big as the card plus its shadow; scaling about a pivot
  outside it would be clipped, and making the window full-screen would
  take every touch meant for Square (an overlay window with
  `FLAG_NOT_TOUCH_MODAL` still owns touches inside its bounds). The pivot
  is the card's nearest corner, which reads as "toward the bubble".
- **Animating brightness by hand.** A `window.attributes` write is a
  relayout request; doing it per frame is exactly the kind of code the
  owner ruled out. The system ramps overrides itself; measure first (M8).
- **Any motion on the QR modules, a fade-in of the code, a redraw as it
  scales, or a "scan me" shimmer.** Principle 3. The code is a bitmap
  scaled as one layer, nothing else, ever.
- **A status bar chip that sweeps.** Android draws it from the four
  vector drawables (decision 68); animating it would mean posting
  notifications per frame.
- **Chip selection colour fades, the "Needs action" count appearing with a
  pop, the Watching dot changing colour slowly.** Discrete states the
  cashier reads, not watches; a fade would only make them look slower.
- **Staggered entrance of the order's lines, the shift's rows or
  onboarding's features.** Decoration that delays reading.
- **A count-up on amounts.** A cashier reads a number once; it must be
  the number the first time.
- **Button press scales, ripple replacements, a shaking refund button on
  error.** Material's ripples are enough and are the platform's.
- **A loading spinner replacing the loading mark; a progress bar for the
  countdown.** The mark exists; the countdown is a number by design.
- **A pulsing "Paid" or a confetti/particle moment.** The paid moment is
  the ring closing and the tick arriving (M4, M10) and one haptic; a
  louder celebration would be noticed, which is the opposite of the goal.
- **Animating the bubble's y while it is dragged, or an inertial drag.**
  The finger has it; it must be exactly under the finger.
- **A `Modifier.animateContentSize` on the panel card for every content
  change.** It would re-measure the whole card (and the QR's `Image`)
  per frame on every tick of the countdown that changes the status
  width. Only the status pill (M12) and the container swaps (M5) animate
  size, and the QR is never inside a size-animating node.
- **Launching the app from the panel with a scale-up `ActivityOptions`
  from the code's bounds** (`makeScaleUpAnimation`): tried in thought;
  Android 12+ draws its own launch (the splash and task animation) and
  ignores a scale-up from a non-activity source, and the app opens on the
  shift, not on the code, so there is nothing for it to grow into. Left
  to the system.
- **The idle shift's faded logo breathing.** It is a placeholder, not a
  state the cashier waits on.
- **`androidx.dynamicanimation` or Material motion libraries.** No new
  dependencies; Compose's springs are the same physics.

## 6. Haptics

One `HapticFeedbackConstants.CONFIRM` (API 30+, else `CONTEXT_CLICK`) when
a payment lands, performed by the surface the cashier is looking at and
by it alone: the bubble's window when the overlay is up (the app isn't on
screen), the sale screen when the app is (the panel and bubble aren't
shown while the app is visible, so there is no double). The notification
channels are silent (`setSilent`), so there is no second buzz from the
shade. `performHapticFeedback` on the view respects the system's haptic
settings, so nothing is added for that. Nothing else buzzes: not the
grabber, not the snap, not the sheet, not a phase step, not an error (the
words and the red do that).

## 7. Reduced motion

One switch, `Motion.reduced` (M13), true when the animator duration scale
is 0 ("Remove animations" in Accessibility, or the developer option).
Then:

- Loops (pulse, glints) don't run; the bubble and mark are still.
- Spatial presets (`surface`, `settle`, `flick`, `sweep`) are `snap()`.
- Crossfades are `fadeQuick` (120 ms): a crossfade is not motion, and a
  hard cut between a white screen and a dark one is harsher than a short
  fade for the people who ask for this.
- Predictive back shows no preview; Back still works.
- `animateItem` specs are `snap()`.
- Dwells (`Dwell.paid`) are unchanged: they are reading time, not motion.

A scale of 0.5× or 2× is left to Compose's frame clock scaling, which
every preset goes through on API 33+.

## 8. Measuring

Before numbers are taken on the base commit and after numbers on the
integration branch, by the tech lead, on the same device, and kept in
`docs/reports/motion/baseline.md` (before, with the scripts and the
emulator caveats) and `docs/reports/motion/after.md`. The emulators here run software GL
(`swangle`) on a host whose load average passed 60 during this round
(the app ANR'd on its first frame once, and a frame took 9 s), so their
frame times say whether something is *wrong*, not how it *feels*; the
final numbers come from a physical device, and from emulators only with
the host idle and compared before to after on the same instance.

| What | How | Flows |
| --- | --- | --- |
| Frame timing | `adb shell dumpsys gfxinfo com.monokulo.till.debug reset`, run the flow ten times, `dumpsys gfxinfo com.monokulo.till.debug framestats`: janky frames %, 90th/95th/99th percentile | panel raise and lower ×10; code enlarge and shrink ×10; shift to sale and back ×10; a paid moment ×3; a new sale arriving on the shift with five rows ×5 |
| Memory | `dumpsys meminfo com.monokulo.till.debug` after each flow, at rest 10 s later: PSS total, Graphics, Java heap | the same flows; the overlay windows up and down |
| APK size | `app-release.apk` (minified) and the debug APK, bytes | before and after |
| Recomposition | Layout Inspector's recomposition counts on `PanelCard`, `QrCode`, `PhaseRing`, `SaleRow`, `StatusPill` during each flow; the target is zero per frame of animation | the same flows |
| Allocation | Android Studio's allocation tracker over one panel raise and one enlarge: no allocations inside draw lambdas | two flows |

Pass: no new janky frames over the before run at the 95th percentile on
the device; PSS at rest within 2 MB of before; APK within 50 KB; zero
recompositions per animated frame of the named composables.

*The 2 MB rule is superseded (decision 268): the owner accepted the
+4.9 to +5.9 MB step after a trim that decision 253 named. The pass is
now "no worse": round 2's after-minus-before on the same emulator, same
flow, trimmed, within the run-to-run noise; §12.7.*

## 9. Order of work and tracks

dev1 (overlay): M13's presets first (the tech lead lands `Motion.kt`
within the hour; until then dev1 writes against the names in section 2),
then M6 (the ring is used by both tracks; dev1 owns `PhaseRing.kt`), M4,
M1, M2, M3, M5, M9's overlay half.

dev2 (screens): M12 (small, everywhere), M11, M7, M8, M10, M9's app half.

Tech lead: M13, M14, the integration branch, and decisions 147 onward in
`docs/decisions/android.md` for anything above that is a call rather than
a refinement (M9's dwell is one).

Conflicts to watch: PR #18 touches `BubbleFace` and `Mark.kt` (M4 note);
PR #16 and #17 don't touch these files.

## 10. Performance rules

What every motion commit is reviewed against (the owner's "blazing fast,
memory efficient, no hacky animation code"), in one place so a review says
"rule 3" and not "I'd rather". Decisions 148 to 154 say why.

1. **Animated values are read in draw or layout only**: `graphicsLayer {}`,
   `drawBehind {}`, `Modifier.offset {}`, a `Canvas` lambda, a
   `Modifier.Node`. A composable's parameters never change per frame.
   `QrCode`, `PhaseRing`, `PanelCard`, `SaleRow` and `StatusPill` recompose
   zero times per animated frame (Layout Inspector's counts).
2. **Nothing is allocated per frame**: no lambdas, `Path`s, `Shape`s,
   `String`s or data classes inside a draw or animation lambda; shapes and
   paths are made once (`MarkPaths` is the model).
3. **One frame clock, Compose's.** No `ValueAnimator`, `Handler.postDelayed`,
   `delay()`-chained choreography, reflection or hand-rolled loops. A motion
   that needs "wait, then" is two motions, or one driven by the other's
   progress or completion.
4. **Every spec is a named preset** from `ui/Motion.kt` (`motion.surface`,
   `settle`, `flick`, `sweep`, `fade`, `fadeQuick`, `pulse`, `glints`,
   `Motion.Dwell.PAID_MS`), through `.fraction`, `.px` or `.spec(threshold)`.
   A literal duration, stiffness or threshold at a call site fails review.
   Springs for anything with a position; tweens only for opacity.
5. **Every transition is interruptible and keeps its velocity**:
   `Animatable.animateTo` from the current value with `initialVelocity`;
   `Transition`, `AnimatedContent` and `SeekableTransitionState` retarget.
   Nothing blocks input. The payment code is scannable on every frame.
6. **The code is one bitmap, scaled as one layer.** Encoded once per text
   (`codeBitmap`), laid out once, moved only by `sharedElement` with
   `ScaleToBounds` or a `graphicsLayer` scale; never inside an
   `animateContentSize` node, never re-laid out, re-encoded, faded in or
   drawn module by module; at rest never below its designed size.
7. **Two loops in the app**: the bubble's pulse and the loading mark's
   glints, each only while something is waited for and visible (the pulse:
   the free bubble, a code Waiting or Partial, the panel down). Two loops in
   one window share one `rememberInfiniteTransition`. *Round 2 (decision
   267): the bubble's loops are the pulse and the border's chase, in one
   `rememberInfiniteTransition`, Waiting only; the glints leave the
   bubble and run only where `LoadingMark` is still used (§12.5).*
8. **Reduced motion is one switch**: `motion.still` (`Motion.Still` while
   the animator scale is 0, followed live). Loops off, spatial presets
   `snap()`, crossfades `fadeQuick`, predictive back without a preview,
   dwells unchanged. No other read of the animator scale anywhere.
9. **Overlay windows**: made per open, content added at progress 0, removed
   with `removeView` after the exit lands (`removeViewImmediate` only in
   `stop()`). The bubble's x on release is the only window-attribute
   animation (one `updateViewLayout` per frame for about 300 ms); nothing
   else animates window attributes, `screenBrightness` included.
10. **Stable inputs**: `TillActions` and `PanelActions` are `@Immutable` and
    remembered per sale; what a screen reads is hoisted behind
    `derivedStateOf` or `remember` so the root `AnimatedContent` isn't
    handed new lambdas on every poll of the runner.
11. **No new third-party dependency**: androidx compose animation,
    foundation, ui and activity-compose; for tests `ui-test-junit4` and
    `ui-test-manifest`, with `androidx.test:runner` 1.7.0 and
    `espresso-core` 3.7.0 pinned (the ones they bring reflect on
    `InputManager.getInstance`, gone in Android 16). No Robolectric, no
    dynamicanimation, no Material motion library, no BOM bump (2026.09.00
    has everything).
12. **Measured before and after** with the same scripts on the same emulator
    instance in one session (`gfxinfo reset`/`framestats`, `meminfo`,
    `apkanalyzer`), compared as counts and ratios; final numbers on a
    device. Pass: no new janky frames at p95 on the device, PSS at rest
    within 2 MB, APK within 50 KB, zero recompositions per animated frame of
    the composables in rule 1. *"Within 2 MB" is superseded by "no worse
    than round 1's step" (decision 268, §12.7).*

## 11. Tech lead amendments

Where the spec met the codebase or the performance rules, what changed and
why, each against the section or item it amends. Decisions 154 to 156 in
`docs/decisions/android.md` record the calls.

1. **Presets are generic over what moves (§2, M13).** A Compose spec is
   typed by its value: `FiniteAnimationSpec<Float>` for a progress,
   `<IntOffset>` for `animateItem`'s placement, `<IntSize>` for
   `animateContentSize`, `<Rect>` for a `boundsTransform`, `<Color>` for a
   ground; one `Motion.surface` value can't be all of them. Each preset is
   a `Preset` with `fraction` (a value in 0..1, finished within a
   thousandth so a full-screen surface never jumps its last pixel), `px`
   (within half a pixel) and `spec(visibilityThreshold)` for the rest. Call
   sites read `motion.surface.fraction`,
   `motion.settle.spec(IntOffset.VisibilityThreshold)`,
   `motion.surface.spec<Rect>()`. §2's thresholds live inside the preset,
   never at a call site.
2. **`Motion.reduced` is `motion.still`, and the reduced presets are a
   second instance (§2, §7, M13).** `Motion.Still` is the same class with
   `snap()` for the spatial presets, the 120 ms tween for `fade`, and
   `pulse` and `glints` null, so a loop can't be started by mistake
   (`rememberInfiniteTransition` needs a spec to run). `TillTheme`
   provides `LocalMotion` from `rememberMotion()`, one Binder read per
   window and a `ContentObserver` after; `reducedMotion()` is gone.
3. **`Motion.Dwell.paid` is `Motion.Dwell.PAID_MS` (§2, M9)**, the repo's
   style for a constant with a unit (`FLING_DP_PER_S`, `GLINT_MS`).
   `Bubble.PAID_MS` (4 s) and `TillNav.PAID_STEP_BACK_MS` are removed; the
   panel's, the service's, the overlay's and the view model's timers read
   the one constant now. The rest of M9 (the bubble's fade following the
   panel's collapse) is the tracks'.
4. **Instrumented tests have a harness (§8, decision 152).**
   `ui-test-junit4` and `ui-test-manifest` (androidx, test-only), with
   `AndroidJUnitRunner`; `androidx.test:runner` 1.7.0 and `espresso-core`
   3.7.0 are pinned because the versions `ui-test-junit4` brings fail on
   API 36 (`NoSuchMethodException: InputManager.getInstance`) before any
   test runs. `app/src/androidTest/` holds the foundation's two tests,
   green on `Pixel_9_Pro_motion` (emulator-5558); the tracks add theirs
   beside them and run `connectedDebugAndroidTest` on their emulator
   before a PR.
5. **The code's bitmap cache is in the foundation (M5, M8, decision 153).**
   `codeBitmap(text)` in `Qr.kt` keeps the last four codes, so the sale
   screen, the enlarged code, the panel and a shared element between them
   get one `ImageBitmap` for one text; `QrCode` reads it already. Both
   tracks' shared elements need it, so neither waits for the other.
6. **The handoff frame is a pre-draw listener (M1).** "The free bubble
   window is removed on the frame the panel window has drawn" is
   `View.doOnPreDraw` (core-ktx; one listener that removes itself) on the
   panel's view, and the same on the free bubble's view for the lower; not
   a post, not a delay. Both windows are added at progress 0, so the
   pre-draw frame shows nothing either way.
7. **The bubble's snap runs on the UI dispatcher (M3).** An `Animatable`
   needs a `MonotonicFrameClock` in its coroutine context, which
   `lifecycleScope` doesn't carry: the snap is
   `scope.launch(AndroidUiDispatcher.Main)` on the overlay's existing
   scope (the dispatcher that ticks with the Choreographer), and the next
   `ACTION_DOWN` cancels the job. No scope per bubble, no
   `GlobalScope`.
8. **PR #18 and the bubble (M4).** `BubbleFace(look, pulse, phase)` keeps
   its signature; when #18 lands, its loading mark's glints and the pulse
   share the bubble's one `rememberInfiniteTransition`. dev1 reads #18's
   diff before starting M4 and says in the PR what will conflict.
9. **`AnimatedContent`'s key is the `Screen` itself (M7).** The screens
   are data classes, so the default `contentKey` already makes `Sale(a)`
   to `Sale(b)` a transition and `Sale(a)` to `Sale(a)` with new sales
   none. No custom key.
10. **One clock per screen (M12).** One `LaunchedEffect` ticking a
    `mutableLongStateOf` at the screen's root, handed down as a
    `State<Long>` and read only inside the `Text` that shows the
    countdown, so a tick recomposes those texts and nothing above them;
    `secondsLeft(expiresAt)` with its own effect per text goes.
11. **Who owns the shared files (§9).** dev1: `overlay/*`, `ui/PhaseRing.kt`
    (M6, used by both), `ui/Mark.kt`, `TillService.kt`. dev2: `ui/Parts.kt`,
    `ui/Qr.kt`, `ui/Theme.kt`, `ui/Till.kt`, `SaleText.kt`,
    `MainActivity.kt`, `AppViewModel.kt` and the rest of `ui/`. The lead:
    `ui/Motion.kt`, `notify/SaleNotifications.kt`, the docs and the
    numbers. A change needed in the other track's file is asked of its
    owner, not made.

## 12. Round 2: the waiting tray, Back, and the logo bubble

The owner's decisions of 2026-10-30, after reading round 1's showcase
(`docs/reports/motion.md`), and the design the owner approved for them:
the handoff canvas (claude.ai/artifact/WiZwopcdX7qrCYaAuknYdb), whose
nine artboards are read here from disk (`Main`, `TrayOpen`, `TrayDark`,
`TrayFew`, `BackSequence`, `TraySpec`, `BubbleStates`, `BubblePrototype`,
`BubbleSpec`). The canvas is the source of truth and is matched exactly;
where it and the code disagree on an *existing* value, the code's value
wins unless the canvas states a change, and every such call is in §12.6.
Decisions 266 to 268 in `docs/decisions/android.md` record the round.
`docs/reports/motion/before-r2/` holds clips of today's behaviour on
`motion-main` (65a81c7), the ones this section changes.

What this section supersedes from round 1, each marked where it stands:
the follow after a leave (decision 258; now decision 266, §12.1), the
paid step-back as a crossfade (259; 266, §12.2), the bubble's ring,
pupil, loading mark and `paidGround` tick (M4, decision 265 a; 267,
§12.5), and the 2 MB memory rule (§8, §10 rule 12; 268, §12.7).

The owner's goals are unchanged and every item below is read against
§10: animated values in draw or layout only, no per-frame allocation or
recomposition, no `delay()` choreography, presets only (two new ones,
§12.5), interruptible, one reduced-motion switch, the code never
animated module by module, colours through `ui/Theme.kt`'s roles or
`Fixed` for the bubble, no new dependency.

### 12.1 Back lands on the shift, always

`AppViewModel.kt` (`follow`, `back`, `backTarget`), `SaleText.kt`
(`TillNav.follow`), `MainActivity.kt` (`TillStage`, `StageBack`),
`ui/Shift.kt`.

- The rule: pressing Back on a sale (button or gesture) lands on the
  shift, never on another sale, even when one or more sales are waiting
  unseen. A sale that arrived while the cashier was on another sale (or
  on Settings, the enlarged code, the order page) waits in the tray
  (§12.3); its bubble and its notification still call. This replaces
  decision 258's "a leave lands, then the follow starts": there is no
  follow on a leave at all.
- What still comes up by itself: a sale that arrives while the shift is
  on screen and at rest rises out of it as in M7 (`Move.Rise`, decisions
  198 and 222). `TillNav.follow(Till, …)` is therefore no longer "the
  newest live sale not left" but "a sale that arrived while the shift
  was showing"; a sale the till did not follow when it came is never
  followed later. The code screen and the order page still give way to
  the sale's page once it is paid or ends (`follow`'s other arms are
  unchanged).
- The move, Back from a live sale into its tray row (`BackSequence`,
  `Motion.surface` on `Motion.track`, 450 ms; the existing `RowGrow`
  reversed, landing in the tray instead of the plain list):

  | Time | What the cashier sees |
  | --- | --- |
  | 0 ms | The live sale fills the screen. Back starts `RowGrow Sale → Till` toward the sale's row, the first in the tray. |
  | 90 ms | The sale's words are going (gone by 120 ms, `fadeQuick`); its ground, now an empty white card, is pulled by `surface`; the shift's chrome starts fading through under it. |
  | 220 ms | The card finds its row: corners from 0 to the row's 20 dp as it shrinks (the first row's top corners; a middle row's are 0, as `rowLook` has them). The tray and its other rows are already in place; nothing else moves. |
  | 330 ms | Landed within half a pixel of the row. The row's words fade in at their own size (`fade`, 200 ms); they are never drawn scaled (decision 195). |
  | 450 ms | At rest: the shift, the tray folded at three rows. |

  The code stays drawn on its card, whole, on every frame the ground
  holds the card (decision 234, with 260 a fixed: §12.4).
- Back into a folded row: if the sale being left would be folded away
  (it is the fourth or later live sale), the tray opens as the cashier
  lands so the sale has a row to shrink into (the rows past the third
  fade in under the arriving card, `animateItem`'s `fade`), and stays
  open until "Show fewer" or the shift is left.
- A second Back while the shift is still arriving (decision 260 c): it
  lands on the shift, never leaves the app. `StageBack` stays enabled
  while `screen == Till && stage.currentState != stage.targetState`,
  with `model.back(Till)` the no-op it is; a stray press must never take
  the cashier out of the till.
- Interruption: a tap on a tray row mid-landing is the next target
  (`RowGrow` forward again from where the stage is, decision 249); a
  sale that arrives mid-landing goes into the tray under the moving
  card and does not take the move over.
- Reduced motion: a 120 ms crossfade, no move; the tray opens with a
  snap when it must.
- TalkBack: after Back, focus goes to the row the cashier came from.
- Tests (`TraySpec`): Back with a waiting unseen sale ends on the shift
  with the ground in the left sale's tray row (`RunningMovesTest`); Back
  into a folded row opens the tray first and the ground lands in the
  row; a second Back while the shift is arriving lands on the shift.
- Before: `before-r2/back-with-sale-waiting.mp4` (Back on sale #1655
  with #1656 waiting: #1655's words fade, #1656's code replaces its code
  in a frame, the shift is never drawn; the second Back is the row
  shrink into the plain list).

### 12.2 The paid step-back is Back

`ui/Till.kt` (`SaleScreen`), `AppViewModel.kt` (the dwell), `Nav.kt`.

- After `Motion.Dwell.PAID_MS` the paid page leaves exactly as a Back
  pressed on it would: `RowGrow Sale → Till` with the `sale/<id>` course,
  the page's ground shrinking into its row, which by then sits at the top
  of "Earlier" (§12.3), the hero's tick becoming the row's badge (M10's
  glyph between hero and badge, decision 180), the page's words out under
  `fadeQuick` and the row's in under `fade`. The owner wants it to feel
  identical to pressing Back; it is the same move with the same ends,
  started by the clock instead of the finger.
- Interruption: Back during the dwell runs the same exit at once; a tap
  on the page during the step-back is nothing (the page is leaving).
- Reduced motion: a 120 ms crossfade, as Back is.
- Test: the step-back's trace carries the course, and lands in the row
  at the top of Earlier.
- Before: `before-r2/paid-step-back.mp4` (the fold to paid, the 5 s
  dwell, then the words fading and the shift's list fading in with no
  surface moving: decision 259's crossfade).

### 12.3 The waiting tray (A4, opens in place)

`ui/Shift.kt` (`ShiftScreen`, `SaleRow`), `ShiftList.kt` (the rows and
their sections). Artboards: `Main` (folded, light), `TrayOpen`,
`TrayDark`, `TrayFew` (two waiting: no fold), `TraySpec`.

**What it is.** Under the chips, a section header "Waiting for payment"
with the count at its end, then an amber tray (`warnGround`) holding a
white card (`card`) of every live sale: waiting, seen, confirming, part
paid. More than three live sales fold after the third row into a stack
inside the tray: two card edges peek under the third row over a "+N"
pill (the fixes canvas's board 4); tapped, or pulled down, the tray opens
in place and the pill reads "Show fewer". Under the tray, an "Earlier" header
and the rest of the shift as today: paid, closed, expired, refunded and
other tills' sales. With no live sale there is no tray, no header and no
"Earlier" header: the list reads exactly as today. Nothing else is
added: no "New" tag, no bottom bar, no snackbar, no banner, no stripe,
no outline. The tint and its header are the separation.

**Layout** (the row itself is unchanged):

| What | Value |
| --- | --- |
| Section header row | padding 18 dp top, 24 dp start and end, 8 dp bottom |
| Header text · count | 14 sp 700 `ink` · 14 sp 600 `muted`, tabular figures |
| Tray outer margin | 16 dp each side (the rows' margin today) |
| Tray corner · padding | 24 dp · 4 dp |
| Inner card corner | 20 dp (24 − 4, so the card sits concentric in the tray; `RowCorner` today) |
| Sale row | min 64 dp, padding 16/12/12/12 (start, top, end, bottom), as today |
| Row ring · gap | 36 dp (`RingRowSize`) · 12 dp, as today |
| Divider | 1 dp `divider`, inset 64 dp, as today |
| Fold row | 48 dp high (touch ≥ 48), full width of the tray, centred |
| Fold text · chevron | 15 sp 600 `ink` · 16 dp chevron, 6 dp gap, pointing down folded and up open |
| Fold threshold | 3 rows shown |
| Earlier header | padding 22 dp top, 24 dp start and end, 8 dp bottom |
| Column max width | 640 dp, as today |
| Tablet, split screen, a phone on its side | the same one layout, the column capped at 640 dp and centred, as the shift is today |

**Colour roles.** Every colour is an existing role in `ui/Theme.kt`; no
new tokens (the roles test enforces it):

| Part · role | Light | Dark |
| --- | --- | --- |
| Tray ground · `warnGround` | #F1E3CF | #3D3320 |
| Inner card · `card` | #FFFFFF | #2A2825 |
| Header · `ink` | #1A1917 | #ECE8DF |
| Count · `muted` | #5A564C | #A8A29A |
| Fold label and chevron · `ink` | #1A1917 | #ECE8DF |
| Divider · `divider` | #ECE6DA | #3A3733 |
| Waiting sub-line · `link` (as today) | #B34700 | #FF8A3D |
| Ring track · `chip` (as today) | #E4DFD5 | #3A3733 |
| Ring arc · `accent` (as today) | #FF6600 | #FF6600 |

The fold label is `ink`, not `link`: ink on the tray ground is 13.9:1
light and 10.1:1 dark, while the link orange measured 4.36:1 on the light
tint, under 4.5:1; the fold is a neutral button, as the theme's
one-orange-primary rule wants. Row text on the inner card is as today.

**Copy** (`SaleText`/`ShiftList`):

| Where | Text |
| --- | --- |
| Tray header | Waiting for payment |
| Count | the number of live sales, e.g. 21 |
| Fold, folded | +N, N = count − 3 ("+1") |
| Fold, open | Show fewer |
| Earlier header | Earlier |
| Row title | Sale #1257 · $12.00 (unchanged) |
| Row sub-line | Waiting · 4:12 left · Seen on the network · Confirming · Part paid · 3:10 left (unchanged) |

**Behaviour.**

- Order: newest first in both sections, as the shift orders today, so a
  new sale lands at the top of the tray and a sale just paid at the top
  of Earlier.
- Fold state: kept for the shift screen's life (`rememberSaveable`),
  across rotation; a fresh shift starts folded. Back into a folded row
  opens it (§12.1).
- Filters: All shows the tray and Earlier. Needs action shows the tray,
  unfolded, then the ended-unpaid sales under Earlier. Paid shows Earlier
  only, and with no tray there is no header, as today.
- Counts: the header count and the Needs action badge count different
  things (live sales; everything needing action). Both are shown as they
  are.
- The idle shift (no sales at all) is unchanged.

**Motion.** Presets only; no stagger; the tray has no height animation of
its own, it is its rows:

| What | Preset | Note |
| --- | --- | --- |
| Back from a sale into its tray row | `surface` on `track` | §12.1's keyframes: `RowGrow` reversed, now landing in the tray |
| Paid step-back after the dwell | `surface` on `track` | §12.2: the same `RowGrow` reversed into the row at the top of Earlier |
| Tray opens | `animateItem`: `fadeIn` `fade` · placement `settle` | The N rows fade in where they will sit; Earlier slides down on `settle`. No stagger, no tween of the tray's height: it is the rows |
| Tray folds | `animateItem`: `fadeOut` `fadeQuick` · placement `settle` | Rows past the third fade out; Earlier slides up. If the fold row would leave the screen, the list scrolls so the fold row stays under the finger |
| Fold label and chevron | `fade` · chevron rotation `settle` · pill width `settle` | "+N" fades through to "Show fewer"; the chevron turns 180° (read in `graphicsLayer`) |
| Stack pulled | the finger · release `flick` with its velocity · fold `settle` | The edges follow the finger down, each taking half the pull, toward a row's height and the card's width (laid out and drawn from the pull, never composed); let go past a third of the reach or flung down, the tray opens, else they settle back. A fold change starts the pull 13 dp off so the pill never jumps as the edges come or go |
| A sale is paid or ends | `fadeOut` `fadeQuick` · `fadeIn` `fade` · placement `settle` | Its row leaves the tray and arrives at the top of Earlier; the rows around it settle. The last waiting sale gone: the tray and its header fade out (`fadeQuick`) and Earlier slides up |
| A new sale arrives on a shift that is showing | `fadeIn` `fade` · placement `settle` | Lands at the top of the tray (under the sale rising out of the shift, when it does: §12.1); folded, the third row moves into the fold and the count rises by one |
| Animations off (`Still`) | snapped · 120 ms fades | Back and the step-back are 120 ms crossfades; open and fold jump; labels swap |

The rows' `animateItem` is M11's; the tray adds two keyed items (the
header row and the fold row) and one more (the Earlier header) to the
same `LazyColumn`, so one placement spec moves everything and nothing is
measured per frame outside the rows that move.

**TalkBack.**

| Where | Says |
| --- | --- |
| Tray header (a heading) | "Waiting for payment, 21 sales" ("1 sale") |
| Fold row (a button) | "Show 18 more waiting sales", state collapsed; then "Show fewer", state expanded. Focus stays on it |
| Rows | Unchanged: "Sale #1257, $12.00, waiting, 4 minutes 12 seconds left, show the code" |
| After Back | Focus goes to the row the cashier came from |
| Changes | No announcement when a sale joins or leaves the tray; the bubble and the notification already speak |

**Tests to add.** Fold shows 3 rows and "Show N more waiting"; open shows
all and "Show fewer"; the counts at 4 and 21; two waiting shows no fold
(`TrayFew`); no live sale shows no tray and no headers; the Needs action
filter shows the tray unfolded; every tray colour comes from a theme role
(the existing roles test); and §12.1's and §12.2's.

### 12.4 The frames to tidy (decision 260 a, b, c)

- (a) On a Back from a live sale and on "Close it for now", the code is
  drawn whole on its card on every frame the row's ground holds the
  card, including the one or two frames on which that ground covers the
  whole screen (decision 234's rule, which these two paths broke: a bare
  ground, luminance 242, no code; `review-r6/phone-back-into-row.webp`,
  `phone-close-for-now-light.webp`). The shrink into the row never begins
  with a code-less frame. The code leaves only when the ground no longer
  holds its rest margin (decision 189), as the other paths already do.
- (b) In the sale screen's paid fold the tick belongs to the last share
  of the fold (`crossfadeOnTrack`), as decision 252 has it on the code
  screen: it is never drawn over the fading code while the card is still
  four fifths of its size (`review-r6/phone-paid-fold-on-sale.webp`,
  frames 3 to 6 of 9). One glyph in the card at a time.
- (c) A second Back while the shift is still arriving lands on the
  shift: §12.1.
- Reduced motion: (a) and (b) are about which frames draw what, not
  motion; under `Still` the crossfade already draws one thing at a time.
- Tests: a `RunningMovesTest` frame check that the code is drawn on
  every frame of a Back and a close-for-now until the ground leaves the
  card; the paid fold's tick alpha is 0 until the last share; the second
  Back.

### 12.5 The bubble is the logo (B5 c, "the border chases")

`overlay/Faces.kt` (`BubbleFace`, `Circle`), `ui/Mark.kt` (`mark`,
`MarkPaths`), `ui/Motion.kt` (two presets), `ui/Theme.kt` (`Fixed`).
Artboards: `BubbleStates` (the three states at 3×, the chase at five
points of its turn), `BubblePrototype` (the timing, interactive),
`BubbleSpec`.

**The three states.** One mark, drawn by `mark()` in every state; only
its colours and its border change.

- Idle: no sale waiting. Grey mark (the mark's greyscale shades), grey
  border, the whole bubble at 55 %, ground white at 90 %. As today; the
  only change is that it is drawn by the same `mark()` as the others.
- Waiting: a code waits for money. A still orange lens, no glints (the
  loading mark's glints from #18 leave the bubble; `LoadingMark` stays
  where else it is used). The logo's own border becomes a pale track
  (#D8D2C7) with a black (#1A1917) segment, 30 % of the border, chasing
  clockwise round it at constant speed, one turn per 1250 ms, starting at
  12 o'clock. The pulse rings out behind, as today, Waiting and the panel
  down only. Seen and Confirming keep chasing: the step is in the panel,
  the notification and TalkBack, not the bubble.
- Paid: the glass fills green from the core outwards and a white tick
  draws over it; the border is whole and black. No spinner, no green
  ring, no pulse: only the glass and the tick say it. One `CONFIRM`
  haptic as today (§6). The panel, if up, folds into this bubble as it
  does now (M9, decision 158). Held `Motion.Dwell.PAID_MS`, then Idle
  (the green spreading to grey the same way, the border to grey).
- Ended or closed unpaid: back to Idle on a fade, no red, as today; the
  shift and the notification say what happened.
- The bubble no longer draws `PhaseRing` or the pupil; the rows, the
  chip and the panel keep theirs. The bubble's appear and disappear, its
  alpha and ground in the layer, and its docking are M4's, unchanged.

**Geometry** (mark units `u` are the SVG's 64-unit box, converted at the
46 dp logo: 1 u = 46 / 64 × 1.42 dp; board 7 of the fixes canvas grew it
from 38 dp so the disc round the border is about 7 dp, not 11):

| What | Value |
| --- | --- |
| Bubble circle | 56 dp, window padding 16 dp (unchanged) |
| Elevation | 6 dp shadow (unchanged) |
| Logo | 46 dp, centred, in every state (the lit face was `LoadingMark` at 40 dp: §12.6) |
| Lens | 32.1 dp across (r 15.75 u) |
| Border radius · stroke | 18.4 dp · 4.6 dp (r 18 u · 4.5 u); about 7 dp of disc round it |
| Chasing arc | 30 % of the border, round caps (34 of 113.1 u) |
| Arc start · direction | 12 o'clock · clockwise |
| Tick | `M23.5 28.5 l4.3 4.3 8.7 -9` (u), inside the lens's clip |
| Tick stroke | 3.3 dp (3.2 u), round caps and joins |
| Pulse | `accent` at 55 % to 0, radius +16 dp (unchanged) |
| Idle alpha | 0.55 for the whole bubble, ground 90 % (unchanged) |

**Colours.** The bubble sits over another app, so its colours are
`Fixed`, not the theme's. In night mode (board 7) its disc is
`Fixed.BubbleDark` #2A2825, the track #4A4743, the arc and the whole
border #ECE8DF and the idle border #8C8577; the lens, the tick, the
pulse and the alphas are as in the table, which is light mode. The two lighter greens are new and
go in `Fixed` beside `Paid`; the idle grey border (#565656, a literal in
`Faces.kt` today) moves to `Fixed` too.

| Part | Idle | Waiting | Paid |
| --- | --- | --- | --- |
| Lens, light facets (shade 0) | #7F7F7F | #FF6600 | #2EA052 |
| Lens, mid facets (shade 1) | #747474 | #E85D00 | #238A43 |
| Lens, core (shade 2) | #616161 | #C24E00 | #1A7F37 (`Fixed.Paid`) |
| Border track | #565656 | #D8D2C7 | #1A1917 (`Fixed.Ink`) |
| Chasing arc | (none) | #1A1917 | (none: the track is ink) |
| Tick | — | — | #FFFFFF |
| Ground | #FFFFFF at 90 % | #FFFFFF | #FFFFFF |
| Pulse | — | #FF6600 (`Fixed.Accent`) | — |

Contrast: the white tick is 3.4 to 5.1:1 on the three greens and the
black arc 11.7:1 on its pale track, both above 3:1 for graphics. The Idle
bubble is decorative at 55 % and says nothing a cashier must read.

**Motion.** Two additions to `Motion`: `chase`, a loop like `pulse`, and
`spread`, a step. Everything else is an existing preset.

| What | Preset | Note |
| --- | --- | --- |
| Idle ↔ Waiting ↔ Paid, the whole bubble | `fade` | Alpha 0.55 ↔ 1 and the ground, read in the layer as today (M4) |
| Lens colour, grey → orange → green (and back) | `spread`: `fade` per ring, each ring `SPREAD_STEP_MS` (60) after the last | Four rings from the core out: the core; the inner six; the middle twelve with the base circle; the rim twelve (`MarkPaths.facets` in that order). They start at 0, 60, 120 and 180 ms, so the lens is done by 380 ms. Each ring's colour is lerped in the draw from one progress per ring |
| The chase (Waiting only) | `chase`: a loop, one turn of 1250 ms, `LinearEasing` | The arc's start angle runs 0 → 360° per turn. It starts with Waiting and stops on Paid or Idle; the arc's alpha fades out (`fade`) as the track darkens to ink or grey |
| Border track colour | `fade` | Grey → pale → ink with the state |
| The tick | `sweep`, after the core's ring | Trimmed 0 → 1 along its path once the core is green (its spec starts where the core's ring ends, a start in the spec, not a `delay()`); overdamped, so it never overshoots |
| Pulse | `pulse`, as today | Waiting and the panel down only |
| Paid hold | `Dwell.PAID_MS` | Then Idle on `fade`; green to grey spreads too. The tick leaves by fading whole on `fadeQuick` (its alpha), never un-drawn: its trim holds and is rewound (`untick`) once it is invisible (decision 293) |
| Animations off (`Still`) | no loops · 120 ms fades | No chase: Waiting shows the whole border in ink (#1A1917). No pulse. The lens changes in one 120 ms fade, no spread; the tick appears whole |

`Motion.spread(ring)` is a `Timed` preset, `fade`'s length with a delay of
`ring × SPREAD_STEP_MS` (as `fadeThrough` has a delay in its spec); under
`Still` it is `fadeQuick` with no delay. `Motion.chase` is a
`loop(CHASE_MS, LinearEasing)`, null under `Still`, beside `pulse`.

**Build.**

- One draw, no recomposition: `mark()` in `ui/Mark.kt` takes the lens
  shades per ring, the spread's progress per ring, the border's track
  colour, the arc's start angle and alpha, and the tick's trim, all read
  in the draw phase from `Animatable`s or `Transition` values and one
  infinite transition. Paths stay the prebuilt `MarkPaths`; the tick is
  one more prebuilt `Path` measured once (`PathMeasure`) for its trim;
  nothing allocates per frame. The arc is one `drawArc` with the stroke
  and round caps, start angle from the loop.
- Only while seen: the chase's infinite transition exists only in
  Waiting and only while the bubble's window is shown; Idle and Paid run
  no loop. The pulse and the chase share the bubble's one
  `rememberInfiniteTransition` (§10 rule 7).
- The docked bubble above the panel chases too (it is the same bubble,
  Waiting); it does not pulse, as today.
- Interruption: a paid that lands mid-turn stops the chase where it is
  and fades the arc as the track darkens; a sale that ends mid-spread
  retargets each ring's colour from where it is (an `Animatable<Color>`
  per ring, or one `Transition` of the look with per-ring specs).
- Reduced motion: as the table's last row.

**TalkBack.** The bubble's window carries a label (today `BubbleFace`
clears its semantics), and it carries the step the bubble no longer
shows:

| Bubble | Says |
| --- | --- |
| Waiting | "Till, $12.00 waiting for payment", then "Till, $12.00 seen on the network", "Till, $12.00 confirming", as the step changes |
| Paid | "Till, $12.00 paid" |
| Idle | "Till, nothing waiting" |

**Tests.** Waiting draws the arc, Idle and Paid draw none (a pixel test
at three points of the turn); Paid draws the green core, then the rim,
then the tick, on the test clock; no recomposition across 10 frames of
the chase (as `OverlayRecompositionsTest`); `Still`: Waiting draws the
whole ink border and runs no loop; the bubble's label carries the step.

Before: `before-r2/bubble-states.mp4` and `bubble-states-close.mp4` (a
crop round the bubble, 2×): the grey idle bubble; a sale arriving, the
lit bubble with the loading mark's glints and the pulse; paid, the
green ring closing round a tick on `paidGround`; after the dwell, idle.

### 12.6 Where the canvas and the code differ, and ambiguities

Each is a call the designer made; the tech lead may overrule, in a
decision.

1. **"A third" of the border vs 30 %.** The owner's words say a black
   third chases; the canvas measures the arc at 30 % (34 of 113.1 u) on
   every artboard. The canvas is the approved design: 30 %.
2. **The lit face's size.** The lit bubble draws `LoadingMark` at 40 dp
   today (#18) and the idle `Logo` at 38 dp; the canvas's geometry says
   one 38 dp logo. The canvas states the change (the lit face is the
   same `mark()` as the others), so 38 dp in every state.
3. **The paid ground.** Today the paid bubble's ground fades to
   `paidGround` (#DCEFE2) with a green tick; the canvas's colour table
   has the ground white in every state and the green in the lens. The
   canvas states the change: the ground stays white, and `paidGround`
   leaves the bubble.
4. **The shift's own follow.** The canvas says Back never follows and
   the tray holds what arrived meanwhile; it says nothing about a sale
   arriving while the shift itself is on screen, which rises by itself
   today (M7, decisions 198 and 222, the till's reason to exist). The
   code's behaviour stands there: §12.1 narrows `follow` to that case.
   The tray's "a new sale arrives" motion row therefore covers a sale
   landing in the tray under a running move or while the app is away,
   not a sale arriving on a resting shift, which still rises.
5. **The idle border's grey.** #565656 is a literal in `Faces.kt` today
   and the canvas keeps it; it moves into `Fixed` with the two greens
   (colours by role, test-enforced), not a new value.
6. **The tick's start "from 200 ms".** The canvas's "from 200 ms" is the
   core's ring finishing its `fade`; it is written as the tick's spec
   starting where the core's ends, driven by the one `Transition`, never
   a `delay()`.
7. **`TraySpec`'s "Paid: Earlier only".** Read with "No tray: no header,
   no Earlier header": under the Paid filter there is no header at all;
   the list reads as today.
8. **Tray on the Needs action filter, "unfolded".** The canvas says the
   tray is unfolded there; it is read as "the fold does not apply" (every
   live sale is a row, no fold row), not as the fold state being changed.
9. **The bubble's label.** `BubbleFace` clears its semantics today, so a
   TalkBack label is new behaviour, not a change to a value; the canvas's
   strings are taken as written, with `SaleText.amount` for the amount.

### 12.7 Memory: the step is accepted, and is not made worse

Decision 253 named the +4.9 to +5.9 MB PSS step after a trim as the
shift's motion machinery standing from launch (not a session's growth:
+0.5 MB a session), and the owner accepted it on 2026-10-30 (decision
268). The 2 MB rule in §8 and §10 rule 12 is superseded by: round 2's
after-minus-before, measured the same way on the same emulator instance
in one session (`mem-r6.sh`'s flow, trimmed PSS, pairs alternated), is
within the run-to-run noise of that instance, with the Java heap's
object count read as decision 253 read it. No part of this round is
spent on the step; nothing in it may add a standing cost of its own
(the tray is three more keyed items in the list the rows already live
in; the bubble's mark is the same paths with more colours).

### 12.8 Order of work

dev1 (overlay): §12.5 whole, with the two presets agreed with the tech
lead first (`chase`, `spread`), the `Fixed` colours, `mark()`'s new
parameters, the bubble's label; then the lint warnings in
`overlay/Faces.kt`.

dev2 (screens): §12.1 (`follow`, `StageBack`), §12.3 (the tray), §12.2
(the step-back), §12.4 a and b; `RunningMovesTest.aSaleRisesDark`'s
guard on `HeldMove` generations.

Tech lead: the presets, `TillStage`'s two `snapshotFlow`s on one
predicate into one, the measurements of §12.7, decisions 269 on, the
integration.

## 13. Round 2 build

The tech lead's architecture for §12, written before the tracks start
and binding on both; decisions 269 to 271 in `docs/decisions/android.md`
record the calls. The foundation this section names as "here" is
committed on `motion-main` with this section, `./gradlew build` green,
and both track branches start from it.

### 13.1 The shape of the round

Two tracks, no shared files between them:

| Track | Owns this round | Touches nothing in |
| --- | --- | --- |
| Overlay (dev1) | `overlay/Faces.kt`, `ui/Mark.kt`, `TillService.kt`, `overlay/*`, their tests (`BubbleOnDeviceTest`, `OverlayFramesTest`, `OverlayRecompositionsTest`, `PanelTest`) | `ui/Shift.kt`, `Nav.kt`, `AppViewModel.kt`, `ui/Motion.kt`, `ui/Theme.kt` |
| Screens (dev2) | `ui/Shift.kt`, `ShiftList.kt`, `SaleText.kt` (`TillNav`), `AppViewModel.kt`, `MainActivity.kt`, `Nav.kt`, `ui/Till.kt`, `ui/SaleDetail.kt`, their tests (`RunningMoves*`, `StageBackTest`, `ShiftListTest`, `ShiftRowsTest`, `MotionFramesTest`, `ModelStage.kt`) | `overlay/*`, `ui/Mark.kt`, `ui/Motion.kt`, `ui/Theme.kt` |
| Tech lead | `ui/Motion.kt`, `ui/Theme.kt` (`Fixed`), the docs, the measurements, the integration | |

A change a track needs in the lead's files is asked for in its PR
description, not made. `ui/Mark.kt` moves from "shared, dev1's" to
dev1's outright: the only other callers (`Logo`, `LoadingMark` in
`Onboarding.kt`) keep their signatures.

### 13.2 The foundation, here

- `Motion.chase`: a loop beside `pulse`, `tween(CHASE_MS = 1250,
  LinearEasing)` repeating, null under `Still`.
- `Motion.spread`: a `List<Preset>` of `SPREAD_RINGS = 4`, ring `i` a
  `fade` with `delayMillis = i × SPREAD_STEP_MS` (60), so the rim is done
  at 380 ms; under `Still` every ring is `fadeQuick` with no delay. Rings
  are the mark's: 0 the core, 1 the inner six, 2 the middle twelve with
  the base circle, 3 the rim twelve, in `MarkPaths.facets`'s order (the
  list is 1 + 6 + 12 + 12).
- `Motion.tick`: `sweep` held at its start for `fade`'s length (`After`,
  a `Preset` whose spec plays the spring from a later start; the one way
  a spring carries a start, as a tween carries `delayMillis`), `snap()`
  under `Still`.
- `Fixed.PaidLight` #2EA052, `Fixed.PaidMid` #238A43 (the lens's two
  lighter greens; `Fixed.Paid` is the core), `Fixed.ChaseTrack` #D8D2C7,
  `Fixed.IdleBorder` #565656 (out of `Faces.kt`). The mark's orange and
  grey shades stay `Mark.kt`'s own lists, as the SVG's; the green list is
  built there from these three.
- Tests: `MotionTest` (the chase's turn and constant speed; the spread's
  four rings, steps and `Still` form; the tick's wait, its identity with
  the sweep after it, no overshoot, `Still` snap) and `BubbleColoursTest`
  (the white tick ≥ 3:1 on every green, ink ≥ 3:1 on the chase track,
  the greens darkening toward the core).

### 13.3 The tray: items in the one list

- `ShiftList.sections(sales, filter, folded): List<ShiftItem>` is a pure
  function beside `rows`, tested on the JVM (`ShiftListTest`): `ShiftItem.
  TrayHeader(count)`, `ShiftItem.Row(sale, ends, tray: Boolean)`,
  `ShiftItem.Fold(hidden: Int, open: Boolean)`, `ShiftItem.Earlier`, in
  list order. Live sales (`Standing.Live`) newest first in the tray, the
  rest newest first under Earlier; with no live sale under the filter,
  no header, no fold, no Earlier (the list is `rows` as today). Folded
  with more than three live sales: three rows, then the fold with
  `hidden = count − 3`. `NeedsAction` shows every live row, no fold item
  (§12.6 item 8). `Paid` has no live rows, so no headers (item 7).
- `ShiftScreen` puts the items in the existing `LazyColumn` with
  `items(sections, key = …)`: keys `"tray"`, `"fold"`, `"earlier"` and
  `sale.orderId` for rows, so a row that leaves the tray for Earlier is
  the same keyed item and `animateItem` moves it (M11's three specs,
  nothing new). The tray has no node of its own: each tray item draws its
  slice of the amber ground behind itself (`drawBehind`, `warnGround`,
  24 dp corners on the tray's top for the header and bottom for the fold
  row, or for the last row when there is no fold; 4 dp padding inside),
  and the row's white card stays the `sharedGround` it is today with
  `rowLook(ends)` (20 dp = 24 − 4, concentric). Tray rows' `ends` are
  counted within the tray, Earlier's within Earlier.
- The fold is `rememberSaveable { mutableStateOf(true) }` in
  `ShiftScreen` (folded), beside `filter`. The fold row is a `Role.Button`
  with `stateDescription` collapsed/expanded, 48 dp, label `ink`, a 16 dp
  chevron turned by `graphicsLayer { rotationZ }` on `settle`; the label
  crossfades on `fade` (`AnimatedContent`, as `RowBadge` does). Folding
  with the fold row about to leave the screen: `LazyListState.
  requestScrollToItem` on the fold's index, so it stays under the finger.
- Header words are the shift's chrome (`words(through = ShiftThrough)`),
  so they fade through a Back like the rest of the chrome; the amber
  slices and the rows' cards are not chrome: they are in place under the
  arriving card by 220 ms (`BackSequence`), as the rows' grounds are
  today.
- Back into a folded row: the shift reads the stage once per change, at
  composition, not per frame: `LocalStage.current?.transition?.
  currentState as? Screen.Sale` is the sale a running `RowGrow Sale →
  Till` is leaving. If that sale would be hidden by the fold, a
  `LaunchedEffect` keyed on its id sets `folded = false` as the change
  starts, so its row exists for the ground to land in; it stays open
  until "Show fewer" or the shift is left. The tray opens at the start of
  the Back, not on landing: a row must be there for the card to shrink
  into.
- Focus after Back: the row the cashier came from asks for focus when
  the stage lands on the shift (`snapshotFlow` on the transition's
  `currentState`, the one flow the shift adds, running only while a
  change does); `FocusRequester` per row is not kept: one requester on
  the arriving row, found by its key.

### 13.4 Back and the step-back: one path

- `AppViewModel.leave(orderId)` is the one path into the stage for both:
  `back()` on a sale calls it (today), and the dwell's job calls it
  (today). Nothing new is added between the model and the stage; what
  changes is `follow`.
- `TillNav.follow(Screen.Till, …)` narrows to "a sale that arrived while
  the shift was showing, at rest": it takes the live ids the till knew at
  its last look (`AppViewModel.liveBefore`, kept today) and whether the
  stage rests on the shift, and brings up only a live sale that is new
  since that look, not left, and only while the shift is at rest. A sale
  that arrives on Settings, on another sale, on the enlarged code or the
  order page, or mid-Back, is known by the next look and never rises
  later: it is in the tray. `leave` and `openShift` make no `follow`
  call after `go(Screen.Till)` (the `follow(graph.runner.state.value.
  till.sales)` line in `leave` goes); `closeSettings` and `backTarget`
  go to `beforeSettings` itself, no follow (what arrived meanwhile
  waits in the tray). The other arms of `follow` (the code screen and
  the order page giving way once the sale is not live) are unchanged.
- "At rest" is the stage's word, reported to the model as the panel's
  collapse is reported to the bubble (decision 258's mechanism, kept for
  this one reason): `FollowScreen` gets `onLanded: (Screen) -> Unit`,
  called when `state.currentState == state.targetState == screen.value`;
  the model keeps `restingOn: Screen?` (set there, cleared in `go`), and
  `follow` rises a new sale only while `restingOn == Screen.Till`. So a
  sale that arrives mid-Back lands in the tray under the moving card
  and does not take the move over (§12.1), while a sale arriving on the
  resting shift still rises (§12.6 item 4). `TestModel` gets the same
  `landed` so `RunningMovesTest` drives it.
- The second Back (260 c): `MainActivity` enables `StageBack` while
  `screen == Screen.Till && stage.currentState != stage.targetState`
  (a `derivedStateOf`, two recompositions of the root per change, none
  per frame); `model.back(Till)` stays the no-op it is. `ModelStage.kt`
  mirrors it so `RunningMovesTest` sees the same handler.
- The paid step-back (259 → §12.2): the move is already `RowGrow Sale →
  Till`; what is missing is the course `sale/<id>` in the trace, which a
  Back on the same page carries. Find the difference with the stage
  trace (`StageTrace`, decision 237) before changing anything: the
  suspects are the paid page's `End` (its `sharedGround` is registered
  under `Move.RowGrow` on the live face's `Box`; the paid face is
  `SaleDetail` inside the `AnimatedContent`, so check which `End`s the
  course has at the dwell's end) and `keptWhileLeaving`. The fix gives
  the dwell's leave the same two ends the Back has; no second mechanism.
- 260 a: the row's ground holding the whole screen with no code is the
  frame on which `paintCodes` has no partner for the code's white (the
  `End.white` arm) while the ground still holds the card's rest margin;
  the rule (decision 234, 189) is drawn whole until the ground no longer
  holds the margin. 260 b: the paid fold's tick alpha is 0 until the last
  `crossfadeOnTrack` share, as `CodeScreen`'s is (decision 252): same
  `faceSurface` arm, read per draw.

### 13.5 The bubble: one mark, read in draw

- `ui/Mark.kt`: `mark()` grows to `mark(lens: LensLook, border: Color,
  arcStart: Float?, arcAlpha: Float, tick: Float)` where `LensLook` is
  four `Color`s, one per ring, read per draw (`Color` is an inline
  value: no allocation). `MarkPaths` gains `ring` per facet and the
  tick's `Path` (`M23.5 28.5 l4.3 4.3 8.7 -9`), measured once by a
  `PathMeasure` kept beside it; the trimmed segment is drawn into one
  reused `Path` (`rewind()` then `getSegment`), the only mutable object,
  allocated once. The arc is one `drawArc(sweep = 0.30 × 360°, start =
  −90° + arcStart, round caps, 4.5 u stroke)` over the track circle. The
  glint and core-alpha code stays for `LoadingMark`.
- `overlay/Faces.kt` `Circle`: `AnimatedContent(look)` is replaced by
  `updateTransition(look)` with: `animateFloat` alpha (0.55 / 1,
  `fade`); `animateColor` border track (`IdleBorder` / `ChaseTrack` /
  `Ink`, `fade`); four `animateColor` ring colours, ring `i`'s
  `transitionSpec = { m.spread[i].spec() }` targeting the look's shade
  for its ring (grey / orange / green lists); `animateFloat` arc alpha
  (1 in Lit, else 0, `fade`); `animateFloat` tick trim
  (`transitionSpec = { if (targetState == Paid) m.tick.fraction else
  m.untick.fraction }`, 1 on Paid) and tick alpha (`fadeQuick`, 1 on
  Paid), so the tick draws in and fades out (decision 293). The chase angle is
  `rememberInfiniteTransition` → `animateFloat(0f, 360f, m.chase)` only
  while `look == Lit && m.chase != null`; it is the same infinite
  transition the pulse uses (one `rememberInfiniteTransition` per
  bubble, §10 rule 7); Idle and Paid run no loop. Everything is read in
  `Canvas`'s draw; nothing above the draw recomposes per frame
  (`OverlayRecompositionsTest` adds `BubbleFace` across ten frames of
  the chase and of the spread).
- `PhaseRing`, the pupil, `LoadingMark`, `LightTokens.paid`/`paidGround`
  and `MkIcons.Tick` leave `Circle`; `BubblePadding`, the halo, the
  layer's alpha and shadow, `Confirm(look)` and the docking are
  unchanged. `BubbleFace` sets `contentDescription` from the look, the
  sale's phase and `SaleText.amount` (§12.5's strings) in place of
  `clearAndSetSemantics {}`; the overlay passes the sale it already has.
- `Still`: `m.chase == null` draws the whole border in `Ink` when Lit
  (the arc is not drawn; the track colour is `Ink` in Lit under Still,
  a `when` on `m.still` at the colour's choice, not a second code path).

### 13.6 Tests and checks per track

Both: `./gradlew build` green (lint included: the four
`ModifierParameter` warnings in `Faces.kt` go in dev1's first commit);
`connectedDebugAndroidTest` on the track's own emulator, every test
green, before the PR; no `delay()`, `Handler`, reflection or
`LaunchedEffect` frame loop in the diff; presets only, colours only via
roles or `Fixed`; the QR code whole on every frame it is drawn
(`RunningMovesTest`'s frame checks stay green).

Overlay: `OverlayFramesTest` pixel tests at three points of the turn
(Lit draws the arc, Idle and Paid none), Paid's core green before its
rim before the tick on the test clock, `Still` whole ink border and no
loop (`m.chase` null means no infinite transition is created: count via
`Recompositions` or a `rememberInfiniteTransition` label), the label
carrying the step; `OverlayRecompositionsTest` ten frames of the chase.

Screens: `ShiftListTest` for `sections` (fold at 4 and 21, no fold at
2, `NeedsAction` unfolded, `Paid` headerless, order); `ShiftRowsTest` or
a new `TrayTest` for the fold row's label, state and the count's
semantics; `RunningMovesTest`: Back with a waiting unseen sale lands on
the shift with the ground in the left sale's tray row; Back into a
folded row opens the tray first; the step-back's trace carries the
course and lands in the row at the top of Earlier; the code drawn on
every frame of a Back and a close-for-now until the ground leaves the
card (260 a); the paid fold's tick alpha 0 until the last share (260 b);
`StageBackTest`: a second Back while the shift is arriving lands on the
shift; a slow-frame `aSaleRises*` (a 100 ms+ `mainClock` advance mid-
rise) with the `HeldMove` guard as the regression (§12.8).

Memory (§12.7): the lead measures after integration on 5558 with
`mem-r6.sh`'s flow; the tracks add no standing cost (the tray is three
keyed items in the list the rows already live in; the bubble's mark is
the same paths with four more colours and one small `Path`).

### 13.7 Order

dev1 and dev2 start from `motion-main` at this commit, on
`motion-main-dev1` and `motion-main-dev2`, each with a draft PR against
`motion-main`. The lead integrates dev1 first (its diff is contained),
then dev2, re-runs both suites on 5558 and 5560, measures §12.7, and
writes the showcase clips under `docs/reports/motion/` beside the
`before-r2/` ones.
