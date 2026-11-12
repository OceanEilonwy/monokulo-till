import AVFoundation
import SwiftUI
import TillCore
import UIKit

// Setting up a till on the shop's own wallet (the Vk* boards,
// docs/view-key-mode.md §11.1), drawn from the engine's wallet pages in
// setup's own layout: the boards' words, order and controls, in the
// column and bottom bar every other step uses, Back in the toolbar.

/// V1 (VkMode): how Till sees payments, each way a tile ticked when
/// chosen; the light wallet server shown but not chosen yet.
struct PaymentsModePage: View {
    let step: ModeStep
    let view: OnboardingView
    let send: Send

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            VStack(spacing: 14) {
                ForEach(Array(step.choices.enumerated()), id: \.offset) { _, choice in
                    ModeTile(choice: choice) { send(.choosePaymentsMode(mode: choice.mode)) }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 20)
        } bottom: {
            PrimaryPress(press: step.next) { send(.next) }
            Text(step.footnote)
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// One of V1's ways: a radio mark, its name and what it means.
struct ModeTile: View {
    let choice: ModeChoice
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 18) {
                Image(systemName: choice.selected ? "checkmark.circle.fill" : "circle")
                    .font(.title2)
                    .foregroundStyle(choice.selected ? Palette.ink : Palette.track)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(choice.title)
                        .font(.title3.weight(.semibold))
                    Text(choice.text)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    if let note = choice.note {
                        Text(note)
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(Palette.secondary)
                            .padding(.top, 2)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 16).fill(Palette.card))
            .overlay(
                RoundedRectangle(cornerRadius: 16)
                    .strokeBorder(choice.selected ? Palette.ink : Palette.border, lineWidth: choice.selected ? 2 : 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 16))
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.ink)
        .disabled(!choice.enabled)
        .opacity(choice.enabled ? 1 : 0.6)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(choice.selected ? .isSelected : [])
    }
}

/// V2 (VkWallet): Connect to Fob first, on its own card with its mark and
/// the step's one orange button; then the three ways to enter the wallet
/// yourself.
struct WalletEntryPage: View {
    let step: WalletEntryStep
    let view: OnboardingView
    let send: Send

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            FobCard(step: step) { send(.connectFob) }
                .padding(.horizontal, 16)
                .padding(.top, 20)
            SectionLabel(text: step.section)
            Card {
                ForEach(Array(step.ways.enumerated()), id: \.offset) { index, way in
                    if index > 0 {
                        Hairline()
                    }
                    WalletWayButton(way: way) { send(.chooseWalletWay(way: way.way)) }
                }
            }
        } bottom: {
            LinkPress(press: step.getFob) { send(.getFob) }
        }
    }
}

