import CoreGraphics
import Foundation
import Testing
@testable import Till

/// Phase I1's checks of the parts' pure pieces: the loading mark's timing
/// against Mark.kt's (`glintAlpha`, `coreAlpha`, `GLINT_MS`, `CORE_MS`),
/// the mark's path data, the QR code's modules, and the bundled font.
struct LookTests {
    private func close(_ a: Double, _ b: Double) -> Bool { abs(a - b) < 1e-9 }

    @Test func aGlintComesInHoldsAndFadesByTwelvePercent() {
        #expect(close(LoadingMarkTiming.glintAlpha(0), 0))
        #expect(close(LoadingMarkTiming.glintAlpha(0.005), 0.475))
        #expect(close(LoadingMarkTiming.glintAlpha(0.01), 0.95))
        #expect(close(LoadingMarkTiming.glintAlpha(0.05), 0.95))
        #expect(close(LoadingMarkTiming.glintAlpha(0.105), 0.475))
        #expect(close(LoadingMarkTiming.glintAlpha(0.12), 0))
        #expect(close(LoadingMarkTiming.glintAlpha(0.5), 0))
    }

    @Test func theCorePulsesInTheSecondHalfOfItsTurn() {
        #expect(close(LoadingMarkTiming.coreAlpha(0.2), 0))
        #expect(close(LoadingMarkTiming.coreAlpha(0.435), 0.475))
        #expect(close(LoadingMarkTiming.coreAlpha(0.6), 0.95))
        #expect(close(LoadingMarkTiming.coreAlpha(0.87), 0.475))
        #expect(close(LoadingMarkTiming.coreAlpha(0.98), 0))
    }

    @Test func theLoopsTurnEvery4200And700Milliseconds() {
        #expect(LoadingMarkTiming.glintMs == 4200)
        #expect(LoadingMarkTiming.coreMs == 700)
        #expect(close(LoadingMarkTiming.glintProgress(timeMs: 2100, delayMs: 0), 0.5))
        #expect(close(LoadingMarkTiming.glintProgress(timeMs: 4200 + 2100, delayMs: 0), 0.5))
        // Before its delay, a glint is late in the turn before.
        #expect(close(LoadingMarkTiming.glintProgress(timeMs: 0, delayMs: 2100), 0.5))
        #expect(close(LoadingMarkTiming.coreProgress(timeMs: 700 * 3 + 350), 0.5))
    }

    @Test func theSpokesAreMarkKtsStaggeredOverOneTurn() {
        let delays = MarkGeometry.spokes.map(\.delayMs)
        #expect(delays.count == 17)
        #expect(delays.first == 0)
        #expect(delays.last == 3591)
        #expect(delays == delays.sorted())
        #expect(MarkGeometry.facets.count == 31)
    }

    @Test func pathDataIsReadAsAbsoluteMovesAndLines() {
        let triangle = MarkGeometry.path("M0,0L10,0L10,5Z")
        #expect(triangle.boundingRect == CGRect(x: 0, y: 0, width: 10, height: 5))
        // Every facet lies in the lens's square.
        let lens = CGRect(x: 6, y: 4, width: 48, height: 48).insetBy(dx: -0.01, dy: -0.01)
        for facet in MarkGeometry.facets {
            let bounds = MarkGeometry.path(facet.d).boundingRect
            #expect(!bounds.isEmpty)
            #expect(lens.contains(bounds))
        }
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
