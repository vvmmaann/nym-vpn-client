import WidgetKit
import NetworkExtension

class VPNStatusTimelineProvider: TimelineProvider {
    typealias Entry = VPNStatusTimelineEntry

    private static let groupDefaults = UserDefaults(suiteName: "group.net.nymtech.vpn")
    #if os(iOS)
    private static let entryKey = "ios_widgetEntryLocation"
    private static let exitKey = "ios_widgetExitLocation"
    #elseif os(macOS)
    private static let entryKey = "macos_widgetEntryLocation"
    private static let exitKey = "macos_widgetExitLocation"
    #endif

    func placeholder(in context: Context) -> VPNStatusTimelineEntry {
        VPNStatusTimelineEntry(
            date: Date(),
            status: .status(.connected),
            entryLocation: "Switzerland",
            exitLocation: "France"
        )
    }

    func getSnapshot(
        in context: Context,
        completion: @escaping (VPNStatusTimelineEntry) -> Void
    ) {
        let entry = VPNStatusTimelineEntry(
            date: Date(),
            status: .status(.connected),
            entryLocation: "Germany",
            exitLocation: "France"
        )
        completion(entry)
    }

    func getTimeline(
        in context: Context,
        completion: @escaping (Timeline<VPNStatusTimelineEntry>) -> Void
    ) {
        NETunnelProviderManager.loadAllFromPreferences { managers, error in
            let entry = Self.buildEntry(managers: managers, error: error)
            let timeline = Timeline(entries: [entry], policy: .atEnd)
            completion(timeline)
        }
    }

    private static func buildEntry(
        managers: [NETunnelProviderManager]?,
        error: Error?
    ) -> VPNStatusTimelineEntry {
        let defaults = groupDefaults
        let entryLoc = defaults?.string(forKey: entryKey) ?? ""
        let exitLoc = defaults?.string(forKey: exitKey) ?? ""
        let expiration = Date().addingTimeInterval(5 * 60)

        let status: VPNStatus
        if error != nil {
            status = .error
        } else if let manager = managers?.first {
            status = .status(manager.connection.status)
        } else {
            status = .notConfigured
        }

        return VPNStatusTimelineEntry(
            date: expiration,
            status: status,
            entryLocation: entryLoc,
            exitLocation: exitLoc
        )
    }
}
