import WidgetKit

struct VPNStatusTimelineEntry: TimelineEntry {
    let date: Date
    let status: VPNStatus
    let entryLocation: String
    let exitLocation: String

    init(date: Date, status: VPNStatus = .notConfigured, entryLocation: String = "", exitLocation: String = "") {
        self.date = date
        self.status = status
        self.entryLocation = entryLocation
        self.exitLocation = exitLocation
    }
}