/// V2's Fob card: the mark, what Connect to Fob does, and its button
/// beside the words where they fit, else under them.
struct FobCard: View {
    let step: WalletEntryStep
    let connect: @MainActor () -> Void

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 20) {
                words
                PrimaryPress(press: step.connect, action: connect)
                    .frame(width: 200)
            }
            .frame(minWidth: 540)
            VStack(alignment: .leading, spacing: 18) {
                words
                PrimaryPress(press: step.connect, action: connect)
            }
        }
        .padding(22)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 16).fill(Palette.card))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Palette.ink, lineWidth: 2))
    }

    private var words: some View {
        HStack(spacing: 18) {
            FobMark(size: 56)
            VStack(alignment: .leading, spacing: 4) {
                Text(step.fobTitle)
                    .font(.title3.weight(.semibold))
                Text(step.fobText)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
    }
}

/// Fob's mark (monokulo-fob's docs/brand/fob-mark.svg), decoration only.
struct FobMark: View {
    var size: CGFloat = 56

    var body: some View {
        Image("FobMark")
            .resizable()
            .scaledToFit()
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// Each of V2's ways' symbol.
enum WalletWayLook {
    static func symbol(_ way: WalletWay) -> String {
        switch way {
        case .scanCode: "qrcode.viewfinder"
        case .keys: "key"
        case .seed: "text.alignleft"
        }
    }
}

/// One of V2's ways to enter the wallet: its symbol, name and line, and a
/// chevron, as a list's row that goes on.
struct WalletWayButton: View {
    let way: WalletWayRow
    let action: @MainActor () -> Void

    var body: some View {
        Button {
            action()
        } label: {
            HStack(spacing: 14) {
                Image(systemName: WalletWayLook.symbol(way.way))
                    .font(.title3)
                    .frame(width: 28)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(way.title)
                    Text(way.text)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                Image(systemName: "chevron.forward")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.track)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .frame(minHeight: 64)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.ink)
        .accessibilityElement(children: .combine)
    }
}

/// C1 (VkPadRequest): the request's code and its pattern on a white card,
/// the three steps beside or under it, and "Scan Fob's code", which opens
/// the camera for Fob's answer.
struct FobRequestPage: View {
    let step: FobRequestStep
    let view: OnboardingView
    let send: Send

    @State private var scanning = false
    @Environment(\.openURL) private var openURL
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 32) {
                    codeCard
                    steps
                        .frame(maxWidth: 300)
                }
                VStack(alignment: .leading, spacing: 22) {
                    codeCard
                        .frame(maxWidth: .infinity)
                    steps
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 20)
            ProblemNote(text: step.problem)
        } bottom: {
            PrimaryPress(press: step.scan, systemImage: "qrcode.viewfinder") { scan() }
        }
        .sheet(isPresented: $scanning) {
            FobAnswerScanner(looking: step.looking) { text in
                scanning = false
                send(.fobAnswer(text: text))
            }
        }
    }

    private var codeSide: CGFloat { sizeClass == .regular ? 280 : 240 }

    private var codeCard: some View {
        VStack(spacing: 16) {
            QRCodeView(text: step.qr, label: step.patternCaption, size: codeSide, padding: 0, cornerRadius: 0)
            HStack(spacing: 14) {
                FobPatternView(pattern: step.pattern, size: 64)
                // On the code's white card in either theme: the light
                // theme's secondary ink.
                Text(step.patternCaption)
                    .font(.subheadline)
                    .foregroundStyle(Color(hex: 0x5A564C))
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 160, alignment: .leading)
            }
            .accessibilityElement(children: .combine)
        }
        .padding(22)
        .background(RoundedRectangle(cornerRadius: 16).fill(Palette.codeGround))
    }

    private var steps: some View {
        VStack(alignment: .leading, spacing: 22) {
            ForEach(Array(step.steps.enumerated()), id: \.offset) { index, text in
                HStack(alignment: .firstTextBaseline, spacing: 14) {
                    Text(String(index + 1))
                        .font(.subheadline.weight(.bold))
                        .foregroundStyle(Palette.ground)
                        .frame(width: 28, height: 28)
                        .background(Circle().fill(Palette.ink))
                        .accessibilityHidden(true)
                    Rich.text(text)
                        .font(.body)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    /// The camera, once iOS allows it; after a refusal only Settings can.
    private func scan() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .denied, .restricted:
            if let settings = URL(string: UIApplication.openSettingsURLString) {
                openURL(settings)
            }
        default:
            scanning = true
        }
    }
}

/// C1's camera, in a sheet: Fob's answer code, sent once.
struct FobAnswerScanner: View {
    let looking: String
    let found: @MainActor (String) -> Void

    @State private var camera = AVCaptureDevice.authorizationStatus(for: .video)
    @State private var sent = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ZStack {
                Color.black.ignoresSafeArea()
                if camera == .authorized {
                    QRScanner { text in
                        guard !sent else { return }
                        sent = true
                        found(text)
                    }
                    .ignoresSafeArea()
                }
                ViewfinderCorners()
                    .stroke(Color.white, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                    .frame(width: 240, height: 240)
                    .accessibilityHidden(true)
                VStack {
                    Spacer()
                    Text(looking)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Color.white)
                        .padding(.horizontal, 18)
                        .frame(minHeight: 40)
                        .background(Capsule().fill(Color.black.opacity(0.55)))
                        .padding(24)
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
            .toolbarColorScheme(.dark, for: .navigationBar)
        }
        .task {
            guard camera == .notDetermined else { return }
            _ = await AVCaptureDevice.requestAccess(for: .video)
            camera = AVCaptureDevice.authorizationStatus(for: .video)
        }
    }
}

/// A viewfinder's four corners (VkScanCode), rounded.
struct ViewfinderCorners: Shape {
    var arm: CGFloat = 44
    var radius: CGFloat = 12

    func path(in rect: CGRect) -> Path {
        var path = Path()
        let r = radius
        // Top left.
        path.move(to: CGPoint(x: rect.minX, y: rect.minY + arm))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + r))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.minY), tangent2End: CGPoint(x: rect.minX + r, y: rect.minY), radius: r)
        path.addLine(to: CGPoint(x: rect.minX + arm, y: rect.minY))
        // Top right.
        path.move(to: CGPoint(x: rect.maxX - arm, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - r, y: rect.minY))
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.minY), tangent2End: CGPoint(x: rect.maxX, y: rect.minY + r), radius: r)
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + arm))
        // Bottom right.
        path.move(to: CGPoint(x: rect.maxX, y: rect.maxY - arm))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - r))
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.maxY), tangent2End: CGPoint(x: rect.maxX - r, y: rect.maxY), radius: r)
        path.addLine(to: CGPoint(x: rect.maxX - arm, y: rect.maxY))
        // Bottom left.
        path.move(to: CGPoint(x: rect.minX + arm, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX + r, y: rect.maxY))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.maxY), tangent2End: CGPoint(x: rect.minX, y: rect.maxY - r), radius: r)
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - arm))
        return path
    }
}

