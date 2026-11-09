# Design review: Till (iOS / iPadOS, SwiftUI)

Read-only review in the Apple design skill's design improvement mode. Paths are relative to `/home/henry/Downloads/monokular`. HIG pages I opened (under `scratchpad/apple-skill/references/hig/`): accessibility, designing-for-ios, designing-for-ipados, layout, typography, color, buttons, liquid-glass, multitasking, onboarding, activity-views, playing-video, notifications, live-activities, lists-and-tables, settings, dark-mode, writing, feedback, branding. Every citation below comes from one of those pages. Anything else is marked as judgment.

**Context.** Till is a companion to a self-hosted Monero payment server. The cashier rings up a sale in Square or Clover, Till sees it, shows the customer a QR payment code and follows the payment until it is confirmed. iPhone uses a Live Activity; iPad currently runs in Slide Over beside Square. The app is SwiftUI, iOS/iPadOS 26.7, and it renders views that the Rust core (`crates/till-screens`) describes, including every string. What I reviewed: the code, the core's copy and the design canvas boards. What I couldn't check: what the app looks like on a device, timing, and how the camera behaves. Contrast figures are WCAG 2.x, computed from the hex values in `ios/Till/Look/Palette.swift`.

**Thesis.** One job: *get the customer's money in, and make it impossible for the cashier to hand goods over too early.* The most characteristic element is already in the code but goes unused: the confirmation ring (`RingGlyph`, `ios/Till/TillScreens/TillParts.swift:115`), the payment visibly closing toward Paid.

---

### Summary

The groundwork is careful. The palette is faithful to the canvas (every hex matches), contrast is mostly well above minimums in both appearances, Dynamic Type scales even the Manrope display type, reduce-motion is honoured, the customer code forces light mode and full brightness, and VoiceOver labels are thoughtful. The problems are structural. The app was laid out for a 320–375 pt Slide Over panel, so on a full iPad it is a 560/640 pt column that only scrolls in the middle. It chooses layout by device idiom. It has no system navigation, so there's no swipe back. Onboarding teaches iPadOS window management that iPadOS 26 has changed. It also uses green for "seen" before "paid". Visually it sits close to the cream-plus-accent template. Only Monero orange is truly rooted in the product. **Overall rating: Critical issues** (two accessibility failures, both easy to fix), otherwise **Needs work**.

---

### Critical

**C1. Done and to-do states are told apart only by a 1.3–1.5:1 colour change.**
- **What**: Ready's checklist draws the same `checkmark.circle.fill` for done and not done, coloured paid green or `border` #DCD5C7, and hides it from VoiceOver (`ios/Till/Onboarding/OnboardingPages.swift:688-690`). On a white card #DCD5C7 is **1.46:1**. A "not connected" row looks like a faint tick. The step rail's to-do ring is #DCD5C7 on cream at **1.29:1** (`ios/Till/Onboarding/OnboardingScreen.swift:123`), though its number text (#5A564C, 6.49:1) saves it there.
- **Why**: `accessibility.md › Vision`: "Offer visual indicators, like distinct shapes or icons, in addition to color to help people perceive differences in function and changes in state."
- **Fix**: use `check.done ? "checkmark.circle.fill" : "circle.dashed"` (or `exclamationmark.circle` in `Palette.expiredInk`, 5.36:1) and stop hiding it from VoiceOver. `.accessibilityValue(check.done ? "Done" : "Not set up")`.

**C2. Payment progress graphics are below 3:1.**
- **What**: `StepBar` on the customer screen fills with #FF6600 on white (**2.94:1**) over a track of #E4DFD5 (**1.33:1**) (`ios/Till/TillScreens/SaleScreen.swift:296`). `RingGlyph` draws its track in `border` (#DCD5C7, **1.46:1** on a white card) and its arc in #FF6600 (**2.94:1**) (`TillParts.swift:122-125`). That ring is the only "how far along" cue in a shift row.
- **Why**: `accessibility.md › Vision`: "it's important that there's enough contrast between foreground text and icons and background colors". The 3:1 figure for non-text graphics is WCAG 1.4.11 (my addition, not in the HIG table).
- **Fix**: in light mode, draw arcs in #E85D00, the mark's own second facet shade (**3.50:1** on white, 3.10:1 on cream), and tracks in #8C8577 (**3.66:1** on white). Keep #FF6600 in dark mode (5.86:1 on #1C1B19). Under `@Environment(\.colorSchemeContrast) == .increased`, use #C24E00 (4.79:1).

