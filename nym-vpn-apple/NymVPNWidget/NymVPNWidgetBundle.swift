import WidgetKit
import SwiftUI

@main
struct NymVPNWidgetBundle: WidgetBundle {
    var body: some Widget {
        NymVPNStatusWidget()
        if #available(iOSApplicationExtension 18.0, macOS 15.0, *) {
            NymVPNControlWidget()
        }
    }
}