/// The request's pattern (VkIdenticon): the engine's 5 by 5 cells, row by
/// row, in its ink on its paper, the same in light and dark. No words.
struct FobPatternView: View {
    let pattern: FobPattern
    var size: CGFloat = 64

    var body: some View {
        let gap = max(2, (size / 40).rounded())
        let inset = (size * 0.1).rounded()
        let cell = (size - inset * 2 - gap * 4) / 5
        let ink = FobPatternView.color(pattern.ink)
        VStack(spacing: gap) {
            ForEach(0..<5, id: \.self) { row in
                HStack(spacing: gap) {
                    ForEach(0..<5, id: \.self) { column in
                        let index = row * 5 + column
                        let on = index < pattern.cells.count && pattern.cells[index]
                        RoundedRectangle(cornerRadius: gap)
                            .fill(on ? ink : Color.clear)
                            .frame(width: cell, height: cell)
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .background(RoundedRectangle(cornerRadius: size * 0.12).fill(FobPatternView.color(pattern.paper)))
        .overlay(RoundedRectangle(cornerRadius: size * 0.12).strokeBorder(Color(hex: 0xE4DED2), lineWidth: 1))
        .accessibilityHidden(true)
    }

    /// `#rrggbb` as a colour; ink when it isn't one.
    static func color(_ text: String) -> Color {
        let digits = text.hasPrefix("#") ? String(text.dropFirst()) : text
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else { return Palette.codeInk }
        return Color(hex: value)
    }
}

/// B4 (phone) and C4 (iPad), VkConnected and VkPadConnected: what Fob
/// handed over, then Till's own Paid after and code expiry.
struct FobConnectedPage: View {
    let step: FobConnectedStep
    let view: OnboardingView
    let send: Send

    /// Back, where there's no "Use another wallet" (the phone): both go
    /// back to V2.
    private var back: (@MainActor () -> Void)? {
        guard step.otherWallet == nil else { return nil }
        return { send(.back) }
    }

    var body: some View {
        StepPage(back: back) {
            VStack(alignment: .leading, spacing: 10) {
                if let eyebrow = step.eyebrow {
                    Text(eyebrow.uppercased())
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                }
                HStack(spacing: 12) {
                    DoneTick(size: 30)
                    Heading(text: view.heading)
                }
                Lead(text: view.lead)
            }
            .padding(.horizontal, 20)
            .padding(.top, 12)
            FactCard(facts: step.facts)
                .padding(.top, 20)
            SectionLabel(text: step.section)
            SteppersCard(rows: [step.paidAfter, step.expiry]) { index, up in
                send(index == 0 ? .stepPaidAfter(up: up) : .stepCodeExpiry(up: up))
            }
            Footnote(text: step.footnote)
            ProblemNote(text: step.problem)
        } bottom: {
            PrimaryPress(press: step.next) { send(.next) }
            if let other = step.otherWallet {
                LinkPress(press: other) { send(.useAnotherWallet) }
            }
        }
    }
}

/// Steppers on a card, hairlines between, each a ``StepperRow``.
struct SteppersCard: View {
    let rows: [SetupStepper]
    /// A row's place, and up (+) or down (−).
    let step: @MainActor (Int, Bool) -> Void

    var body: some View {
        Card {
            ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                if index > 0 {
                    Hairline()
                }
                StepperRow(stepper: row) { up in step(index, up) }
            }
        }
    }
}

/// A value with − and + (D4, B4, C4 and wallet Settings), by the owner's
/// rule for every platform: on a phone the value is the line under its
/// label, − and + at the right (B4); on iPad it sits between − and + (C4,
/// D4), and the label wraps before the control does. VoiceOver hears one
/// adjustable value.
struct StepperRow: View {
    let stepper: SetupStepper
    /// In a list's row (Settings), which pads it itself.
    var inList = false
    let step: @MainActor (Bool) -> Void

    private var valueInside: Bool { UIDevice.current.userInterfaceIdiom == .pad }

    var body: some View {
        Group {
            if valueInside {
                HStack(spacing: 12) {
                    words(stepper.detail.map { [$0] } ?? [])
                    Spacer(minLength: 8)
                    control(inside: true)
                }
            } else {
                beside
            }
        }
        .padding(.horizontal, inList ? 0 : 16)
        .padding(.vertical, inList ? 2 : 10)
        .frame(minHeight: inList ? 44 : 60)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(stepper.label)
        .accessibilityValue([stepper.value, stepper.detail].compactMap { $0 }.joined(separator: ", "))
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: if stepper.plus.enabled { step(true) }
            case .decrement: if stepper.minus.enabled { step(false) }
            @unknown default: break
            }
        }
    }

    /// The value (and its line) under the label, and − + beside them.
    private var beside: some View {
        HStack(spacing: 12) {
            words([stepper.value] + (stepper.detail.map { [$0] } ?? []))
            Spacer(minLength: 8)
            control(inside: false)
        }
    }

    private func words(_ lines: [String]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(stepper.label)
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(line)
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }

    private func control(inside: Bool) -> some View {
        HStack(spacing: 0) {
            button(stepper.minus, symbol: "minus", up: false)
            if inside {
                Text(stepper.value)
                    .font(.body.weight(.semibold))
                    .monospacedDigit()
                    .fixedSize()
                    .frame(minWidth: 32)
            }
            button(stepper.plus, symbol: "plus", up: true)
        }
        .background(RoundedRectangle(cornerRadius: 10).fill(Palette.chip))
        .fixedSize()
    }

    private func button(_ press: Press, symbol: String, up: Bool) -> some View {
        Button {
            step(up)
        } label: {
            Image(systemName: symbol)
                .font(.body.weight(.medium))
                .frame(width: minimumTarget, height: minimumTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.ink)
        .disabled(!press.enabled)
        .opacity(press.enabled ? 1 : 0.35)
    }
}

/// D1 (VkScanCode): the camera, framed, looking for a wallet code; or one
/// pasted.
struct WalletCodePage: View {
    let step: WalletCodeStep
    let view: OnboardingView
    let send: Send

    @State private var camera = AVCaptureDevice.authorizationStatus(for: .video)
    @Environment(\.openURL) private var openURL

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            viewfinder
                .padding(.horizontal, 16)
                .padding(.top, 20)
            ProblemNote(text: step.problem)
            Button {
                paste()
            } label: {
                Text(step.paste.label)
                    .font(.body)
                    .frame(minHeight: minimumTarget)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(Palette.link)
            .padding(.horizontal, 20)
            .padding(.top, 8)
            Text(step.footnote)
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 20)
        } bottom: {
            EmptyView()
        }
        .task { await askForCamera() }
    }

