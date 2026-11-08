import AVFoundation
import SwiftUI
import UIKit

/// The camera looking for QR codes (PadOnb2, docs/ios-plan.md §5): AVFoundation's
/// metadata output, no extra library. Each new code's text goes to `found`,
/// once; the engine decides whether it is a connect code. Shows nothing on a
/// device without a camera (the simulator).
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
        private let queue = DispatchQueue(label: "com.monokulo.till.camera")

        func start() {
            if !configured {
                configured = true
                guard let camera = AVCaptureDevice.default(for: .video),
                      let input = try? AVCaptureDeviceInput(device: camera),
                      session.canAddInput(input)
                else { return }
                session.addInput(input)
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
            queue.async { [self] in
                if session.isRunning { session.stopRunning() }
            }
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
