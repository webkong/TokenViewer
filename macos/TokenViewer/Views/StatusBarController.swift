import SwiftUI

@MainActor
final class StatusBarController {
    private var statusItem: NSStatusItem?
    private var popover: NSPopover?
    private var mainWindow: NSWindow?
    private var eventMonitor: Any?
    private var localMonitor: Any?

    init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem?.button {
            button.image = makeMenuBarIcon()
            button.action = #selector(togglePopover)
            button.target = self
        }

        let popover = NSPopover()
        popover.contentSize = NSSize(width: 420, height: 620)
        popover.behavior = .transient
        var hostedController: NSHostingController<PopoverView>?
        hostedController = NSHostingController(
            rootView: PopoverView(
                onOpenMainWindow: { [weak self] in self?.openMainWindow() },
                onClose: { [weak self] in self?.close() },
                onHeightChange: { [weak popover] height in
                    popover?.contentSize = NSSize(width: 420, height: height)
                    hostedController?.preferredContentSize = NSSize(width: 420, height: height)
                }
            )
        )
        hostedController?.preferredContentSize = NSSize(width: 420, height: 620)
        hostedController?.view.wantsLayer = true
        hostedController?.view.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        popover.contentViewController = hostedController
        self.popover = popover
    }

    @objc private func togglePopover() {
        guard let button = statusItem?.button, let popover else { return }
        if popover.isShown {
            close()
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            NSApp.activate(ignoringOtherApps: true)
            if let window = popover.contentViewController?.view.window {
                window.level = .floating
                window.isOpaque = true
                window.backgroundColor = .windowBackgroundColor
            }
            startEventMonitor()
            // Trigger sync here (AppKit) rather than in PopoverView.onAppear:
            // NSPopover reuses the same NSHostingController, so SwiftUI's
            // onAppear does not reliably fire on subsequent opens. Throttled via
            // syncIfStale so frequent reopens don't keep pulling; the panel's
            // refresh button forces a sync.
            UsageViewModel.shared.syncIfStale()
            LimitsViewModel.shared.refreshIfStale()
        }
    }

    private func makeMenuBarIcon() -> NSImage {
        let size = NSSize(width: 18, height: 18)
        let img = NSImage(size: size, flipped: false) { rect in
            NSColor.black.setFill()
            // Crossbar
            let bar = NSBezierPath(roundedRect: NSRect(x: 1, y: 13, width: 16, height: 3.5), xRadius: 1.5, yRadius: 1.5)
            bar.fill()
            // Stem: 3 stacked segments
            for (i, h): (Int, CGFloat) in [(0, 3.0), (1, 3.0), (2, 2.5)] {
                let y = 8.5 - CGFloat(i) * 4.0
                let seg = NSBezierPath(roundedRect: NSRect(x: 7, y: y, width: 4, height: h), xRadius: 1, yRadius: 1)
                seg.fill()
            }
            return true
        }
        img.isTemplate = true
        return img
    }

    private func close() {
        stopEventMonitor()
    }

    private func startEventMonitor() {
        stopEventMonitor()
        eventMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            self?.close()
        }
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            if event.keyCode == 53 { // ESC
                self?.close()
                return nil
            }
            return event
        }
    }

    private func stopEventMonitor() {
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
        if let monitor = localMonitor {
            NSEvent.removeMonitor(monitor)
            localMonitor = nil
        }
    }

    func openMainWindow() {
        popover?.performClose(nil)

        if let window = mainWindow {
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 700, height: 520),
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Token Viewer"
        window.center()
        window.contentView = NSHostingView(rootView: MainWindowView())
        window.isReleasedWhenClosed = false
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.mainWindow = window
    }
}