    private var viewfinder: some View {
        ZStack {
            Palette.codeInk
            if camera == .authorized {
                QRScanner { text in send(.scanWalletCode(text: text)) }
                ViewfinderCorners()
                    .stroke(Color.white, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                    .frame(width: 240, height: 240)
                    .accessibilityHidden(true)
                VStack {
                    Spacer()
                    Text(step.looking)
                        .font(.subheadline)
                        .foregroundStyle(Color(hex: 0xECE8DF))
                        .padding(.bottom, 18)
                }
            } else {
                VStack(spacing: 12) {
                    Text(step.cameraAsk)
                        .font(.subheadline)
                        .foregroundStyle(Color.white)
                        .multilineTextAlignment(.center)
                    Button(step.allowCamera) { allowCamera() }
                        .buttonStyle(.primary)
                        .frame(maxWidth: 240)
                }
                .padding(24)
            }
        }
        .frame(height: 380)
        .clipShape(RoundedRectangle(cornerRadius: 16))
        .accessibilityElement(children: .contain)
    }

    private func paste() {
        let text = UIPasteboard.general.string?.trimmingCharacters(in: .whitespacesAndNewlines)
        send(.pasteWalletCode(text: text))
    }

    private func askForCamera() async {
        guard camera == .notDetermined else { return }
        _ = await AVCaptureDevice.requestAccess(for: .video)
        camera = AVCaptureDevice.authorizationStatus(for: .video)
    }

    private func allowCamera() {
        if camera == .notDetermined {
            Task { await askForCamera() }
        } else if let settings = URL(string: UIApplication.openSettingsURLString) {
            openURL(settings)
        }
    }
}

/// D2 (VkKeys): the network, then the address and the view key, each sent
/// to the engine as it's typed, which says under each field what's wrong
/// (or that the address is right). The view key stays in its secure
/// field; the engine never hands it back.
struct WalletKeysPage: View {
    let step: WalletKeysStep
    let view: OnboardingView
    let send: Send

