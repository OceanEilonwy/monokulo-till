import CoreGraphics
import Foundation
import SwiftUI
import Testing
@testable import Till

/// Checks of the look's pure pieces: the palette's variants, the Till
/// logo's states and colours against the TillWidget and TillRound boards,
/// the status glyphs, the QR code's modules, and the bundled font.
struct LookTests {
    private func close(_ a: Double, _ b: Double) -> Bool { abs(a - b) < 1e-9 }

    @Test func darkModesGroundIsBlackWithWarmCards() {
        let ground = { (dark: Bool) in Palette.hex(dark: dark, contrast: false, light: 0xF4F1EA, darkHex: 0x000000, lightContrast: nil, darkContrast: nil) }
        #expect(ground(false) == 0xF4F1EA)
        #expect(ground(true) == 0x000000)
    }

    @Test func increaseContrastPicksTheStrongerVariantWhereThereIsOne() {
        // The secondary text: #5A564C, #3F3B33 under Increase Contrast.
        #expect(Palette.hex(dark: false, contrast: true, light: 0x5A564C, darkHex: 0xA8A29A, lightContrast: 0x3F3B33, darkContrast: 0xC9C3B8) == 0x3F3B33)
        #expect(Palette.hex(dark: true, contrast: true, light: 0x5A564C, darkHex: 0xA8A29A, lightContrast: 0x3F3B33, darkContrast: 0xC9C3B8) == 0xC9C3B8)
        #expect(Palette.hex(dark: false, contrast: false, light: 0x5A564C, darkHex: 0xA8A29A, lightContrast: 0x3F3B33, darkContrast: 0xC9C3B8) == 0x5A564C)
        // A colour without one stays as it is.
        #expect(Palette.hex(dark: true, contrast: true, light: 0xFFFFFF, darkHex: 0x1C1B19, lightContrast: nil, darkContrast: nil) == 0x1C1B19)
    }

    @Test func theLogosStepsFollowAPayment() {
        #expect(TillLogoState.of(step: 1) == .waiting)
        #expect(TillLogoState.of(step: 2) == .seen)
        #expect(TillLogoState.of(step: 3) == .confirming)
        #expect(TillLogoState.of(step: 4) == .paid)
        #expect(TillLogoState.idle.progress == 0)
        #expect(TillLogoState.waiting.progress == 0)
        #expect(close(Double(TillLogoState.seen.progress), 1.0 / 3))
        #expect(close(Double(TillLogoState.confirming.progress), 2.0 / 3))
        #expect(TillLogoState.paid.progress == 1)
        #expect(TillLogoState.paid.label == "Till, paid")
    }

    @Test func theKeysLightOneByOneAndAllTurnGreenPaid() {
        let dim: UInt32 = 0x4A4740
        #expect(TillLogoColours(state: .waiting, dark: false).keys == [dim, dim, dim])
        #expect(TillLogoColours(state: .seen, dark: false).keys == [0xFF6600, dim, dim])
        #expect(TillLogoColours(state: .confirming, dark: false).keys == [0xFF6600, 0xFF6600, dim])
        #expect(TillLogoColours(state: .paid, dark: false).keys == [0x2EA052, 0x2EA052, 0x2EA052])
        #expect(TillLogoColours(state: .idle, dark: false).keys == [dim, dim, dim])
    }

    @Test func theScreenIsOrangeGreyIdleAndGreenPaid() {
        let waiting = TillLogoColours(state: .waiting, dark: false)
        #expect([waiting.mid, waiting.warm, waiting.deep] == [0xFF6600, 0xE85D00, 0xC24E00])
        let idle = TillLogoColours(state: .idle, dark: false)
        #expect([idle.mid, idle.warm, idle.deep] == [0x7F7F7F, 0x747474, 0x616161])
        let paid = TillLogoColours(state: .paid, dark: false)
        #expect([paid.mid, paid.warm, paid.deep] == [0x2EA052, 0x238A43, 0x1A7F37])
        #expect(paid.barFill == 0x2EA052)
        #expect(waiting.barFill == 0xFF6600)
    }

    @Test func theDarkLogoHasALighterBodyAndDimKeys() {
        let dark = TillLogoColours(state: .seen, dark: true)
        #expect(dark.body == 0x3A3733)
        #expect(dark.dim == 0x66615A)
        #expect(dark.widgetOutline == 0x1E1E1E)
        #expect(dark.roundOutline == 0x1A1917)
        let light = TillLogoColours(state: .seen, dark: false)
        #expect(light.body == 0x1A1917)
        #expect(light.widgetOutline == 0xF4F1EA)
        #expect(light.roundOutline == 0x4A4740)
    }

    @Test func theRoundLogosRingStartsAtTheTopAndGoesRound() {
        let ring = TillLogoGeometry.outlineFromTop()
        #expect(ring.boundingRect.insetBy(dx: -0.01, dy: -0.01).contains(TillLogoGeometry.outline))
        let start = ring.trimmedPath(from: 0, to: 0.001).boundingRect
        #expect(abs(start.minX - 48) < 0.5)
        #expect(abs(start.minY - 27) < 0.5)
        // Every facet lies on the screen.
        let screen = TillLogoGeometry.screen.insetBy(dx: -0.01, dy: -0.01)
        for facet in TillLogoGeometry.facets {
            var path = Path()
            path.addLines(facet)
            #expect(screen.contains(path.boundingRect))
        }
    }

    @Test func theWaitingKeyBlinksUpAndDown() {
        #expect(close(BlinkTiming.opacity(0), 0))
        #expect(close(BlinkTiming.opacity(0.6), 1))
        #expect(close(BlinkTiming.opacity(1.2), 0))
        #expect(close(BlinkTiming.opacity(0.3), 0.5))
    }

    @Test func greenIsForPaidAlone() {
        #expect(StatusGlyphKind.of(.ring, step: 1) == .waiting)
        #expect(StatusGlyphKind.of(.ring, step: 2) == .seen)
        #expect(StatusGlyphKind.of(.ring, step: 3) == .seen)
        #expect(StatusGlyphKind.of(.tick, step: 4) == .paid)
        #expect(StatusGlyphKind.of(.clock, step: 1) == .closed)
        #expect(StatusGlyphKind.paid.colour == Palette.paid)
        #expect(StatusGlyphKind.seen.colour == Palette.progress)
        #expect(StatusGlyphKind.waiting.colour == Palette.secondary)
        #expect(StatusGlyphKind.watching.symbol == nil)
        #expect(StatusGlyphKind.paid.symbol == "checkmark.circle.fill")
    }

    @Test func aShortTextIsAVersionOneCode() throws {
        let matrix = try #require(QRMatrix(text: "HELLO"))
        #expect(matrix.size == 21)
        // The finder pattern's dark corner modules, top left, top right, bottom left.
        #expect(matrix[0, 0] && matrix[20, 0] && matrix[0, 20])
    }

    @Test func aPaymentAddressIsAWholeCodeWithAnImage() throws {
        let matrix = try #require(QRMatrix(text: PartsView.sampleURI))
        #expect(matrix.size > 21)
        #expect((matrix.size - 17) % 4 == 0)
        let image = try #require(matrix.image())
        #expect(image.width == matrix.size)
        #expect(image.height == matrix.size)
    }

    @Test func manropeIsBundled() {
        #expect(Manrope.url != nil)
        #expect(Manrope.isAvailable)
    }
}
