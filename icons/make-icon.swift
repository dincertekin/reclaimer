// Draws the Reclaimer app icon and writes every format we ship:
//   icon.png (1024 px, window/Dock icon), icon.icns (macOS), icon.ico (Windows).
//
// Usage (macOS):  swift icons/make-icon.swift icons
//
// Also writes .github/assets/banner.png (1280×640): the README banner and
// GitHub's social preview image.
//
// The mark: an R whose leg makes a U-turn and rises as an arrow — the
// letter itself "coming back". Drawn on a 1024-unit grid and rendered
// fresh at each size so small icons stay crisp.

import CoreGraphics
import CoreText
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

    // The mark, drawn as thick strokes with round ends.
    ctx.translateBy(x: -26, y: 0) // optical centring: the arrow adds weight on the right
    ctx.setStrokeColor(ink)
    ctx.setLineWidth(84)
    ctx.setLineCap(.round)
    ctx.setLineJoin(.round)

    // Stem and bowl of the R.
    let letter = CGMutablePath()
    letter.move(to: CGPoint(x: 352, y: 736))
    letter.addLine(to: CGPoint(x: 352, y: 296))
    letter.addLine(to: CGPoint(x: 500, y: 296))
    letter.addArc(center: CGPoint(x: 500, y: 404), radius: 108, startAngle: -.pi / 2, endAngle: .pi / 2, clockwise: false)
    letter.addLine(to: CGPoint(x: 352, y: 512))
    ctx.addPath(letter)
    ctx.strokePath()

    // The leg flows down into a U-turn, then rises straight up...
    let r: CGFloat = 84
    let turn = CGPoint(x: 600, y: 652)
    let leg = CGMutablePath()
    leg.move(to: CGPoint(x: 440, y: 512))
    leg.addQuadCurve(to: CGPoint(x: turn.x - r, y: turn.y), control: CGPoint(x: turn.x - r, y: 540))
    leg.addArc(center: turn, radius: r, startAngle: .pi, endAngle: 0, clockwise: true)
    leg.addLine(to: CGPoint(x: turn.x + r, y: 580))
    ctx.setLineCap(.butt)
    ctx.addPath(leg)
    ctx.strokePath()

    // ...into an arrowhead pointing back up, its corners softened.
    let tipX = turn.x + r
    let head = CGMutablePath()
    head.move(to: CGPoint(x: tipX, y: 476))
    head.addLine(to: CGPoint(x: tipX + 90, y: 592))
    head.addLine(to: CGPoint(x: tipX - 90, y: 592))
    head.closeSubpath()
    ctx.addPath(head)
    ctx.setFillColor(ink)
    ctx.setLineWidth(18)
    ctx.drawPath(using: .fillStroke)
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
// Banner: the icon beside the name and what it does, in Inter.
func banner() -> CGImage {
    let (w, h) = (1280, 640)
    let ctx = CGContext(
        data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.setFillColor(gray(255))
    ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))

    let fonts = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../assets/fonts")
    for file in ["Inter-Regular.ttf", "Inter-SemiBold.ttf"] {
        CTFontManagerRegisterFontsForURL(fonts.appendingPathComponent(file) as CFURL, .process, nil)
    }
    // CoreGraphics counts y from the bottom.
    ctx.draw(render(380), in: CGRect(x: 110, y: 130, width: 380, height: 380))
    func text(_ string: String, font: String, size: CGFloat, color: CGColor, x: CGFloat, baseline: CGFloat) {
        let attributes: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName(font as CFString, size, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): color,
        ]
        ctx.textPosition = CGPoint(x: x, y: baseline)
        CTLineDraw(CTLineCreateWithAttributedString(NSAttributedString(string: string, attributes: attributes)), ctx)
    }
    let x: CGFloat = 516
    text("Reclaimer", font: "Inter-SemiBold", size: 100, color: ink, x: x - 6, baseline: 368)
    text("Recover deleted photos, videos and files", font: "Inter-Regular", size: 30, color: gray(0x55), x: x, baseline: 300)
    text("from disks, SD cards, USB drives and phones.", font: "Inter-Regular", size: 30, color: gray(0x55), x: x, baseline: 260)
    text("Free  ·  Open source  ·  macOS, Windows, Linux", font: "Inter-Regular", size: 22, color: gray(0x8E), x: x, baseline: 204)
    return ctx.makeImage()!
}
let assets = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../.github/assets")
try png(banner()).write(to: assets.appendingPathComponent("banner.png"))

print("Wrote icon.png, icon.icns, icon.ico to \(out.path), and the README banner")