    @State private var address = ""
    @State private var viewKey = ""
    @State private var helpOpen = false

    /// The network chosen, by its place: Mainnet, then Stagenet.
    private var network: WalletNetwork {
        step.networks.firstIndex { $0.selected } == 1 ? .stagenet : .mainnet
    }

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 8) {
                    FieldLabel(text: step.networkLabel)
                    Picker(step.networkLabel, selection: Binding(
                        get: { network },
                        set: { edit(network: $0) }
                    )) {
                        ForEach(Array(step.networks.enumerated()), id: \.offset) { index, choice in
                            Text(choice.label).tag(index == 1 ? WalletNetwork.stagenet : WalletNetwork.mainnet)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 320)
                }
                VStack(alignment: .leading, spacing: 8) {
                    FieldLabel(text: step.address.label)
                    AddressField(text: $address)
                        .accessibilityLabel(step.address.label)
                        .fieldBox(problem: !step.address.good && step.address.note != nil)
                        .onChange(of: address) { edit() }
                    FieldNote(field: step.address)
                }
                VStack(alignment: .leading, spacing: 8) {
                    FieldLabel(text: step.viewKey.label)
                    SecureField("", text: $viewKey)
                        .accessibilityLabel(step.viewKey.label)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.asciiCapable)
                        .textContentType(nil)
                        .fieldBox(problem: !step.viewKey.good && step.viewKey.note != nil)
                        .onChange(of: viewKey) { edit() }
                    FieldNote(field: step.viewKey)
                }
                DisclosureGroup(isExpanded: $helpOpen) {
                    Text(step.help)
                        .font(.subheadline)
                        .foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.top, 4)
                } label: {
                    Text(step.helpTitle)
                        .foregroundStyle(Palette.link)
                        .frame(minHeight: minimumTarget)
                }
                .tint(Palette.link)
            }
            .padding(.horizontal, 20)
            .padding(.top, 20)
        } bottom: {
            PrimaryPress(press: step.next) { send(.next) }
        }
        .onAppear {
            if address.isEmpty { address = step.address.value }
        }
        .onDisappear { viewKey = "" }
    }

    private func edit(network chosen: WalletNetwork? = nil) {
        send(.editWalletKeys(network: chosen ?? network, address: address, viewKey: viewKey))
    }
}

