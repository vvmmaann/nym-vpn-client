import SwiftUI
import WidgetKit

struct NymVPNStatusWidget: Widget {
    let kind = "NymVPNStatusWidget"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: VPNStatusTimelineProvider()) { entry in
            VPNStatusView(entry: entry)
                .widgetURL(URL(string: "nymvpn://home"))
        }
        .configurationDisplayName("NymVPN")
        .description("View and control your VPN connection.")
        .supportedFamilies([.systemSmall])
        .contentMarginsDisabled()
    }
}
