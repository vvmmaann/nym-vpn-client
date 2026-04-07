import NetworkExtension
import Logging
import ConfigurationManager
import Constants
import NymLogger
import ErrorHandler
import NymVPNLib
import TunnelMixnet
import Tunnels
import AppVersionProvider
import Vapor
import NIOSSL

class PacketTunnelProvider: NEPacketTunnelProvider {
    let tunnelActor: TunnelActor

    lazy var logger = Logger(label: "MixnetTunnel")
    var logInitFailure: String?
    var vpnService: NymVpnService?
    var commandSender: NymVpnServiceCommandSender?
    private var doHApp: Application?

    override init() {
        tunnelActor = TunnelActor()
        super.init()

        self.configureLogger()
        LoggingSystem.bootstrap { label in
            let fileLogHandler = FileLogHandler(label: label, logFileManager: LogFileManager(logFileType: .tunnel))
#if DEBUG
            let osLogHandler = OSLogHandler(
                subsystem: Bundle.main.bundleIdentifier ?? "NymMixnetTunnel",
                category: label
            )
            return MultiplexLogHandler([osLogHandler, fileLogHandler])
#else
            return fileLogHandler
#endif
        }
    }

    override func startTunnel(options: [String: NSObject]? = nil) async throws {
        await tunnelActor.setTunnelProvider(self)

        do {
            let app = try await makeDoHApp(upstreamURL: URL(string: "https://1.1.1.1/dns-query")!)
            self.doHApp = app
            try await app.startup()
        } catch {
            logger.error("Failed to start local dns proxy: \(error.localizedDescription)")
        }
        
        guard let tunnelProviderProtocol = protocolConfiguration as? NETunnelProviderProtocol,
              let mixnetConfig = await tunnelProviderProtocol.asMixnetConfig()
        else {
            logger.error("Failed to obtain tunnel configuration")
            throw PacketTunnelProviderError.invalidSavedConfiguration
        }
        let vpnConfig = try mixnetConfig.asVpnConfig(tunProvider: self)
        try await setup(vpnConfig: vpnConfig)

        _ = try await commandSender?.connectTunnel()
        try await tunnelActor.waitUntilStarted()
    }

    override func stopTunnel(with reason: NEProviderStopReason) async {
        logger.info("Stop tunnel... \(reason.rawValue)")
        
        try? await doHApp?.asyncShutdown()
        doHApp = nil

        await vpnService?.shutdownAndWait()
        await tunnelActor.setTunnelProvider(nil)
        vpnService = nil
        commandSender = nil
    }
}

func makeDoHApp(upstreamURL: URL) async throws -> Application {
    let app = try await Application.make(.production)

    // Load your p12 identity — extract cert + key for Vapor/NIOSSL
    let p12URL = Bundle.main.url(forResource: "server", withExtension: "p12")!
    let p12Data = try Data(contentsOf: p12URL)

    // NIOSSL expects PEM — convert your p12 at build time or use NIOSSLCertificate directly
    // If you have the cert/key as PEM files in the bundle instead, use those:
    let certURL = Bundle.main.url(forResource: "server", withExtension: "crt")!
    let keyURL  = Bundle.main.url(forResource: "server", withExtension: "key")!

    let certs = try NIOSSLCertificate.fromPEMFile(certURL.path).map { NIOSSLCertificateSource.certificate($0) }
    let key   = try NIOSSLPrivateKey(file: keyURL.path, format: .pem)

    var tlsConfig = TLSConfiguration.makeServerConfiguration(
        certificateChain: certs,
        privateKey: .privateKey(key)
    )
    // Explicitly advertise h2 + http/1.1 — Vapor does this automatically when
    // tlsConfiguration is set, but being explicit makes the intent clear.
    tlsConfig.applicationProtocols = ["h2", "http/1.1"]

    app.http.server.configuration.hostname = "127.0.0.1"
    app.http.server.configuration.port = 9000
    app.http.server.configuration.tlsConfiguration = tlsConfig
    // HTTP/2 is enabled automatically by Vapor when TLS is configured.
    // Explicitly set it to be safe:
    app.http.server.configuration.supportVersions = [.two, .one]

    // Register DoH routes
    let handler = DoHHandler(upstreamURL: upstreamURL)
    app.get("dns-query", use: handler.handle)
    app.post("dns-query", use: handler.handle)

    return app
}