/// D2's address: monospaced, wrapped at any character and never
/// hyphenated (a hyphen in an address would read as part of it), with no
/// correction, capitals, smart punctuation or suggestions. SwiftUI's
/// vertical text field hyphenates long words, so this is UIKit's text view.
struct AddressField: UIViewRepresentable {
    @Binding var text: String

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        view.isScrollEnabled = false
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.textContainer.lineBreakMode = .byCharWrapping
        view.autocorrectionType = .no
        view.autocapitalizationType = .none
        view.spellCheckingType = .no
        view.smartQuotesType = .no
        view.smartDashesType = .no
        view.smartInsertDeleteType = .no
        view.inlinePredictionType = .no
        view.keyboardType = .asciiCapable
        view.textContentType = nil
        view.adjustsFontForContentSizeCategory = true
        view.typingAttributes = AddressField.attributes
        view.attributedText = NSAttributedString(string: text, attributes: AddressField.attributes)
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        if view.text != text {
            view.attributedText = NSAttributedString(string: text, attributes: AddressField.attributes)
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 320
        let fitted = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        return CGSize(width: width, height: max(fitted.height, AddressField.font.lineHeight * 2))
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    @MainActor static var font: UIFont {
        UIFontMetrics(forTextStyle: .subheadline).scaledFont(
            for: .monospacedSystemFont(ofSize: 15, weight: .regular)
        )
    }

    /// Ink, monospaced, wrapped at characters, no hyphens.
    @MainActor static var attributes: [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byCharWrapping
        paragraph.hyphenationFactor = 0
        paragraph.lineSpacing = 3
        return [.font: font, .foregroundColor: UIColor(Palette.ink), .paragraphStyle: paragraph]
    }

    @MainActor final class Coordinator: NSObject, UITextViewDelegate {
        let text: Binding<String>

        init(text: Binding<String>) {
            self.text = text
        }

        func textViewDidChange(_ textView: UITextView) {
            text.wrappedValue = textView.text
        }
    }
}

/// A field's name over it.
struct FieldLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.subheadline.weight(.semibold))
            .accessibilityAddTraits(.isHeader)
    }
}

/// What the engine says under a field: a tick and green for right, red
/// for what's wrong.
struct FieldNote: View {
    let field: KeyField

    var body: some View {
        if let note = field.note {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                if field.good {
                    Image(systemName: "checkmark")
                        .font(.footnote.weight(.bold))
                        .accessibilityHidden(true)
                }
                Text(note)
                    .font(.footnote)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .foregroundStyle(field.good ? Palette.paid : Palette.danger)
        }
    }
}

extension View {
    /// A setup field's box: white, a border, red and heavier when what's in
    /// it is wrong.
    func fieldBox(problem: Bool) -> some View {
        padding(.horizontal, 14)
            .padding(.vertical, 12)
            .frame(minHeight: 50)
            .background(RoundedRectangle(cornerRadius: 10).fill(Palette.card))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(problem ? Palette.danger : Palette.border, lineWidth: problem ? 2 : 1)
            )
    }
}

/// D3 (VkSeed): the warning and its way to a view-only code instead, how
/// many words, and a secure field for each. Continue sends the words and
/// clears every field at once; the keyboard neither suggests nor learns
/// them.
struct WalletSeedPage: View {
    let step: WalletSeedStep
    let view: OnboardingView
    let send: Send

