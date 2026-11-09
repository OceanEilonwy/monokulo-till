@preconcurrency import AVFoundation
@preconcurrency import AVKit
import CoreMedia
import CoreVideo
import SwiftUI
import UIKit

/// The iPad's Picture in Picture window (docs/ipad-redesign.md decision 2,
/// "A floating window"): iOS draws the window, Till only its picture,
/// through an `AVSampleBufferDisplayLayer` content source fed with frames
/// SwiftUI draws (``PipFrameView``). The window starts by itself when Till
/// goes to the back (`canStartPictureInPictureAutomaticallyFromInline`),
/// and goes when Till comes to the front.
///
/// What it needs from the app: the Background Modes capability's "Audio,
/// AirPlay, and Picture in Picture" (`UIBackgroundModes` `audio`, in
/// Support/Info.plist) and an audio session in the playback category,
/// without which iOS never offers Picture in Picture; no entitlement. The
/// session mixes with others, so Square's sounds play on: Till has no
/// sound of its own here.
///
/// The picture's shape is the frames' aspect: a strip (3:1) while
/// watching, a square for a sale's code, and iOS reshapes the window to
/// match. Frames are drawn when the picture or the window's size changes,
/// and each second while a countdown shows.
@MainActor
final class PipWindow: NSObject {
    /// ⤢: Till comes to the front on this sale (A2).
    var onRestore: (@MainActor (String) -> Void)?
    /// The system's play control on a cash sale offered in Monero (K1):
    /// take it in Monero.
    var onTakeInMonero: (@MainActor (String) -> Void)?

    /// The layer the window shows, also drawn small in the app's window
    /// (``PipSourceView``), as iOS starts Picture in Picture from a layer
    /// on screen.
    let layer = AVSampleBufferDisplayLayer()

    private var controller: AVPictureInPictureController?
    private var state: TillActivityAttributes.ContentState?
    private var cash: PipCashOffer?
    private var place = ""
    /// The window's width in points, as iOS last said.
    private var width: CGFloat?
    private var drawn: PipFrameKey?
    private var ticker: Task<Void, Never>?
    private let timebase: CMTimebase?

    /// Pixels to a point: every iPad iPadOS 26 runs on draws at 2x.
    static let scale: CGFloat = 2

    /// Whether this device can show it: iPad, with Picture in Picture.
    static var supported: Bool {
        UIDevice.current.userInterfaceIdiom == .pad && AVPictureInPictureController.isPictureInPictureSupported()
    }

    override init() {
        var timebase: CMTimebase?
        CMTimebaseCreateWithSourceClock(allocator: kCFAllocatorDefault, sourceClock: CMClockGetHostTimeClock(), timebaseOut: &timebase)
        if let timebase {
            CMTimebaseSetTime(timebase, time: CMClockGetTime(CMClockGetHostTimeClock()))
            CMTimebaseSetRate(timebase, rate: 1)
        }
        self.timebase = timebase
        super.init()
        layer.videoGravity = .resizeAspect
        layer.controlTimebase = timebase
    }

    /// The window is up.
    var isActive: Bool { controller?.isPictureInPictureActive ?? false }

    /// What it shows now.
    var picture: PipPicture {
        PipPicture.of(state, cash: cash, place: place)
    }

    /// The engine's state for the window (`Update.liveActivity` on iPad):
    /// nil while the floating window isn't chosen, setup isn't done or the
    /// shift is off, and then the window goes.
    func show(_ state: TillActivityAttributes.ContentState?) {
        self.state = state
        if state == nil {
            stop()
        } else {
            start()
        }
        controller?.invalidatePlaybackState()
        draw()
    }

    /// The shift as last published: the location the strip names, and the
    /// first cash sale offered in Monero.
    func shift(place: String, cash: PipCashOffer?) {
        let changed = cash != self.cash
        self.place = place
        self.cash = cash
        if changed { controller?.invalidatePlaybackState() }
        draw()
    }

