import AVFoundation
import SwiftUI
import UIKit

/// The camera looking for QR codes (PadOnb2, docs/ios-plan.md §5): AVFoundation's
/// metadata output, no extra library. Each new code's text goes to `found`,
/// once; the engine decides whether it is a connect code. The picture
/// follows the device's rotation (`AVCaptureDevice.RotationCoordinator`), so
/// a turned iPad shows it upright. Shows nothing on a device without a
/// camera (the simulator).
struct QRScanner: UIViewRepresentable {
    let found: @MainActor (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> PreviewView {
        let view = PreviewView()
        view.backgroundColor = .clear
        view.previewLayer.videoGravity = .resizeAspectFill
        view.previewLayer.session = context.coordinator.session
        context.coordinator.found = found
        context.coordinator.start()
        context.coordinator.followRotation(of: view.previewLayer)
        return view
    }

    func updateUIView(_ uiView: PreviewView, context: Context) {
        context.coordinator.found = found
    }

    static func dismantleUIView(_ uiView: PreviewView, coordinator: Coordinator) {
        coordinator.stop()
    }

    /// A view whose layer is the camera's picture.
    final class PreviewView: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }

        var previewLayer: AVCaptureVideoPreviewLayer {
            // The layer is always of `layerClass`.
            layer as! AVCaptureVideoPreviewLayer
        }
    }

    /// Owns the capture session, and hears the metadata output on the main
    /// queue. The session starts and stops on a queue of its own, as
    /// `startRunning` blocks.
    final class Coordinator: NSObject, AVCaptureMetadataOutputObjectsDelegate, @unchecked Sendable {
        let session = AVCaptureSession()
        var found: (@MainActor (String) -> Void)?
        private var last: String?
        private var configured = false
        private var camera: AVCaptureDevice?
        /// Keeps the preview upright as the device turns, and its observation.
        private var rotation: AVCaptureDevice.RotationCoordinator?
        private var rotationObservation: NSKeyValueObservation?
        private weak var previewLayer: AVCaptureVideoPreviewLayer?
        private let queue = DispatchQueue(label: "com.monokulo.till.camera")

        func start() {
            if !configured {
                configured = true
                guard let camera = AVCaptureDevice.default(for: .video),
                      let input = try? AVCaptureDeviceInput(device: camera),
                      session.canAddInput(input)
                else { return }
                session.addInput(input)
                self.camera = camera
                let output = AVCaptureMetadataOutput()
                guard session.canAddOutput(output) else { return }
                session.addOutput(output)
                output.setMetadataObjectsDelegate(self, queue: .main)
                output.metadataObjectTypes = [.qr]
            }
            queue.async { [self] in
                if !session.isRunning { session.startRunning() }
            }
        }

        func stop() {
            rotationObservation?.invalidate()
            rotationObservation = nil
            rotation = nil
            queue.async { [self] in
                if session.isRunning { session.stopRunning() }
            }
        }

        /// Turns `layer`'s picture to the horizon-level angle the rotation
        /// coordinator gives, now and each time the device turns. The
        /// coordinator reports on the main queue.
        @MainActor
        func followRotation(of layer: AVCaptureVideoPreviewLayer) {
            guard let camera, rotation == nil else { return }
            let coordinator = AVCaptureDevice.RotationCoordinator(device: camera, previewLayer: layer)
            rotation = coordinator
            previewLayer = layer
            turnPreview(to: coordinator.videoRotationAngleForHorizonLevelPreview)
            rotationObservation = coordinator.observe(\.videoRotationAngleForHorizonLevelPreview, options: [.new]) { [weak self] coordinator, _ in
                let angle = coordinator.videoRotationAngleForHorizonLevelPreview
                MainActor.assumeIsolated {
                    self?.turnPreview(to: angle)
                }
            }
        }

        @MainActor
        private func turnPreview(to angle: CGFloat) {
            guard let connection = previewLayer?.connection, connection.isVideoRotationAngleSupported(angle) else { return }
            connection.videoRotationAngle = angle
        }

        func metadataOutput(
            _ output: AVCaptureMetadataOutput,
            didOutput metadataObjects: [AVMetadataObject],
            from connection: AVCaptureConnection
        ) {
            let texts = metadataObjects.compactMap { ($0 as? AVMetadataMachineReadableCodeObject)?.stringValue }
            guard let text = texts.first, text != last else { return }
            last = text
            // The delegate's queue is the main queue.
            MainActor.assumeIsolated {
                found?(text)
            }
        }
    }
}
