# The iPad redesign (November 2026)

Status: designs accepted 2026-11-09. This page is what the build follows.

The owner's first real iPad test (TestFlight, iPadOS 26.7, 2026-11-09)
showed that arranging Till and Square side by side is too awkward, and
turned up a list of smaller problems. The answer was designed on the canvas
[Till iPad feedback proposals](https://claude.ai/artifact/HLJ95G34TRawP8N99SeGfZ).
Its boards are copied into [designs/ipad-redesign/](designs/ipad-redesign/)
(`.dc.html` files, open as HTML, laid out by `canvas.json`). Where this page
and a board disagree, this page wins. A design review against Apple's Human
Interface Guidelines is in
[reports/2026-11-09-apple-hig-review.md](reports/2026-11-09-apple-hig-review.md).
Its token plan was partly overruled by the owner; see "Look" below.

The board names below (A1, P2, O3 …) are the canvas's.

## Decisions

### How a sale reaches the cashier on iPad

1. **Alert first is the default** (A1–A3). Square stays full screen. When
   this till's sale is found, Till posts a time-sensitive notification:
   "Sam · $24.50", body "Monero sale. Tap to show Sam the code." Tapping it
   opens Till full screen on that sale, with the code ready to turn towards
   the customer (A2). When the sale is paid, Till shows Paid with a **Back to
   Square** button (A3), which opens `square-commerce-v1://`. The owner
   checked on an iPad that this URL opens Square Point of Sale. From the app
   there is no confirmation prompt; Safari's "Open in Square?" prompt is
   Safari's own. Add `square-commerce-v1` to `LSApplicationQueriesSchemes`
   and show the button only when `canOpenURL` says Square is installed. Till
   then returns to the shift by itself, so the next alert opens fresh. The
   engine offers it on iPhone too (core.md decision 141). There is no Back to
   Clover: Clover runs on its own device, with Till on a phone or iPad
   beside it (core.md decision 142).
2. **Picture in Picture is the second mode** (P1–P4), chosen in onboarding
   (O4) or Settings. iOS draws the window; Till supplies its picture through
   `AVPictureInPictureController` with an `AVSampleBufferDisplayLayer`
   content source, rendering frames of its own. Till has no buttons of its
   own in the window: iOS shows ✕ (top left), ⤢ (top right) and playback
   controls on a tap, so those corners stay empty.
   - Watching: a short, wide strip (about 3:1) with the Till logo (idle,
     dark) and "Watching / Front Counter".
   - A sale: Till switches the frames to 1:1 and iOS reshapes the window into
     a square with the customer's name, the code (with its receipt, as the
     window is drawn on the device: core.md decision 140) and the amount with
     time left. Below about 200 pt the code isn't drawn (it can't be scanned);
     the amount and "Tap ⤢ to show the customer" show instead.
   - Paid: back to the strip with the green Till logo and "Paid · $24.50 /
     Sam", then after a few seconds back to Watching.
   - ⤢ opens Till full screen (A2). It is offered as an opt-in setting
     because App Review expects Picture in Picture for video.
3. **Side by side** stays as the third choice ("Till and Square in two
   windows, arranged by hand"). The old Slide Over onboarding step is
   replaced by O4.

### Counters (device-based channelling)

4. Square names the device that took each payment
   (`device_details.device_id`, `device_name`). The onboarding test sale is
   rung up on this counter's Square, so Till learns this counter's device
   from it (C4) and asks "Only sales on Front iPad (recommended)" or "Every
   sale at Front Counter". `Till::with_device_id`
   (`crates/till-sales/src/till.rs`) already filters by device but nothing
   calls it yet.
5. Sales from other devices never alert, start a Live Activity or grow the
   PiP window. They stay visible: the **title menu** (C3) is the chosen way,
   on iPad and iPhone. Its title is the device being shown ("Front iPad ▾");
   the menu lists this counter (with "This counter · alerts"), each other
   device with its waiting count, and "All counters". Opening another
   counter's sale shows its code, without the receipt, which stays on the
   till that found it.
6. Settings (C5): "Alert for: This device: Front iPad / Every device at Front
   Counter", "Find this device again with a test sale", and "Show other
   counters' sales in the list". If the test was skipped, Till takes every
   device's sales and Settings offers the test sale. A Clover counter gets
   the same, its devices named from Clover's device list (core.md decision
   142).

### Cash sales

7. **By default a Cash sale in Square is treated as Monero** (owner,
   2026-11-09): it alerts, gets a code, grows the PiP window, and follows the
   same flow as Other: Monero. The sale's page offers **Paid in cash**,
   which closes it with no refund (Square already recorded Cash) when the
   customer pays real cash after all.
8. Settings (K4) can turn that off ("Only offer Monero"). Then cash sales show
   quietly with **Take it in Monero** and **Keep as cash** (K3). In the PiP
   strip (K1) the take action is the system play control, which Till treats
   as "take it in Monero". This is a review risk; ⤢ is the fallback.
9. Square's books say Cash for a sale paid in Monero. Till's shift lists which
   cash sales were paid in Monero, for the end-of-day count.