---

### Improvements

**I1. High: the iPad layout is a phone column with dead margins.** This confirms the owner's note that pages only scroll from the centre column.
- **What**: `TillScreen` wraps every screen in `.frame(maxWidth: 560)` *outside* each screen's `ScrollView`/`List` (`TillScreen.swift:27-28`, `TillParts.swift:7`). Onboarding does the same at 640 pt (`OnboardingScreen.swift:16-17, 28-29`). On a 1180 pt landscape iPad, about 540 pt (or 240 pt beside the 300 pt rail) of margin ignores drags. Layout is also chosen by `UIDevice.current.userInterfaceIdiom` (`TillScreen.swift:18`, `OnboardingPages.swift:152`).
- **Why**: `layout.md › Size classes`: "Determine layout based on size classes, not device type or orientation." `designing-for-ipados.md › Best practices`: "Take advantage of the large display to elevate the content people care about".
- **Fix**: let scroll views fill the window and limit the *content* instead: `ScrollView { … }.contentMargins(.horizontal, max(16, (w - 560) / 2), for: .scrollContent)`, and the same on `List`. With the alert-first decision the iPad till is full screen, so at `horizontalSizeClass == .regular` use a `NavigationSplitView`: the shift in the sidebar, the sale in the detail (see the wireframe below). Replace the `phone` flag with the size class plus `isDynamicIslandLimitedInWidth` where that matters.

**I2. High: no system navigation, so no swipe back.**
- **What**: screens swap on `core.route` (`TillScreen.swift:61-104`). Back is a custom glass chevron in a hand-built `TillBar` (`TillParts.swift:11-55`, `BackButton` at `:72-85`), and onboarding's Back works the same way (`OnboardingParts.swift:26-40`). Without a `NavigationStack` path there is no edge swipe back and no scroll-edge effect under the bar.
- **Why**: `designing-for-ios.md › Best practices`: "it's especially important to let people swipe to navigate back or initiate actions in a list row."
- **Fix**: drive a `NavigationStack(path:)` from the engine's route (map `Route` to `[Route]`), use `.navigationTitle` / `.navigationSubtitle` for title and line, and put Close in `ToolbarItem(placement: .confirmationAction)` with `role: .destructive`. Report a back-swipe as `.back` via `onChange(of: path)`. Settings already shows the pattern (`SettingsScreen.swift:20-68`).

