import AppKit

/// メニューバー用アイコンの生成。アプリアイコン（"C"の円弧＋中央の音波バー）を踏襲し、
/// テンプレート画像として描画することでライト/ダーク双方のメニューバーに自動追従させる。
enum StatusIcon {
    static func make() -> NSImage {
        let size = NSSize(width: 18, height: 18)
        let image = NSImage(size: size, flipped: false) { rect in
            NSColor.black.set()

            let center = NSPoint(x: rect.midX, y: rect.midY)
            let arc = NSBezierPath()
            arc.appendArc(
                withCenter: center, radius: 7.5,
                startAngle: 35, endAngle: 325, clockwise: false
            )
            arc.lineWidth = 2.1
            arc.lineCapStyle = .round
            arc.stroke()

            let barWidth: CGFloat = 1.6
            let heights: [CGFloat] = [3.5, 7.5, 5.5, 3]
            let spacing: CGFloat = 2.3
            var x = center.x - CGFloat(heights.count - 1) * spacing / 2
            for height in heights {
                let barRect = NSRect(
                    x: x - barWidth / 2, y: center.y - height / 2,
                    width: barWidth, height: height
                )
                NSBezierPath(roundedRect: barRect, xRadius: barWidth / 2, yRadius: barWidth / 2).fill()
                x += spacing
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Ceyrad"
        return image
    }
}
