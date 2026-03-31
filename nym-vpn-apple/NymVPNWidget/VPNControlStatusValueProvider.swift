import NetworkExtension
import WidgetKit

@available(iOS 18.0, *)
struct VPNControlStatusValueProvider: ControlValueProvider {
    typealias Value = VPNStatus

    var previewValue: VPNStatus {
        .status(.disconnected)
    }

    func currentValue() async throws -> VPNStatus {
        do {
            let managers = try await NETunnelProviderManager.loadAllFromPreferences()
            guard let manager = managers.first else {
                return .notConfigured
            }
            return .status(manager.connection.status)
        } catch {
            return .error
        }
    }
}