**I3. High: green means "seen" as well as "paid".**
- **What**: a sale seen in the mempool but not confirmed gets `Tone::Paid` (`crates/till-screens/src/shift.rs:366`). That is the same green as Paid, and a busy cashier reads it as "hand it over". Waiting rows use `Tone::Link` (#B34700), the colour of tappable links (`shift.rs:368`).
- **Why**: `color.md › Best practices`: "Avoid using the same color to mean different things… if you use your brand color to indicate that a borderless button is interactive, using the same or similar color to stylize noninteractive text is confusing."
- **Fix**: Seen and Confirming use `Tone::Plain` with the ring part-filled. Green is reserved for Paid only. Waiting uses `Tone::Muted` plus its countdown. Link brown is used only on things you can tap.

**I4. High: onboarding teaches the system, and the Slide Over step is out of date for iPadOS 26.** This confirms the owner's note that the Slide Over instructions are wrong.
- **What**: step 6 on iPad is "Keep Till next to Square / Put Till in Slide Over…" (`onboarding.rs:639-650`), and the rail labels it "Slide Over" (`onboarding.rs:692`). iPadOS 26 rebuilt multitasking around resizable windows, and the app can't see or control any of it.
- **Why**: `onboarding.md › Best practices`: "they don't need to learn how to use the system or the device." `multitasking.md › Tablet`: "Apps don't control multitasking configurations or receive any indication of the ones that people choose."
- **Fix**: replace step 6 with the decided alert-first flow, "Till taps you on the shoulder". Show a live sample of the time-sensitive alert, a "Send a test alert" button, and the Picture in Picture preview. Rename the rail step "Alerts & floating code", or merge it into step 5.

**I5. High: the point-of-sale step buries what matters.** This confirms the owner's note that important content is pushed off screen.
- **What**: once a POS is chosen, the page stacks: the choice card (2 rows × 56 pt), logo-link-logo, a 24 pt heading and lead, the account card, a permissions footnote, a section label, the steps card, the test-sale status and the Clover footnote (`OnboardingPages.swift:318-342, 389-486`). Below that sits a pinned bottom with up to two buttons and a step label (about 140 pt). The test-sale status, the thing the cashier is waiting on, starts roughly 800–900 pt down. On a landscape iPad that is about 1.5 screens.
- **Why**: `layout.md › Visual hierarchy`: "Use progressive disclosure to make layouts cleaner and easier to interact with."
- **Fix**: after choosing, collapse the choice card into one `LabeledContent("Point of sale") { Text("Square"); Button("Change") }` row. Move the permissions text into a `DisclosureGroup("What Till can see")`. Drop the decorative logo-link-logo row. Pin the test-sale status *as* the bottom bar, in a `.safeAreaInset(edge: .bottom)` with its LoadingMark, so it is always visible. On a regular-width iPad, use two columns: account on the left, steps and test on the right.

**I6. High: "Continue" stays dimmed with no reason, and the page seems frozen.** This confirms the owner's note.
- **What**: Continue is disabled until the store read finishes (`onboarding.rs:509`). The only sign of progress is a "Reading…" value in a fact row. A disabled button is just 40% opacity (`Buttons.swift:17`). `StepPage` uses `.scrollBounceBehavior(.basedOnSize)` (`OnboardingParts.swift:48`), so a page that fits doesn't move when dragged, and drags in the side margins do nothing (I1). Together that reads as "stuck".
- **Why**: `buttons.md › Mobile`: "Configure a button to display an activity indicator when you need to provide feedback about an action that doesn't instantly complete." `feedback.md › Best practices`: "Show people when a command can't be carried out and help them understand why."
- **Fix**: while reading, label the button "Reading the store…" with a `ProgressView()` inside, kept at full opacity. If the read takes more than 10 s, show "Try again". Remove `.basedOnSize` on onboarding pages.

**I7. Medium: "Step N of 6" duplicates the rail.**
- **What**: the core always sets `step_label` (`onboarding.rs:458-461`), and `StepPage` prints it under the primary button (`OnboardingParts.swift:51-55`). On iPad it repeats the rail beside it, even on Welcome under "Get started".
- **Why**: `writing.md › Getting started`: "Check each word to be sure it needs to be there."
- **Fix**: return `None` when `device == Ipad`. On iPhone, drop it from the button area. A `ProgressView(value: n, total: 6)` under the nav bar is enough, and nothing at all on Welcome.

**I8. Medium: "Not now: only show other tills' sales" assumes a model the cashier doesn't have.**
- **What**: `onboarding.rs:516`. It refers to running without a POS, where Till only shows Monero sales that *other* tills create (`shift.rs:472`). A one-till shop has no "other tills".
- **Why**: `writing.md › Getting started`, "Be clear", and the skill's rule that names come from what people control and recognise.
- **Fix**: label the button "Set up later". Add a footnote: "Without Square, Till can't see sales you ring up here. You can connect it in Settings."

**I9. Medium: "Copy address" should be Share.** This confirms the owner's note.
- **What**: `SaleScreen.swift:81-90` copies the address via the engine. No visible "Copied" confirmation appears anywhere in the Swift or core code (only a possible haptic, `Core.swift:180`).
- **Why**: `activity-views.md › Best practices`: "Use the Share button to display an activity view… Avoid confusing people by providing an alternative way to do the same thing."
- **Fix**: `ShareLink(item: paymentURI, subject: Text(amount), preview: SharePreview("Monero payment \(amount)", image: qrImage))` with `Label("Share", systemImage: "square.and.arrow.up")`. Share the `monero:` URI with the amount, which gives Copy, AirDrop and Messages for free. Core: rename the `copy_address` `Press` to `share`.

**I10. Medium: the camera preview ignores rotation.** This confirms the owner's note.
- **What**: `QRScanner` never sets the preview connection's rotation (`ios/Till/Onboarding/QRScanner.swift:16-24`), so on a rotated iPad the picture is sideways.
- **Why**: `designing-for-ipados.md › Best practices`: "Adapt seamlessly to appearance changes — like device orientation".
- **Fix**: create an `AVCaptureDevice.RotationCoordinator(device: camera, previewLayer: view.previewLayer)` and observe `videoRotationAngleForHorizonLevelPreview` with KVO, setting `previewLayer.connection?.videoRotationAngle`. Keep the coordinator in the `Coordinator`.

**I11. Medium: the empty till is off-centre and bare.** This confirms the owner's note.
- **What**: `IdlePanel` is a row in the `List`, below "This shift / Nothing paid yet" (`ShiftScreen.swift:39-53`), so it sits near the top, not in the middle of the space. It shows a static logo at 35% opacity (`ShiftScreen.swift:132-133`), and "No sale right now" sits right under "Nothing paid yet".
- **Why**: `writing.md › Best practices`: "An empty screen can be daunting if it isn't obvious what to do next, so guide people on actions they can take".
- **Fix**: when `entries.isEmpty`, add `.overlay { ContentUnavailableView { Label { Text("Watching for sales") } icon: { LoadingMark(size: 72) } } description: { Text("Tap Monero in Square and the code appears here.") } actions: { Button("Send a test sale"…) } }`. Use the *animated* mark, which is the honest "watching" signal. Hide the summary row while it is empty.

**I12. Medium: Liquid Glass in the content layer.**
- **What**: `.glassButton()` (`Buttons.swift:33-38`) styles in-content buttons: Paste the link (`OnboardingPages.swift:145`), Copy address (`SaleScreen.swift:88`), Look again (`:457`), Show code again (`SalePageScreen.swift:105`) and Not yet (`Sheets.swift:94`).
- **Why**: `liquid-glass.md › Where the material belongs`: "Don't use Liquid Glass in the content layer."
- **Fix**: use `.buttonStyle(.bordered).controlSize(.large)` for in-content secondary actions. Glass comes for free on real toolbar items once I2 lands.

**I13. Medium: the pinned bottom area overflows at accessibility sizes, and rows truncate.**
- **What**: `StepPage` pins its bottom outside the scroll. On the scan page, that bottom holds a two-line paragraph plus a button (`OnboardingPages.swift:134-145`). At AX5 this can take half the screen. `SaleRow` titles ("Sale #m2Qe · $8.40") are `.lineLimit(1)` (`ShiftScreen.swift:282`), so the amount is the part that gets cut. `FactRow` keeps label and value side by side at every size (`OnboardingParts.swift:204-211`).
- **Why**: `typography.md › Supporting Dynamic Type`: "Keep text truncation to a minimum as font size increases." Also "Consider adjusting your layout at large font sizes."
- **Fix**: move explanatory text into the scroll and keep only buttons in `.safeAreaInset(edge: .bottom)`. Put the amount in its own `Text` with `.layoutPriority(1)`. Wrap `FactRow` in `ViewThatFits { HStack…; VStack(alignment: .leading)… }`, or switch on `dynamicTypeSize.isAccessibilitySize`.

**I14. Medium: tap anywhere shrinks the customer code.**
- **What**: `CodeScreen` puts `.onTapGesture { send(.back) }` on the whole screen (`SaleScreen.swift:180-181`). Customers point at and touch the screen they are scanning, and the iPad layout has no visible shrink button.
- **Why**: my judgment, backed by `accessibility.md › Cognitive`: "Prefer dismissing views with an explicit action."
- **Fix**: a 48 pt shrink button on both layouts, as on iPhone (`SaleScreen.swift:207-216`). Remove the whole-screen tap and the "Tap anywhere to shrink" line.

**I15. Medium: Settings' alerts switch doesn't switch.**
- **What**: the "New sale alerts" `Toggle` snaps back and opens system Settings (`SettingsScreen.swift:28`, `:453-471`).
- **Why**: `settings.md › Best practices`: "Respect people's systemwide settings and avoid including redundant versions of them in your custom settings area."
- **Fix**: `LabeledContent("New sale alerts", value: on ? "On" : "Off")`, plus a `Button("Open Notification Settings")` when off.

**I16. Low: the close sheet has only one exit.**
- **What**: the close sheet can be dismissed only by swiping (`Sheets.swift:8-54`).
- **Fix**: add `ToolbarItem(placement: .cancellationAction) { Button("Cancel") }` in a `NavigationStack`, and `.presentationDragIndicator(.visible)`.

**I17. Low: button text uses sentence case.**
- **What**: buttons such as "Get started", "Copy address" and "Start Till" are sentence case, applied consistently, and Android shares the strings.
- **Why**: `buttons.md › Content` describes title-style capitalization.
- **Fix**: it's a deliberate cross-platform choice. Either keep it and write the decision down, or switch iOS button labels to title case. The bigger issue is drift: "Get started", "Start Till" and "Start watching" all start a shift (`writing.md › Best practices`: "use consistent language throughout processes with multiple steps"). Settle on "Start watching".

---

### Craft notes

- **High: too close to a template, and the boldness is spread thin.** Warm cream + one hot accent + a heavy geometric display face on *every* heading (step pages, sheets, sale pages, Disconnected) is very close to the cream-plus-accent look the skill names. The orange is truly Monero's; the cream is a family carry-over (Android, icon) with no product reason; Manrope 800 at 24–34 pt appears about 12 times. The logo appears on about 8 surfaces (rail, SheetTop 64 pt, the POS card, Ready 72 pt, the shift bar, the empty state, sample alerts, Welcome 148 pt). `branding.md › Best practices`: "Resist the temptation to display your logo throughout your app… it's usually better to use the space to give people valuable information and controls." See the token plan below.
- **Medium: the signature is already in the code and unused.** `RingGlyph` (confirmations filling an arc) is the most "Till" thing in the app. Right now it's 22 pt inside a list badge. Promote it.
- **Medium: brand orange is used for status as well as action.** It fills the primary button, the "needs action" count, the step bar, the rail's current step, the toggle tint and the busy spinner. `branding.md › Best practices`: "Minimize its use on controls and instead use it intentionally for primary actions or status indicators". Keep it for the primary action and the ring. Toggles take the system green; the spinner, ink.
- **Remove one thing:** the logo-link-logo row on the POS card (`OnboardingPages.swift:391-397`). It says nothing.

### What works (keep these)

- The palette is exact to the canvas, and every text pair passes in both appearances: ink 15.57:1 / 14.08:1, secondary 6.49:1 / 6.80:1, link 4.88:1 / 7.34:1, #111111 on orange 6.43:1. Paid green #1A7F37 is exactly 4.50:1 on cream (passes, no headroom).
- The custom primary capsule, rather than `.glassProminent`: the system's white label on #FF6600 would be 2.94:1. #111111 on orange is the correct, accessible answer, and it has a press state (`buttons.md › Best practices`).
- `display()` scales Manrope with `@ScaledMetric` relative to a text style (`Typeface.swift:42-55`). Most custom-font apps get this wrong.
- `LoadingMark` stops under Reduce Motion (`Mark.swift:248-262`). The customer code forces `.light`, sets brightness to 1 and restores it (`SaleScreen.swift:183-185, 274-284`). The refund note is announced when it arrives (`Sheets.swift:97-101`). Rows combine their accessibility elements and badges are hidden.
- Settings uses `.insetGrouped` with a `confirmationDialog` for Disconnect, as `lists-and-tables.md › Style` expects.
- The Live Activity handles light and dark on the lock screen, tints the keyline, uses `Text(timerInterval:)` for the countdown, and keeps the code-in-island option off by default with a clear privacy alert (`SettingsScreen.swift:128-138`), in line with `live-activities.md › Best practices` ("Avoid displaying sensitive information").

### Platform notes

- **iPad, alert-first + Picture in Picture (decided).** Only AVKit can float above other apps. Use `AVPictureInPictureController(contentSource: .init(sampleBufferDisplayLayer:playbackDelegate:))`, render the ring, amount and a small code into `CMSampleBuffer`s, and enable the "Audio, AirPlay, and Picture in Picture" background mode. **Trade-off:** the HIG frames PiP as being for "videos and FaceTime calls" (`multitasking.md › Tablet`), so a non-video PiP risks App Review questions and a small window. Recommendation: PiP is the *cashier's* glanceable status (amount, ring, a 120 pt code at most). The customer-facing code is always Till full screen, one tap from the PiP window or the alert. The alert is a time-sensitive notification. Tapping it opens the sale's code directly, and its actions do work without opening the app ("Share", "Close sale"), per `notifications.md › Notification actions`: "Avoid providing an action that merely opens your app."
- **Live Activity.** `CompactTrailing` prints amounts at a fixed 15 pt with no limited-width variant (`ios/TillWidgets/TillLiveActivity.swift:84-89`). `live-activities.md › Creating Live Activity layouts`: "adapt your compact and minimal presentations to fit inside a 28pt-diameter circle". When `isDynamicIslandLimitedInWidth`, show the ring only. The lock-screen background is custom cream, so check the Always-On reduced-luminance state (`live-activities.md › Choosing colors`). Use `@Environment(\.isLuminanceReduced)` to hide the 84 pt QR and dim the orange.
- **iPhone** is portrait-only (`ios/project.yml:79`). That's acceptable for a till.

---

## Token system (design improvement mode, step 2)

**Grounding.** Product: a self-hosted Monero till, used by a cashier on a counter beside Square or Clover, with a customer leaning in to scan. The materials of the subject: Monero's orange; the receipt (white paper, tabular figures); the confirmations a Monero payment collects; the Monokulo lens mark ("monokulo" is Esperanto for monocle).

### First plan (essentially what ships)

| Role | Light | Dark | Contrast |
|---|---|---|---|
| Paper (ground) | #F4F1EA | #1C1B19 | n/a |
| Ink | #1A1917 | #ECE8DF | 15.57 / 14.08 |
| Muted | #5A564C | #A8A29A | 6.49 / 6.80 |
| Orange (accent fill) | #FF6600, text #111111 | same | 6.43 (text on fill) |
| Link | #B34700 | #FF8A3D | 4.88 / 7.34 |
| Paid | #1A7F37 | #4CC27A | 4.50 / 7.63 |

Type: Manrope 800 for every heading and amount; SF everywhere else. Signature: the logo. Motion: the animated mark.

### Step 3: is it a template?

Yes, mostly. Would I give this same plan to a coffee-subscription app? Yes: cream + one hot accent + a heavy geometric display face on every heading. That is the cream-plus-accent default the skill names, missing only the serif. What is truly Till's: the **orange** (Monero's own), the **ring** (confirmations), the **white code card** (a receipt you scan), and **tabular amounts**. The logo as signature is branding, not product. The plan also gives brown "link" a second job as "waiting", which breaks one-colour-one-meaning (I3).