    @State private var words: [String] = []

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead.isEmpty ? nil : view.lead)
            warning
                .padding(.horizontal, 16)
                .padding(.top, 16)
            Picker("", selection: Binding(
                get: { step.lengths.firstIndex { $0.selected } ?? 0 },
                set: { send(.chooseSeedLength(words: $0 == 1 ? 16 : 25)) }
            )) {
                ForEach(Array(step.lengths.enumerated()), id: \.offset) { index, choice in
                    Text(choice.label).tag(index)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(maxWidth: 360)
            .padding(.horizontal, 20)
            .padding(.top, 18)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 104), spacing: 8)], spacing: 8) {
                ForEach(0..<words.count, id: \.self) { index in
                    // Read through a guard: the fields go from 25 to 16
                    // while a field's view may still ask for its word.
                    SeedWordField(number: index + 1, word: Binding(
                        get: { index < words.count ? words[index] : "" },
                        set: { if index < words.count { words[index] = $0 } }
                    ))
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            Footnote(text: step.footnote)
            ProblemNote(text: step.problem)
        } bottom: {
            PrimaryPress(press: step.next) { enter() }
        }
        .onChange(of: step.words, initial: true) { _, count in
            words = Array(repeating: "", count: Int(count))
        }
        .onDisappear { words = Array(repeating: "", count: words.count) }
    }

    /// The warning, its last words a link to D1.
    private var warning: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(Palette.expiredInk)
                .accessibilityHidden(true)
            Text(warningText)
                .font(.subheadline)
                .fixedSize(horizontal: false, vertical: true)
                .tint(Palette.link)
                .environment(\.openURL, OpenURLAction { _ in
                    MainActor.assumeIsolated { send(.chooseWalletWay(way: .scanCode)) }
                    return .handled
                })
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(RoundedRectangle(cornerRadius: 12).fill(Palette.expiredGround))
    }

    private var warningText: AttributedString {
        var text = AttributedString(step.warning + " ")
        var link = AttributedString(step.scanInstead.label)
        link.link = URL(string: "till:scan-instead")
        link.swiftUI.foregroundColor = Palette.link
        text += link
        text += AttributedString(".")
        return text
    }

    /// The words to the engine, then gone from the fields and this view.
    private func enter() {
        let phrase = words
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        words = Array(repeating: "", count: words.count)
        send(.enterSeed(words: phrase))
    }
}

/// One of D3's words: its number and a secure field that won't suggest,
/// correct or learn.
struct SeedWordField: View {
    let number: Int
    @Binding var word: String

    var body: some View {
        HStack(spacing: 6) {
            Text(String(number))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .monospacedDigit()
                .frame(width: 20, alignment: .trailing)
                .accessibilityHidden(true)
            SecureField("", text: $word)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.asciiCapable)
                .textContentType(nil)
                .accessibilityLabel("Word \(number)")
        }
        .padding(.horizontal, 8)
        .frame(height: 40)
        .background(RoundedRectangle(cornerRadius: 8).fill(Palette.card))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.border, lineWidth: 1))
    }
}

/// D4 (VkTillNo): the till's number, the node and its checks, Paid after
/// and the code's expiry.
struct ThisTillPage: View {
    let step: ThisTillStep
    let view: OnboardingView
    let send: Send

    @State private var changingNode = false

    var body: some View {
        StepPage(back: { send(.back) }) {
            PageTop(heading: view.heading, lead: view.lead)
            VStack(spacing: 18) {
                if let number = step.number {
                    SteppersCard(rows: [number]) { _, up in send(.stepTillNumber(up: up)) }
                }
                if let node = step.node {
                    NodeCard(node: node) { changingNode = true }
                }
                SteppersCard(rows: [step.paidAfter, step.expiry]) { index, up in
                    send(index == 0 ? .stepPaidAfter(up: up) : .stepCodeExpiry(up: up))
                }
            }
            .padding(.top, 20)
            ProblemNote(text: step.problem)
        } bottom: {
            PrimaryPress(press: step.next) { send(.next) }
        }
        .sheet(isPresented: $changingNode) {
            if let node = step.node {
                NodeSheet(node: node) { url in
                    changingNode = false
                    send(.setNode(url: url))
                }
                .presentationDetents([.medium, .large])
            }
        }
    }
}

/// D4's node: where it is, Change, and the checks under it.
struct NodeCard: View {
    let node: NodeRow
    let change: @MainActor () -> Void

