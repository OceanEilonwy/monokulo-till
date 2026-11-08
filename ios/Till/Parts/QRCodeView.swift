import CoreGraphics
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

/// A QR code's modules: `size` by `size`, row by row from the top, true
/// for a dark module. No quiet zone; the view's card is that.
struct QRMatrix: Equatable, Sendable {
    let size: Int
    let modules: [Bool]

    subscript(x: Int, y: Int) -> Bool { modules[y * size + x] }

    /// [text]'s code with error correction M, as Android's `ui/Qr.kt` makes
    /// it, from Core Image's `CIQRCodeGenerator` (docs/ios-plan.md §5).
    /// Nil only if Core Image fails.
    init?(text: String) {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let extent = output.extent.integral
        guard let image = CIContext().createCGImage(output, from: extent) else { return nil }

        // One pixel a module: read it as grey, then crop to the dark modules,
        // whose bounds are the symbol's (the finder patterns are its corners),
        // whatever quiet zone the generator added.
        let width = image.width
        let height = image.height
        var pixels = [UInt8](repeating: 255, count: width * height)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width, space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue
            ) else { return false }
            context.interpolationQuality = .none
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }

        var minX = width, minY = height, maxX = -1, maxY = -1
        for y in 0..<height {
            for x in 0..<width where pixels[y * width + x] < 128 {
                minX = min(minX, x)
                maxX = max(maxX, x)
                minY = min(minY, y)
                maxY = max(maxY, y)
            }
        }
        guard maxX >= minX, maxX - minX == maxY - minY else { return nil }
        let size = maxX - minX + 1
        var modules = [Bool](repeating: false, count: size * size)
        for y in 0..<size {
            for x in 0..<size {
                modules[y * size + x] = pixels[(minY + y) * width + minX + x] < 128
            }
        }
        self.size = size
        self.modules = modules
    }

    /// The code as an image, one pixel a module, ink on white: to be scaled
    /// up with no smoothing.
    func image(ink: UInt32 = 0x1A1917) -> CGImage? {
        let r = UInt8((ink >> 16) & 0xFF), g = UInt8((ink >> 8) & 0xFF), b = UInt8(ink & 0xFF)
        var pixels = [UInt8](repeating: 255, count: size * size * 4)
        for i in 0..<(size * size) where modules[i] {
            pixels[i * 4] = r
            pixels[i * 4 + 1] = g
            pixels[i * 4 + 2] = b
        }
        return pixels.withUnsafeMutableBytes { buffer -> CGImage? in
            CGContext(
                data: buffer.baseAddress, width: size, height: size, bitsPerComponent: 8,
                bytesPerRow: size * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            )?.makeImage()
        }
    }
}

/// A payment code on its white card (the design's QR part): ink on white
/// whatever the theme, the card's padding its quiet zone, the modules
/// scaled up crisp with no smoothing.
struct QRCodeView: View {
    let text: String
    /// What VoiceOver says for the code.
    let label: String
    /// The card's side.
    var size: CGFloat
    /// The white round the code, at least the four modules a scanner wants at a phone's sizes.
    var padding: CGFloat = 20
    var cornerRadius: CGFloat = 32

    private let image: CGImage?

    init(text: String, label: String, size: CGFloat, padding: CGFloat = 20, cornerRadius: CGFloat = 32) {
        self.text = text
        self.label = label
        self.size = size
        self.padding = padding
        self.cornerRadius = cornerRadius
        self.image = QRMatrix(text: text)?.image()
    }

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                .fill(Palette.codeGround)
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .interpolation(.none)
                    .antialiased(false)
                    .aspectRatio(1, contentMode: .fit)
                    .padding(padding)
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityAddTraits(.isImage)
    }
}
