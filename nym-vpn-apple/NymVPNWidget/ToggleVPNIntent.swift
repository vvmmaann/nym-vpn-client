import AppIntents
import NetworkExtension
import WidgetKit

struct ToggleVPNIntent: AppIntent {
    static var title: LocalizedStringResource = "Toggle NymVPN"

    func perform() async throws -> some IntentResult {
        let managers = try await NETunnelProviderManager.loadAllFromPreferences()
        guard let manager = managers.first else { return .result() }

        switch manager.connection.status {
        case .connected, .connecting, .reasserting:
            manager.connection.stopVPNTunnel()
        default:
            try manager.connection.startVPNTunnel()
        }

        WidgetCenter.shared.reloadAllTimelines()
        return .result()
    }
}

@available(iOS 18.0, *)
struct ToggleVPNSetValueIntent: SetValueIntent {
    static var title: LocalizedStringResource = "Toggle NymVPN"

    @Parameter(title: "Enabled")
    var value: Bool

    func perform() async throws -> some IntentResult {
        let managers = try await NETunnelProviderManager.loadAllFromPreferences()
        guard let manager = managers.first else { return .result() }

        if value {
            try manager.connection.startVPNTunnel()
        } else {
            manager.connection.stopVPNTunnel()
        }

        WidgetCenter.shared.reloadAllTimelines()
        return .result()
    }
}