    var body: some View {
        Card {
            // On one line where it fits, else the address under its label.
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) {
                    Text(node.label)
                    Spacer(minLength: 8)
                    address
                    changeButton
                }
                HStack(spacing: 12) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(node.label)
                        address
                            .font(.subheadline)
                    }
                    Spacer(minLength: 8)
                    changeButton
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 4)
            .frame(minHeight: 52)
            if !node.checks.isEmpty {
                Hairline()
                FlowRow(spacing: 18, lineSpacing: 8) {
                    ForEach(Array(node.checks.enumerated()), id: \.offset) { _, check in
                        NodeCheck(check: check)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
        }
    }

    private var address: some View {
        Text(node.value)
            .foregroundStyle(Palette.secondary)
            .lineLimit(1)
            .fixedSize()
    }

    private var changeButton: some View {
        Button(node.change.label) { change() }
            .buttonStyle(.plain)
            .foregroundStyle(Palette.link)
            .frame(minHeight: minimumTarget)
            .disabled(!node.change.enabled)
    }
}

/// One of the node's checks: a tick, a cross or a spinner, and its words.
struct NodeCheck: View {
    let check: NodeCheckRow

    var body: some View {
        HStack(spacing: 6) {
            switch check.mark {
            case .passed:
                Image(systemName: "checkmark")
                    .font(.footnote.weight(.bold))
                    .foregroundStyle(Palette.paid)
            case .failed:
                Image(systemName: "xmark")
                    .font(.footnote.weight(.bold))
                    .foregroundStyle(Palette.danger)
            case .checking:
                ProgressView()
                    .controlSize(.mini)
            }
            Text(check.text)
                .font(.subheadline)
        }
        .accessibilityElement(children: .combine)
        .accessibilityValue(NodeCheck.spoken(check.mark))
    }

    static func spoken(_ mark: CheckMark) -> String {
        switch mark {
        case .passed: "Passed"
        case .failed: "Failed"
        case .checking: "Checking"
        }
    }
}

/// D4's Change: Fob's default nodes, or one typed.
struct NodeSheet: View {
    let node: NodeRow
    let use: @MainActor (String) -> Void

    @State private var typed = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if !node.choices.isEmpty {
                    Section {
                        ForEach(node.choices, id: \.self) { url in
                            Button {
                                use(url)
                            } label: {
                                HStack {
                                    Text(url)
                                        .foregroundStyle(Palette.ink)
                                    Spacer(minLength: 8)
                                    if url == node.url {
                                        Image(systemName: "checkmark")
                                            .foregroundStyle(Palette.link)
                                            .accessibilityHidden(true)
                                    }
                                }
                            }
                            .accessibilityAddTraits(url == node.url ? .isSelected : [])
                        }
                    }
                }
                Section {
                    TextField("https://", text: $typed)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .onSubmit { useTyped() }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Palette.ground)
            .navigationTitle(node.label)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(node.change.label) { useTyped() }
                        .disabled(typed.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private func useTyped() {
        let url = typed.trimmingCharacters(in: .whitespaces)
        guard !url.isEmpty else { return }
        use(url)
    }
}

/// Its views side by side, wrapping onto new lines where they don't fit.
struct FlowRow: Layout {
    var spacing: CGFloat = 8
    var lineSpacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        let rows = lines(width: width, subviews: subviews)
        let height = rows.reduce(0) { $0 + $1.height } + lineSpacing * CGFloat(max(0, rows.count - 1))
        let widest = rows.map(\.width).max() ?? 0
        return CGSize(width: proposal.width ?? widest, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for line in lines(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for index in line.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y + (line.height - size.height) / 2), proposal: .unspecified)
                x += size.width + spacing
            }
            y += line.height + lineSpacing
        }
    }

    private struct Line {
        var indices: [Int] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func lines(width: CGFloat, subviews: Subviews) -> [Line] {
        var lines: [Line] = []
        var line = Line()
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let needed = line.indices.isEmpty ? size.width : line.width + spacing + size.width
            if needed > width, !line.indices.isEmpty {
                lines.append(line)
                line = Line()
            }
            line.width = line.indices.isEmpty ? size.width : line.width + spacing + size.width
            line.height = max(line.height, size.height)
            line.indices.append(index)
        }
        if !line.indices.isEmpty {
            lines.append(line)
        }
        return lines
    }
}
