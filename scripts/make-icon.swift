// Renders the MQTT Panel app icon (1024 x 1024, transparent corners) in the mqtt-panel.com family:
// a flat slate tile, two paper cards and a three-row topic tree with a clay dot as the live value.
// Run from the repo root:  xcrun swift scripts/make-icon.swift
// Then:  cd apps/desktop/src-tauri && cargo tauri icon icons/app-icon.png
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let S: CGFloat = 1024
let out = "apps/desktop/src-tauri/icons/app-icon.png"

func hex(_ v: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((v >> 16) & 255) / 255, green: CGFloat((v >> 8) & 255) / 255, blue: CGFloat(v & 255) / 255, alpha: a)
}
func rr(_ r: CGRect, _ c: CGFloat) -> CGPath { CGPath(roundedRect: r, cornerWidth: c, cornerHeight: c, transform: nil) }

// mqtt-panel.com family: paper #F3F1EC, sage #5E7A6E (Image Simple), clay #9A6E43 (PDF Simple), slate #56697A (this app)
let slate = hex(0x56697A), paper = hex(0xF3F1EC), node = hex(0x3F4F5C), pill = hex(0xB9C3CB), clay = hex(0x9A6E43)

let cs = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(data: nil, width: Int(S), height: Int(S), bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.translateBy(x: 0, y: S); ctx.scaleBy(x: 1, y: -1) // top-left origin

// Tile: the macOS rounded square with the usual margin (transparent outside)
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
ctx.setFillColor(slate); ctx.addPath(rr(tile, 184)); ctx.fillPath()

// Rear card (muted paper) and front card, centred as a group
let backR = CGRect(x: 0, y: 0, width: 470, height: 380)
let frontR = CGRect(x: -106, y: 160, width: 440, height: 384)
let group = backR.union(frontR)
let dx = (S - group.width) / 2 - group.minX, dy = (S - group.height) / 2 - group.minY
let back = backR.offsetBy(dx: dx, dy: dy), front = frontR.offsetBy(dx: dx, dy: dy)
ctx.setFillColor(hex(0xF3F1EC, 0.38)); ctx.addPath(rr(back, 48)); ctx.fillPath()
ctx.setFillColor(slate); ctx.addPath(rr(front.insetBy(dx: -22, dy: -22), 68)); ctx.fillPath() // soft gap
ctx.setFillColor(paper); ctx.addPath(rr(front, 46)); ctx.fillPath()

// Topic tree: three rows, each one step deeper; the last row ends in the clay dot (the live value)
let rowH: CGFloat = 40, step: CGFloat = 48, gap: CGFloat = 96
let x0 = front.minX + 56, y0 = front.minY + 70
let rows: [(indent: CGFloat, width: CGFloat)] = [(0, 176), (1, 212), (2, 128)]
for (i, r) in rows.enumerated() {
    let y = y0 + CGFloat(i) * gap
    let x = x0 + r.indent * step
    ctx.setFillColor(node); ctx.fillEllipse(in: CGRect(x: x, y: y + 6, width: 28, height: 28))
    ctx.setFillColor(pill); ctx.addPath(rr(CGRect(x: x + 48, y: y, width: r.width, height: rowH), rowH / 2)); ctx.fillPath()
    if i == rows.count - 1 {
        ctx.setFillColor(clay); ctx.fillEllipse(in: CGRect(x: x + 48 + r.width + 28, y: y + 2, width: 36, height: 36))
    }
}

let img = ctx.makeImage()!
let url = URL(fileURLWithPath: out)
let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, img, nil)
CGImageDestinationFinalize(dest)
print("wrote \(out)")
