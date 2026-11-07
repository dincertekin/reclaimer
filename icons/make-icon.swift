// Draws the Reclaimer app icon and writes every format we ship:
//   icon.png (1024 px, window/Dock icon), icon.icns (macOS), icon.ico (Windows).
//
// Usage (macOS):  swift icons/make-icon.swift icons
//
// The mark: a black document reassembling from blocks — scattered sectors
// coming back together, which is what carving does. Drawn on a 1024-unit
// grid and rendered fresh at each size so small icons stay crisp.

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let ink = CGColor(srgbRed: 0x1D / 255, green: 0x1D / 255, blue: 0x1F / 255, alpha: 1)

func gray(_ v: CGFloat, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: v / 255, green: v / 255, blue: v / 255, alpha: alpha)
}

/// The macOS icon shape: straight sides, corners ~22% of the width.
func squircle(_ r: CGRect) -> CGPath {
    CGPath(roundedRect: r, cornerWidth: r.width * 0.2237, cornerHeight: r.width * 0.2237, transform: nil)
}

func render(_ size: Int) -> CGImage {
    let ctx = CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let s = CGFloat(size) / 1024
    ctx.scaleBy(x: s, y: s)
    ctx.translateBy(x: 0, y: 1024)
    ctx.scaleBy(x: 1, y: -1) // top-down coordinates from here on

    // Body: white squircle on Apple's 824-unit icon grid, with a soft shadow.
    let body = squircle(CGRect(x: 100, y: 100, width: 824, height: 824))
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -10 * s), blur: 24 * s, color: gray(0, 0.30))
    ctx.addPath(body)
    ctx.setFillColor(gray(255))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(body)
    ctx.clip()
    let shade = CGGradient(colorsSpace: nil, colors: [gray(255), gray(236)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(shade, start: CGPoint(x: 0, y: 100), end: CGPoint(x: 0, y: 924), options: [])
    ctx.restoreGState()
    ctx.addPath(body)
    ctx.setStrokeColor(gray(0, 0.08))
    ctx.setLineWidth(2)
    ctx.strokePath()

    // The document: 6 × 8 cells; its left column is broken into blocks.
    let cell: CGFloat = 56
    let (left, top) = (CGFloat(428), CGFloat(288))
    let right = left + 6 * cell, bottom = top + 8 * cell
    let fold: CGFloat = 120

    let page = CGMutablePath()
    page.move(to: CGPoint(x: left + cell - 1, y: top))
    page.addLine(to: CGPoint(x: right - fold, y: top))
    page.addLine(to: CGPoint(x: right, y: top + fold))
    page.addLine(to: CGPoint(x: right, y: bottom))
    page.addLine(to: CGPoint(x: left + cell - 1, y: bottom))
    page.closeSubpath()
    for row in [0, 1, 3, 4, 6, 7] { // column 0, with gaps where blocks are missing
        page.addRect(CGRect(x: left, y: top + CGFloat(row) * cell, width: cell, height: cell))
    }
    ctx.addPath(page)
    ctx.setFillColor(ink)
    ctx.fillPath()

    // Folded corner.
    let corner = CGMutablePath()
    corner.move(to: CGPoint(x: right - fold, y: top))
    corner.addLine(to: CGPoint(x: right - fold, y: top + fold))
    corner.addLine(to: CGPoint(x: right, y: top + fold))
    corner.closeSubpath()
    ctx.addPath(corner)
    ctx.setFillColor(gray(0x8E))
    ctx.fillPath()

    // Blocks on their way back, fainter the further out they are.
    let inset: CGFloat = 7
    let blocks: [(col: Int, rows: [Int], alpha: CGFloat)] = [(-1, [2, 5, 7], 0.85), (-2, [1, 4], 0.5), (-3, [3, 6], 0.22)]
    for (col, rows, alpha) in blocks {
        for row in rows {
            let r = CGRect(x: left + CGFloat(col) * cell, y: top + CGFloat(row) * cell, width: cell, height: cell)
            ctx.setFillColor(ink.copy(alpha: alpha)!)
            ctx.fill(r.insetBy(dx: inset, dy: inset))
        }
    }
    return ctx.makeImage()!
}

func png(_ image: CGImage) -> Data {
    let data = NSMutableData()
    let dest = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
    return data as Data
}

let out = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ".")
try png(render(1024)).write(to: out.appendingPathComponent("icon.png"))

// macOS: an .iconset folder, packed by iconutil.
let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("Reclaimer.iconset")
try? FileManager.default.removeItem(at: iconset)
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try png(render(base)).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try png(render(base * 2)).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", out.appendingPathComponent("icon.icns").path]
try iconutil.run()
iconutil.waitUntilExit()

// Windows: an .ico holding PNGs (supported since Vista).
let sizes = [16, 24, 32, 48, 64, 128, 256]
let images = sizes.map { png(render($0)) }
var ico = Data([0, 0, 1, 0, UInt8(sizes.count), 0])
var offset = 6 + 16 * sizes.count
func le<T: FixedWidthInteger>(_ v: T) -> Data { withUnsafeBytes(of: v.littleEndian) { Data($0) } }
for (size, image) in zip(sizes, images) {
    let side = UInt8(size == 256 ? 0 : size) // 0 means 256
    ico.append(contentsOf: [side, side, 0, 0])
    ico.append(le(UInt16(1)) + le(UInt16(32)) + le(UInt32(image.count)) + le(UInt32(offset)))
    offset += image.count
}
images.forEach { ico.append($0) }
try ico.write(to: out.appendingPathComponent("icon.ico"))
print("Wrote icon.png, icon.icns, icon.ico to \(out.path)")
