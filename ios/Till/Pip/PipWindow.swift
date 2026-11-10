@preconcurrency import AVFoundation
@preconcurrency import AVKit
import CoreMedia
import CoreVideo
import os
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
    /// A controller let go while its window was still closing: kept until
    /// iOS says it stopped, as one freed mid-animation can bring the app
    /// down.
    private var stopping: AVPictureInPictureController?
    /// Pixel buffers for the frames, reused while the size stays the same,
    /// so a countdown doesn't allocate a new surface each second.
    private var pool: (size: CGSize, pool: CVPixelBufferPool)?
    private var state: TillActivityAttributes.ContentState?
    private var cash: PipCashOffer?
    private var place = ""
    /// The window's width in points, as iOS last said.
    private var width: CGFloat?
    private var drawn: PipFrameKey?
    /// A frame is being drawn and enqueued. Enqueueing can call back into
    /// this class on the same stack: a frame of a new size makes the layer
    /// post its presentation size, and AVKit then reports the window's new
    /// render size (`didTransitionToRenderSize`). A draw from that callback
    /// waits for the next turn of the main run loop instead, as drawing
    /// there recursed until the main thread's stack ran out (the two crash
    /// reports of 2026-11-10, both inside `enqueueSampleBuffer`).
    private var drawing = false
    /// A draw is waiting for the next turn of the main run loop.
    private var redrawQueued = false
    private var ticker: Task<Void, Never>?
    private let timebase: CMTimebase?

    /// Pixels to a point: every iPad iPadOS 26 runs on draws at 2x.
    /// Pixels per point of the window iOS reports (an iPad's screen).
    static let displayScale: CGFloat = 2
    /// Frames are drawn at twice the window's pixel density, so a window
    /// larger than iOS last said (or a pinch between redraws) is shrunk to
    /// fit, never stretched: that was the blur on an iPad.
    static let scale: CGFloat = 4

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
            let said = String(describing: error)
            TillLog.pip.error("no audio session for Picture in Picture: \(said, privacy: .public)")
        }
        let left = MemoryHeadroom.described
        TillLog.pip.notice("floating window ready, memory left \(left, privacy: .public)")
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
            let was = controller.isPictureInPictureActive ? "up" : "down"
            TillLog.pip.notice("floating window off (was \(was, privacy: .public))")
            controller.canStartPictureInPictureAutomaticallyFromInline = false
            if controller.isPictureInPictureActive {
                stopping = controller
                controller.stopPictureInPicture()
            }
        }
        controller = nil
        drawn = nil
        pool = nil
        layer.sampleBufferRenderer.flush(removingDisplayedImage: true, completionHandler: nil)
        try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
    }

    // MARK: Frames

    /// iOS is short of memory: the buffers kept for the next frames go.
    func memoryWarning() {
        pool = nil
    }

    /// Draws the picture again if anything it shows changed. Nothing is
    /// drawn while Till is in the back without the window up: nobody sees
    /// the layer then, and its renderer is failed or holds what's queued.
    /// The window's start (`didStartPictureInPicture`) draws again.
    private func draw() {
        guard controller != nil else { return }
        if !isActive, UIApplication.shared.applicationState == .background { return }
        let picture = picture
        let now = Date()
        let size = picture.shape.size(width: width)
        let key = PipFrameKey(picture: picture, size: size, timeLeft: picture.timeLeft(now: now))
        guard key != drawn else { return }
        if drawing {
            redrawSoon()
            return
        }
        drawing = true
        defer { drawing = false }
        let renderer = layer.sampleBufferRenderer
        if renderer.status == .failed {
            // After going to the back (AVErrorOperationInterrupted, say): a
            // failed renderer drops what's enqueued until it's flushed.
            let said = renderer.error.map { String(describing: $0) } ?? "no error"
            TillLog.pip.error("frames failed: \(said, privacy: .public); flushing")
            renderer.flush()
        }
        guard renderer.isReadyForMoreMediaData else {
            // Its queue is full: the next tick tries again, rather than
            // piling up frames nobody sees.
            return
        }
        let pixels = pixelBuffer(size: size)
        let buffer = autoreleasepool {
            PipWindow.frame(PipFrameView(picture: picture, size: size, now: now), size: size, scale: PipWindow.scale, into: pixels, timebase: timebase)
        }
        guard let buffer else { return }
        // Marked before enqueueing, so a callback during the enqueue sees
        // this frame as drawn.
        drawn = key
        renderer.enqueue(buffer)
    }

    /// [draw] on the next turn of the main run loop, once however often it's
    /// asked for before then; never on the stack that asked.
    private func redrawSoon() {
        guard !redrawQueued else { return }
        redrawQueued = true
        Task { @MainActor [weak self] in
            guard let self else { return }
            self.redrawQueued = false
            self.draw()
        }
    }

    /// A pixel buffer for a frame of `size` points, from the pool kept for
    /// that size.
    private func pixelBuffer(size: CGSize) -> CVPixelBuffer? {
        if pool?.size != size {
            pool = PipWindow.makePool(width: Int((size.width * PipWindow.scale).rounded(.up)), height: Int((size.height * PipWindow.scale).rounded(.up))).map { (size: size, pool: $0) }
        }
        guard let pool = pool?.pool else { return nil }
        var buffer: CVPixelBuffer?
        let status = CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pool, &buffer)
        guard status == kCVReturnSuccess else {
            let code = Int(status)
            TillLog.pip.error("no pixel buffer from the pool: \(code, privacy: .public)")
            return nil
        }
        return buffer
    }

    private static func makePool(width: Int, height: Int) -> CVPixelBufferPool? {
        guard width > 0, height > 0 else { return nil }
        let attributes: [CFString: Any] = [
            kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey: width,
            kCVPixelBufferHeightKey: height,
            kCVPixelBufferCGImageCompatibilityKey: true,
            kCVPixelBufferCGBitmapContextCompatibilityKey: true,
            kCVPixelBufferIOSurfacePropertiesKey: [String: Any](),
        ]
        var pool: CVPixelBufferPool?
        CVPixelBufferPoolCreate(kCFAllocatorDefault, nil, attributes as CFDictionary, &pool)
        return pool
    }

    /// `view` as a sample buffer in `pixels` (or a buffer of its own when
    /// the pool gave none), shown as soon as it's enqueued.
    private static func frame(_ view: some View, size: CGSize, scale: CGFloat, into pixels: CVPixelBuffer?, timebase: CMTimebase?) -> CMSampleBuffer? {
        let renderer = ImageRenderer(content: view)
        renderer.proposedSize = ProposedViewSize(size)
        renderer.scale = scale
        renderer.isOpaque = true
        guard let image = renderer.cgImage else { return nil }
        guard let pixels = PipWindow.draw(image, into: pixels) else { return nil }
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

    /// `image` in a BGRA pixel buffer the display layer can show: `pooled`
    /// when it's the image's size, else one made for it.
    private static func draw(_ image: CGImage, into pooled: CVPixelBuffer?) -> CVPixelBuffer? {
        let width = image.width, height = image.height
        let buffer: CVPixelBuffer
        if let pooled, CVPixelBufferGetWidth(pooled) == width, CVPixelBufferGetHeight(pooled) == height {
            buffer = pooled
        } else {
            let attributes: [CFString: Any] = [
                kCVPixelBufferCGImageCompatibilityKey: true,
                kCVPixelBufferCGBitmapContextCompatibilityKey: true,
                kCVPixelBufferIOSurfacePropertiesKey: [String: Any](),
            ]
            var made: CVPixelBuffer?
            let status = CVPixelBufferCreate(kCFAllocatorDefault, width, height, kCVPixelFormatType_32BGRA, attributes as CFDictionary, &made)
            guard status == kCVReturnSuccess, let made else { return nil }
            buffer = made
        }
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
        let new = PipShape.width(renderWidth: newRenderSize.width, scale: PipWindow.displayScale)
        // A point or so either way is the frame's own rounding coming back:
        // drawing again for it would chase the size without end.
        if let width, abs(new - width) < 2 { return }
        width = new
        redrawSoon()
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
        TillLog.pip.notice("floating window: back to Till")
        if let orderId = picture.orderId {
            onRestore?(orderId)
        }
        completionHandler(true)
    }

    func pictureInPictureController(
        _ pictureInPictureController: AVPictureInPictureController,
        failedToStartPictureInPictureWithError error: any Error
    ) {
        let said = String(describing: error)
        TillLog.pip.error("floating window didn't start: \(said, privacy: .public)")
    }

    func pictureInPictureControllerDidStartPictureInPicture(_ pictureInPictureController: AVPictureInPictureController) {
        let left = MemoryHeadroom.described
        let frames = layer.sampleBufferRenderer.status == .failed ? "failed" : "rendering"
        TillLog.pip.notice("floating window up, memory left \(left, privacy: .public), frames \(frames, privacy: .public)")
        // Frames weren't drawn while Till was in the back without it.
        drawn = nil
        draw()
    }

    func pictureInPictureControllerDidStopPictureInPicture(_ pictureInPictureController: AVPictureInPictureController) {
        let left = MemoryHeadroom.described
        TillLog.pip.notice("floating window down, memory left \(left, privacy: .public)")
        if stopping === pictureInPictureController {
            stopping = nil
        }
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