    /// Till came to the front: the window goes, to start again by itself
    /// the next time Till goes to the back.
    func appInFront() {
        if isActive {
            controller?.stopPictureInPicture()
        }
    }

    // MARK: Up and down

    private func start() {
        guard controller == nil, PipWindow.supported else { return }
        do {
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .moviePlayback, options: [.mixWithOthers])
            try AVAudioSession.sharedInstance().setActive(true)
        } catch {
            NSLog("till: no audio session for Picture in Picture: %@", String(describing: error))
        }
        let source = AVPictureInPictureController.ContentSource(sampleBufferDisplayLayer: layer, playbackDelegate: self)
        let controller = AVPictureInPictureController(contentSource: source)
        controller.delegate = self
        controller.canStartPictureInPictureAutomaticallyFromInline = true
        // No skipping in a picture that isn't a video.
        controller.requiresLinearPlayback = true
        self.controller = controller
        ticker = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                self?.draw()
            }
        }
    }

    private func stop() {
        ticker?.cancel()
        ticker = nil
        if let controller {
            controller.canStartPictureInPictureAutomaticallyFromInline = false
            if controller.isPictureInPictureActive {
                controller.stopPictureInPicture()
            }
        }
        controller = nil
        drawn = nil
        layer.sampleBufferRenderer.flush(removingDisplayedImage: true, completionHandler: nil)
        try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
    }

    // MARK: Frames

    /// Draws the picture again if anything it shows changed.
    private func draw() {
        guard controller != nil else { return }
        let picture = picture
        let now = Date()
        let size = picture.shape.size(width: width)
        let key = PipFrameKey(picture: picture, size: size, timeLeft: picture.timeLeft(now: now))
        guard key != drawn else { return }
        guard let buffer = PipWindow.frame(PipFrameView(picture: picture, size: size, now: now), size: size, scale: PipWindow.scale, timebase: timebase) else {
            return
        }
        let renderer = layer.sampleBufferRenderer
        if renderer.status == .failed {
            renderer.flush()
        }
        renderer.enqueue(buffer)
        drawn = key
    }

    /// `view` as a sample buffer, shown as soon as it's enqueued.
    private static func frame(_ view: some View, size: CGSize, scale: CGFloat, timebase: CMTimebase?) -> CMSampleBuffer? {
        let renderer = ImageRenderer(content: view)
        renderer.proposedSize = ProposedViewSize(size)
        renderer.scale = scale
        renderer.isOpaque = true
        guard let image = renderer.cgImage else { return nil }
        guard let pixels = PipWindow.pixelBuffer(image) else { return nil }
        var format: CMVideoFormatDescription?
        CMVideoFormatDescriptionCreateForImageBuffer(allocator: kCFAllocatorDefault, imageBuffer: pixels, formatDescriptionOut: &format)
        guard let format else { return nil }
        let now = timebase.map(CMTimebaseGetTime) ?? CMClockGetTime(CMClockGetHostTimeClock())
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: now, decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?
        CMSampleBufferCreateReadyWithImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixels,
            formatDescription: format,
            sampleTiming: &timing,
            sampleBufferOut: &sample
        )
        guard let sample else { return nil }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true),
           CFArrayGetCount(attachments) > 0 {
            let first = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(
                first,
                Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                Unmanaged.passUnretained(kCFBooleanTrue).toOpaque()
            )
        }
        return sample
    }

    /// `image` in a BGRA pixel buffer the display layer can show.
    private static func pixelBuffer(_ image: CGImage) -> CVPixelBuffer? {
        let width = image.width, height = image.height
        let attributes: [CFString: Any] = [
            kCVPixelBufferCGImageCompatibilityKey: true,
            kCVPixelBufferCGBitmapContextCompatibilityKey: true,
            kCVPixelBufferIOSurfacePropertiesKey: [String: Any](),
        ]
        var buffer: CVPixelBuffer?
        let status = CVPixelBufferCreate(kCFAllocatorDefault, width, height, kCVPixelFormatType_32BGRA, attributes as CFDictionary, &buffer)
        guard status == kCVReturnSuccess, let buffer else { return nil }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let context = CGContext(
            data: CVPixelBufferGetBaseAddress(buffer),
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: CVPixelBufferGetBytesPerRow(buffer),
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else { return nil }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return buffer
    }
}