// MARK: - DoH handler

struct DoHHandler {
    let upstreamURL: URL

    func handle(req: Request) async throws -> Response {
        // Build upstream URLRequest
        var upstreamRequest = URLRequest(url: buildUpstreamURL(from: req))
        upstreamRequest.httpMethod = req.method == .POST ? "POST" : "GET"

        if req.method == .POST {
            upstreamRequest.httpBody = req.body.data.flatMap { Data(buffer: $0) }
            upstreamRequest.setValue("application/dns-message", forHTTPHeaderField: "Content-Type")
        }

        upstreamRequest.setValue("application/dns-message", forHTTPHeaderField: "Accept")

        // Perform upstream request — use async/await instead of semaphore
        let (responseData, urlResponse, error) = await URLSession.shared.data(for: upstreamRequest)

        if let error = error {
            throw Abort(.badGateway, reason: "Upstream error: \(error.localizedDescription)")
        }

        guard let httpResponse = urlResponse as? HTTPURLResponse else {
            throw Abort(.badGateway, reason: "Invalid upstream response")
        }

        var headers = HTTPHeaders()
        headers.add(name: .contentType, value: "application/dns-message")

        if let cacheControl = httpResponse.value(forHTTPHeaderField: "Cache-Control") {
            headers.add(name: .cacheControl, value: cacheControl)
        }

        return Response(
            status: .init(statusCode: httpResponse.statusCode),
            headers: headers,
            body: .init(data: responseData)
        )
    }

    private func buildUpstreamURL(from req: Request) -> URL {
        guard req.method == .GET,
              let query = req.url.query else {
            return upstreamURL
        }
        return URL(string: upstreamURL.absoluteString + "?" + query) ?? upstreamURL
    }
}

// MARK: - URLSession async helper

extension URLSession {
    func data(for request: URLRequest) async -> (Data, URLResponse?, Error?) {
        await withCheckedContinuation { continuation in
            dataTask(with: request) { data, response, error in
                continuation.resume(returning: (data ?? Data(), response, error))
            }.resume()
        }
    }
}

extension PacketTunnelProvider {
    func setup(vpnConfig: VpnConfig) async throws {
        try await ConfigurationManager.shared.setup()

        vpnService = try await NymVpnService.newService(
            config: vpnConfig,
            environment: ConfigurationManager.shared.networkEnv ?? .newWithMainnetFallback(),
            eventListener: self
        )
        commandSender = vpnService?.getCommandSender()
    }

    func configureLogger() {
        let logDir = LogFileManager.logsDirectory()?.path()
        // Extracted from ConfigurationManager.shared.debugLevel
        let isTestFlight = Bundle.main.appStoreReceiptURL?.lastPathComponent == "sandboxReceipt"
        let isDebugLogsOn = UserDefaults(
            suiteName: Constants.groupID.rawValue
        )?.bool(forKey: "isDebugLogsOn") ?? false

        let logLevel: LogLevel = (isTestFlight || isDebugLogsOn) ? .debug : .info
        initLogger(logDir: logDir, logLevel: logLevel, sentryMonitoring: true)
    }
}

extension PacketTunnelProvider: OsTunProvider {
    func setTunnelNetworkSettings(tunnelSettings: TunnelNetworkSettings) async throws {
        do {
            let networkSettings = tunnelSettings.asPacketTunnelNetworkSettings()
            
            let dohSettings = NEDNSOverHTTPSSettings(servers: ["127.0.0.1"])
            dohSettings.serverURL = URL(string: "https://localhost:9000/dns-query")!
            dohSettings.matchDomains = [""]
            networkSettings.dnsSettings = dohSettings
            
            logger.debug("Set network settings: \(networkSettings)")
            try await setTunnelNetworkSettings(networkSettings)
        } catch {
            logger.error("Failed to set tunnel network settings: \(error)")
            throw error
        }
    }
}
