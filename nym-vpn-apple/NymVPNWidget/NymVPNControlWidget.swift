import SwiftUI
import WidgetKit

@available(iOSApplicationExtension 18.0, *)
struct NymVPNControlWidget: ControlWidget {
    static let displayName = LocalizedStringResource(stringLiteral: "NymVPN")
    static let description = LocalizedStringResource(stringLiteral: "View and manage your VPN connection.")

    var body: some ControlWidgetConfiguration {
        StaticControlConfiguration(
            kind: "NymVPNControlWidget",
            provider: VPNControlStatusValueProvider()
        ) { status in
            ControlWidgetToggle(
                status.isConnected ? "Connected" : "Disconnected",
                isOn: status.isConnected,
                action: ToggleVPNSetValueIntent()
            ) { isOn in
                if isOn {
                    Label("Connected", image: "nymConnected")
                } else {
                    Label("Disconnected", image: "nymDisconnected")
                }
            }
            .tint(.green)
        }
        .displayName(Self.displayName)
        .description(Self.description)
    }
}
