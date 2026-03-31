import NetworkExtension

enum VPNStatus {
    case status(NEVPNStatus)
    case error
    case notConfigured

    var isConnecting: Bool {
        switch self {
        case .status(let status):
            return status == .connecting
        default:
            return false
        }
    }

    var isDisconnecting: Bool {
        switch self {
        case .status(let status):
            return status == .disconnecting
        default:
            return false
        }
    }

    var isConnected: Bool {
        switch self {
        case .status(let status):
            return status == .connected || status == .reasserting
        default:
            return false
        }
    }
}