### Customer names

10. The customer's name is how staff tell orders apart, so it leads every row,
    the sale's title (28 pt on iPad), the alert, the PiP window and Paid. The
    sale number moves to the second line; with no name the row shows "Sale
    #m2Qe". Till reads Square's fulfillment recipient today; add the order's
    `ticket_name` (typed at the counter). The attached customer would need
    Square's customer-read scope and is not part of this work. Names never go
    to Monokulo.

### Onboarding (O1–O4, C4)

11. No "Step N of 6" anywhere on iPad: the rail shows the step. On iPhone a
    thin progress bar replaces the text, and Welcome has none.
12. Choosing Square or Clover is its own step (O1), so signing in (O2) and
    the test sale (O3) fit without scrolling. "Not now: only show other
    tills' sales" becomes **Set up later**, with the footnote "Without a
    point of sale, this iPad can't find new sales itself. It still shows
    sales that another till finds."
13. O3 pins the test-sale status in a bottom bar ("Waiting for the test
    sale… / Continue unlocks when it arrives") beside Skip the test and the
    disabled Continue. While the store is read, Continue shows the progress
    itself.
14. After the test sale, C4 names the device; then O4 asks how a sale shows
    up: An alert (recommended) / A floating window / Side by side, with
    "Send a test alert".

### Screens

15. **Empty till** (E1, E2): one centred block: the Till logo (idle), "Ready
    for the next sale", a line on how a sale appears, and status chips
    (Watching Square · Alerts on · Instant with the Bell). Later in the shift
    the sales list sits beside it, with the shift's total.
16. **iPad full screen** (D1): a split view (`NavigationSplitView`), sales on
    the left and the live sale on the right. **iPhone** (D2): a
    `NavigationStack` with swipe back.
17. **A live sale**: the code on a plain white card; under it the system's
    thin `ProgressView` (tint #E85D00) and a label ("Seen on the network · 1
    of 3"); steps follow the Till widget: waiting, seen, confirming, paid.
    Buttons: **Customer view** (with a turn-around icon; opens the
    full-screen code) and **Share** (`ShareLink` of the `monero:` URI, which
    replaces Copy address). Close the sale stays.
18. **Paid** (D3): the Till logo, green with its tick, "Paid · $13.88", "Sam ·
    Sale #m2Qe", and **Done**. The one motion moment: the bar fills, the logo
    turns green, a success haptic; a crossfade under Reduce Motion.
19. **Settings** (D4, C5, K4): an inset grouped list. Alerts are a status row
    ("Off", warn colour) plus "Open Notification Settings", never a switch
    that snaps back. The wallet row shares the address.

### Look

20. Keep the palette in `ios/Till/Look/Palette.swift` (paper #F4F1EA, ink
    #1A1917, muted #5A564C, Monero orange #FF6600 with #111111 labels, link
    #B34700). Changes: orange fills only the primary button; progress uses
    #E85D00; green #1A7F37 means Paid and nothing else ("Seen on the network"
    is no longer green); dark mode's ground becomes #000000 with cards
    #1C1B19. Manrope only on amounts and the Welcome and Ready titles; system
    type everywhere else.
21. **The Till logo** is the terminal widget from the owner's
    [Monokulo Branding canvas](https://claude.ai/artifact/WSqsTMBfoKDwT979sYGcS6)
    (`TillWidget` and `TillRound` there; copies in
    [designs/ipad-redesign/](designs/ipad-redesign/)). It is the hero on the
    empty till, Paid, and the PiP Watching and Paid strips, and it's the app
    and notification icon. Small status marks in lists and onboarding use
    simple SF Symbol-weight glyphs beside a word (`StatusGlyph`): watching,
    waiting, seen, paid, closed, cash. No invented decorative graphics.
22. Names and status lines never wrap: one line, ending in an ellipsis.
23. Nothing of Till's sits under what iOS draws on top: iPadOS window
    controls at the top left, PiP's ✕ and ⤢, the Dynamic Island, the home
    indicator. Titles go in the system toolbar.
24. Pages scroll across their whole width: the width cap goes on the
    content inside the scroll view (`.contentMargins`), and layout follows
    the size class, not the device.

### Also from the test

25. The scanner's camera preview follows rotation
    (`AVCaptureDevice.RotationCoordinator`).
26. The two accessibility failures in the review: the Ready step tells done
    from not-done by more than colour, and progress tracks and arcs meet 3:1.

## Android

The engine changes (counters, cash, names, onboarding steps and copy) are
shared, so Android must keep building and behaving correctly on them. Its
own redesign is not part of this work; where a new engine field needs
drawing, Android gets the plain version (a row line, a settings row).

## Build order

1. The core: device channelling, cash, names, onboarding steps and copy,
   tones, notification text (crates/, with Android kept compiling).
2. The iOS look: tokens, the Till logo and glyphs, navigation, scrolling,
   the empty till, the sale and Paid, Settings rows, Share, camera rotation.
3. The iOS features on the new core: onboarding O1–O4 and C4, the counters
   menu, cash, alert first with Back to Square, then Picture in Picture.
