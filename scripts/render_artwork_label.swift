#!/usr/bin/env swift

import AppKit
import Foundation

guard CommandLine.arguments.count == 4 else {
    fputs("usage: render_artwork_label.swift INPUT OUTPUT TITLE\n", stderr)
    exit(2)
}

let input = URL(fileURLWithPath: CommandLine.arguments[1])
let output = URL(fileURLWithPath: CommandLine.arguments[2])
let title = CommandLine.arguments[3]
guard let source = NSImage(contentsOf: input) else {
    fputs("cannot read input image: \(input.path)\n", stderr)
    exit(1)
}

let canvas = NSSize(width: 1_254, height: 1_254)
guard let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: Int(canvas.width),
    pixelsHigh: Int(canvas.height),
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
) else {
    fputs("cannot allocate output bitmap\n", stderr)
    exit(1)
}
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
NSGraphicsContext.current?.imageInterpolation = .high
source.draw(in: NSRect(origin: .zero, size: canvas),
            from: NSRect(origin: .zero, size: source.size),
            operation: .copy,
            fraction: 1)

// Spotify's own square-cover typography reads as a substantial lower-left
// title block at library-thumbnail sizes. Keep the label deterministic and
// anchor its measured height to the bottom instead of drawing it at the top of
// an oversized fixed rectangle.
let fontSize: CGFloat = title.count > 24 ? 116 : 132
let paragraph = NSMutableParagraphStyle()
paragraph.lineBreakMode = .byWordWrapping
paragraph.lineSpacing = -12
let shadow = NSShadow()
shadow.shadowColor = NSColor.black.withAlphaComponent(0.48)
shadow.shadowBlurRadius = 16
shadow.shadowOffset = NSSize(width: 0, height: -3)
let attributes: [NSAttributedString.Key: Any] = [
    .font: NSFont(name: "HelveticaNeue-Bold", size: fontSize)
        ?? NSFont.boldSystemFont(ofSize: fontSize),
    .foregroundColor: NSColor.white,
    .paragraphStyle: paragraph,
    .shadow: shadow,
    .kern: -2.0,
]
let label = NSAttributedString(string: title, attributes: attributes)
let labelWidth: CGFloat = 1_100
let measured = label.boundingRect(
    with: NSSize(width: labelWidth, height: .greatestFiniteMagnitude),
    options: [.usesLineFragmentOrigin, .usesFontLeading]
)
let bounds = NSRect(x: 54, y: 42, width: labelWidth, height: ceil(measured.height) + 4)
label.draw(with: bounds, options: [.usesLineFragmentOrigin, .usesFontLeading])
NSGraphicsContext.restoreGraphicsState()

guard let png = bitmap.representation(using: .png, properties: [:]) else {
    fputs("cannot encode output image\n", stderr)
    exit(1)
}
try png.write(to: output, options: .atomic)