/// What a drawn frame showed: the next is drawn only when this changes.
struct PipFrameKey: Equatable {
    let picture: PipPicture
    let size: CGSize
    let timeLeft: String?
}

extension PipPicture {
    /// The countdown the frame shows at `now`, if any.
    func timeLeft(now: Date) -> String? {
        if case .sale(let sale) = self, sale.code != nil { return sale.timeLeft(now: now) }
        return nil
    }

    /// Whether the system's playback control shows play: only for a cash
    /// sale offered in Monero, where play takes it in Monero (K1).
    var paused: Bool { offer != nil }
}

// MARK: The playback delegate

/// iOS asks the picture's playback state as it would a video's. Till's
/// picture is live (an endless time range, so no scrubber) and "playing",
/// but for a cash sale offered in Monero, which shows play.
///
/// Review risk (decision 8): play doing something other than playing a
/// video is Till's own use of the control, which App Review may question.
/// ⤢ is the fallback: it opens the sale in Till, whose page has "Take it
/// in Monero". If review objects, drop `paused` for cash and keep ⤢.
extension PipWindow: @preconcurrency AVPictureInPictureSampleBufferPlaybackDelegate {
    func pictureInPictureController(_ pictureInPictureController: AVPictureInPictureController, setPlaying playing: Bool) {
        if playing, let offer = picture.offer {
            onTakeInMonero?(offer.orderId)
        }
        // A pause on a picture that isn't a video changes nothing.
        pictureInPictureController.invalidatePlaybackState()
    }

    func pictureInPictureControllerTimeRangeForPlayback(_ pictureInPictureController: AVPictureInPictureController) -> CMTimeRange {
        CMTimeRange(start: .negativeInfinity, duration: .positiveInfinity)
    }

    func pictureInPictureControllerIsPlaybackPaused(_ pictureInPictureController: AVPictureInPictureController) -> Bool {
        picture.paused
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        didTransitionToRenderSize newRenderSize: CMVideoDimensions
    ) {
        // iOS reports pixels; the frames are laid out in points.
        width = PipShape.width(renderWidth: newRenderSize.width, scale: PipWindow.scale)
        draw()
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        skipByInterval skipInterval: CMTime,
        completion completionHandler: @escaping () -> Void
    ) {
        completionHandler()
    }
}

// MARK: The controller's delegate

extension PipWindow: @preconcurrency AVPictureInPictureControllerDelegate {
    /// ⤢: Till comes to the front on the sale the window shows, the code
    /// ready for the customer (A2); from the strip, on the shift.
    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        restoreUserInterfaceForPictureInPictureStopWithCompletionHandler completionHandler: @escaping (Bool) -> Void
    ) {
        if let orderId = picture.orderId {
            onRestore?(orderId)
        }
        completionHandler(true)
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        failedToStartPictureInPictureWithError error: any Error
    ) {
        NSLog("till: Picture in Picture didn't start: %@", String(describing: error))
    }
}

/// The window's layer in the app's window, small and out of the way: iOS
/// starts Picture in Picture from a layer that's on screen, and animates
/// the window out of it.
struct PipSourceView: UIViewRepresentable {
    let pip: PipWindow

    func makeUIView(context: Context) -> PipLayerHost {
        PipLayerHost(layer: pip.layer)
    }

    func updateUIView(_ view: PipLayerHost, context: Context) {}
}

/// A view that keeps the display layer at its own size.
final class PipLayerHost: UIView {
    private let shown: CALayer

    init(layer: CALayer) {
        shown = layer
        super.init(frame: .zero)
        isUserInteractionEnabled = false
        self.layer.addSublayer(layer)
    }

    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        shown.frame = bounds
    }
}