### Revised plan (what changed and why)

**Colour: six roles.** Changes: dark paper moves to true black so the app is continuous with the Dynamic Island it lives in; "link" stops being a status colour; the ring gets its own accessible orange; Increase Contrast variants are added.

| Token | Role | Light | Dark | Contrast vs its surface |
|---|---|---|---|---|
| `paper` | Ground | #F4F1EA (kept: shared with Android and the icon) | **#000000** (was #1C1B19; matches the island's black) | n/a |
| `receipt` | The code card, the order card, list cards | #FFFFFF | #1C1B19 (code card stays #FFFFFF in both modes) | n/a |
| `ink` / `muted` | Text | #1A1917 / #5A564C (Increase Contrast: #3F3B33) | #ECE8DF / #A8A29A | light on paper 15.57 / 6.49 (IC 9.88); on receipt 17.57 / 7.32. Dark on black 17.17 / 8.30; on #1C1B19 14.08 / 6.80 |
| `monero` | Primary action fill **only**, label #111111 | #FF6600 | #FF6600 | label 6.43:1; fill vs black 7.15 |
| `ring` | Confirmation arc + track | arc #E85D00 (the mark's facet shade), track #8C8577 | arc #FF6600, track #4A4743 | light arc 3.50 on white / 3.10 on paper; track 3.66 on white. Dark arc 7.15 on black. IC arc #C24E00, 4.79 |
| `signal` | Paid / warn / danger, text and glyph | #1A7F37 / #7D5E00 / #B00020 | #4CC27A / #E8C267 / #FF6B6B | light on paper 4.50 / 5.36 / 6.49 (paid IC #146C2E, 5.79). Dark on black 9.31 / 12.3 / 7.57 |

Tappable text uses the system `.tint`, set to #B34700 / #FF8A3D (4.88 / 7.34) and never used for status. Toggles use the system green. Prefer `Color(uiColor:)` dynamic providers that also branch on `traits.accessibilityContrast == .high`.

**Type.** Keep SF text styles for everything that is a control or a label: navigation titles, buttons, rows, footnotes. **Manrope 800 earns its place on money**, the till's real content: the sale amount (44 pt, `.largeTitle`-relative), the customer code amount (40–56 pt), the Paid amount (34 pt), plus the two onboarding bookends (Welcome title 34 pt, Ready 30 pt). That's it. Step headings, sheet questions, sale-page headings and Disconnected become `.title2.weight(.bold)` (22 pt) or system `.largeTitle` via `navigationTitle`. Add `.monospacedDigit()` to Manrope amounts, using Manrope's `tnum` feature through `CTFontDescriptor` feature settings, so amounts don't jitter as XMR updates.

Scale (Large / default): amount 44 Manrope · title 28/22 SF bold · headline 17 semibold · body 17 · subheadline 15 · footnote 13 · caption 12 (TillBar's line; the floor is 11).

**Layout.** One window that is a list of sales on the left and the live sale on the right when there's room, collapsing to a stack on iPhone. The code goes full-bleed white whenever it faces the customer.

iPad, regular width, full screen (`NavigationSplitView`):
```
┌──────────────── sidebar 320 ───────┬──────────────── detail ─────────────────────────┐
│ Front Counter · ● Watching    ⚙    │ ‹  Sale #m2Qe            11:02 left    Close ✕   │
│ This shift  $142.30 · 9 paid       │                                                  │
│ [All] [Needs action 2] [Paid]      │              $13.88                              │
│ WAITING FOR PAYMENT             2  │           0.05552 XMR  (tap: share)              │
│ ◔ Sale #m2Qe  $13.88  · 11:02 left │      ╭────── ring ──────╮                        │
│ ○ Sale #k9Pa  $4.20   · 14:40 left │      │   ▓▓ QR 300 ▓▓   │   Order                │
│ EARLIER                            │      │   ▓▓        ▓▓   │   2 × Latte  $9.00     │
│ ✓ Sale #j1Qx  $8.40   Paid         │      ╰──────────────────╯   1 × Scone  $4.88     │
│ ✓ Sale #h7Rt  $15.75  Paid         │      Seen · confirming 2 of 10                   │
│                                    │  [ Show customer ⤢ ]  [ Share ⇪ ]                │
└────────────────────────────────────┴──────────────────────────────────────────────────┘
```
iPhone, compact (`NavigationStack`):
```
┌──────────── 390 ────────────┐
│ ‹ Sale #m2Qe       Close ✕  │  ← system nav bar, glass, swipe back
│          $13.88             │
│       0.05552 XMR           │
│   ╭──── ring ─────╮         │
│   │  ▓▓ QR 220 ▓▓ │  ⤢      │
│   ╰───────────────╯         │
│  ◔ Waiting · 11:02 left     │
│  Order  2 items   $13.88  › │
│                             │
│ [   Show customer  ⤢     ]  │  ← monero fill, the one primary
│ [   Share  ⇪             ]  │  ← .bordered
└─────────────────────────────┘
```

**Signature: the confirmation ring.** An orange arc around the payment code that fills as the payment moves through waiting, seen and confirming (n of 10), then closes and turns paid green. The same ring, scaled, is the row badge, the Live Activity's compact and minimal glyph (it fits the 28 pt circle, replacing the static mark), and the PiP window. It belongs to this product because following a Monero payment to confirmation *is* Till's job. No other POS companion needs one, and it answers the cashier's only question: "can I hand it over yet?"

**Motion: one moment.** At Paid, the ring finishes its sweep (0.35 s, `.snappy`), crossfades orange to green, and a checkmark draws on (`Image(systemName: "checkmark").symbolEffect(.drawOn)`), with `.sensoryFeedback(.success, trigger: paid)`. Under Reduce Motion it becomes a 0.2 s crossfade plus the haptic. Everything else uses system default transitions. The `RootView` `.animation(.easeInOut(0.25), value: core.screen)` (`RootView.swift:27`) goes once the NavigationStack provides pushes.

**Platform check of the plan.** Navigation, toolbars, lists, sheets, share and settings are all system components. Brand lives only in the amount type, the orange fill and the ring.

---

## Concrete changes, screen by screen (accessibility → conventions → craft → polish)

### 1. Accessibility
1. **Ready (onboarding)**: different symbol for not-done (`circle.dashed`), `.accessibilityValue`. (C1)
2. **Customer code / sale / shift rows**: `StepBar` and `RingGlyph` arcs to #E85D00, tracks to #8C8577, IC variant #C24E00. Give `StepBar` a spoken label with meaning ("Seen, confirming 2 of 10", not "Step 2 of 4"). (C2)
3. **Onboarding (all steps)**: buttons only in `.safeAreaInset(edge: .bottom)`, explanatory text in the scroll. `FactRow` → `ViewThatFits`. (I13)
4. **Shift**: `SaleRow` amount must never truncate (`layoutPriority`, allow 2 lines at accessibility sizes). (I13)
5. **Settings**: give the wallet-address copy button (`.borderless`, about 22 pt glyph, `SettingsScreen.swift:178-185`) a 44 × 44 `contentShape`, or replace it with `ShareLink`.
6. **Live Activity**: `isLuminanceReduced` handling on the lock screen; a limited-width compact view showing the ring only.

### 2. Conventions
1. **All till screens**: `NavigationStack(path:)` driven by `Route`; replace `TillBar` with `.navigationTitle`/`.navigationSubtitle` and toolbar items; swipe back works. (I2)
2. **iPad layout**: `horizontalSizeClass` instead of idiom; `NavigationSplitView` at regular width; `.contentMargins` instead of an outer `maxWidth`, so the whole window scrolls. Same in `OnboardingScreen`. (I1)
3. **Onboarding step 2 (scan)**: `AVCaptureDevice.RotationCoordinator` for the preview. (I10)
4. **Onboarding step 3 (connected)**: progress *inside* Continue ("Reading the store…" + `ProgressView`); drop `.scrollBounceBehavior(.basedOnSize)`. (I6)
5. **Onboarding step 6 (iPad)**: delete the Slide Over step. New "Alerts & floating code" step: a sample time-sensitive alert, a test alert, and a PiP preview. Rail label changed in `onboarding.rs:692`. (I4)
6. **Sale**: `Copy address` → `ShareLink` of the `monero:` URI. (I9)
7. **In-content buttons**: `.glass` → `.bordered` + `.controlSize(.large)`. Glass stays only on toolbar items. (I12)
8. **Settings**: the alerts `Toggle` becomes a status row plus an "Open Notification Settings" button. (I15)
9. **Close sheet**: Cancel toolbar item + visible drag indicator. (I16)
10. **Customer code**: an explicit 48 pt shrink button on iPad too; remove the whole-screen tap. (I14)

### 3. Craft
1. **Shift colours**: Seen/Confirming → `Tone::Plain` + part-filled ring; Waiting → `Tone::Muted`; green only for Paid (`shift.rs:366-368`). (I3)
2. **Onboarding step 4 (POS)**: collapse the chosen POS into one row with "Change"; `DisclosureGroup` for permissions; delete the logo-link-logo row; pin the test-sale status in the bottom inset; two columns at regular width. (I5)
3. **Shift, empty**: `ContentUnavailableView` overlay, centred both ways, with the *animated* `LoadingMark` "Watching for sales" and a "Send a test sale" action; hide the duplicate summary. (I11)
4. **Type**: Manrope only on amounts and the Welcome/Ready titles; every other `Heading` → `.title2.bold()` or `navigationTitle`.
5. **Signature**: promote `RingGlyph` to wrap the `CodeCard` (ring stroke 6 pt, 12 pt outside the white card), the row badge, the island's compact/minimal views and the PiP frame.
6. **Logo**: keep it on Welcome, Ready and the island only. Remove it from the rail header, `SheetTop`, the POS card, the shift bar and the sample alerts.
7. **Dark paper**: #1C1B19 → #000000 and cards → #1C1B19, in `Palette.ground` / `Palette.card` (`Palette.swift:9, 15`) and `WidgetPalette.darkGround`.
8. **iPad alert-first + PiP**: time-sensitive `UNNotificationContent.interruptionLevel = .timeSensitive`; tapping it opens the code; actions "Share" and "Close sale". PiP via `AVSampleBufferDisplayLayer` shows ring + amount + a small code, with "Show customer" bringing Till full screen.

### 4. Polish
1. **Onboarding**: drop "Step N of 6" on iPad (core returns `None`); on iPhone use a thin `ProgressView(value:total:)`, nothing on Welcome. (I7)
2. **Onboarding step 4 copy**: "Not now: only show other tills' sales" → "Set up later" + footnote. (I8)
3. **Copy consistency**: "Get started" (flow start) / "Continue" / "Start watching" (replacing "Start Till"); decide and document the button capitalization. (I17)
4. **Paid**: the one motion moment (ring closes, `symbolEffect(.drawOn)`, `.sensoryFeedback(.success)`), with a Reduce Motion crossfade.
5. **Sale**: the XMR line's hidden "tap to copy" becomes part of Share; remove the invisible button.
6. **Live Activity**: the ring as compact leading; the amount at `.title3.weight(.semibold)` in compact only when not limited in width.

**Critique again.** One more thing to remove: the RootView-wide `ProgressView` tinted orange at the top (`RootView.swift:18-21`). Busy state belongs in the control that is busy (I6). Quality floor once the above lands: works from a 320 pt iPhone SE width to a 1376 pt iPad window; Reduce Motion is respected (already, plus the Paid moment); Reduce Transparency and Increase Contrast get answers through system toolbars and the IC token variants; AX5 survives because the bottom insets hold buttons only.
