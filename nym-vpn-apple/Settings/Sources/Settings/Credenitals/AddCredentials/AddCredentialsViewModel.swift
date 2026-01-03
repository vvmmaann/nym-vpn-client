import SwiftUI
import AppSettings
import Constants
import CredentialsManager
import ConnectionManager
import ConfigurationManager
#if os(iOS)
import KeyboardManager
#endif
import Theme
import ImpactGenerator
import ExternalLinkManager

@MainActor final class AddCredentialsViewModel: ObservableObject {
    private let credentialsManager: CredentialsManager
    private let configurationManager: ConfigurationManager
#if os(iOS)
    private let keyboardManager: KeyboardManager
#endif
    private let createAccountTitle = "addCredentials.createAccount".localizedString

    var signUpLink: String {
        // TODO: readd once the link is updated in the api
//        if let link = configurationManager.accountLinks?.signUp, !link.isEmpty {
//            return link
//        } else {
            Constants.pricingURL.rawValue
//        }
    }

    let appSettings: AppSettings
    let impactGenerator: ImpactGenerator
    let scannerIconName = "qrcode.viewfinder"

    @Binding private var path: NavigationPath

    @MainActor @Published var credentialText = "" {
        willSet(newText) {
            guard newText != credentialText else { return }
            error = CredentialsManagerError.noError

            scannerDidScanQRCode()
        }
    }
    @Published var error: Error = CredentialsManagerError.noError {
        didSet {
            configureError()
        }
    }
    @Published var textFieldStrokeColor = NymColor.gray2
    @Published var credentialSubtitleColor = NymColor.primary
    @Published var bottomPadding: CGFloat = 12
    @Published var errorMessageTitle = ""
    @MainActor @Published var isScannerDisplayed = false
    @Published var isFocused = true

#if os(iOS)
    init(
        path: Binding<NavigationPath>,
        appSettings: AppSettings,
        impactGenerator: ImpactGenerator,
        credentialsManager: CredentialsManager,
        configurationManager: ConfigurationManager,
        keyboardManager: KeyboardManager
    ) {
        _path = path
        self.appSettings = appSettings
        self.impactGenerator = impactGenerator
        self.credentialsManager = credentialsManager
        self.configurationManager = configurationManager
        self.keyboardManager = keyboardManager
    }
#elseif os(macOS)
    init(
        path: Binding<NavigationPath>,
        appSettings: AppSettings,
        impactGenerator: ImpactGenerator,
        configurationManager: ConfigurationManager,
        credentialsManager: CredentialsManager
    ) {
        _path = path
        self.appSettings = appSettings
        self.impactGenerator = impactGenerator
        self.configurationManager = configurationManager
        self.credentialsManager = credentialsManager
    }
#endif

    @MainActor func importCredentials() {
        error = CredentialsManagerError.noError
        let trimmedCredential = credentialText.trimmingCharacters(in: .whitespacesAndNewlines)

        Task {
            do {
                try await credentialsManager.add(credential: trimmedCredential, type: .mnemonic)
                credentialsDidAdd()
            } catch let newError {
                Task { @MainActor in
                    credentialText = trimmedCredential
                    error = CredentialsManagerError.generalError(String(describing: newError.localizedDescription))
                }
            }
        }
    }
}

// MARK: - Navigation -
extension AddCredentialsViewModel {
    func navigateBack() {
        if !path.isEmpty { path.removeLast() }
    }

    func navigateHome() {
        path = .init()
    }

    func navigateToSocialLogin() {
        // todo
    }

    func navigateToCreateAccount() {
        #if os(iOS)
        impactGenerator.impact()
        path.append(SettingLink.generatePassphrase)
        #elseif os(macOS)
        try? ExternalLinkManager.shared.openExternalURL(urlString: signUpLink)
        #endif
    }
}

// MARK: - Private -
extension AddCredentialsViewModel {
    @MainActor func configureError() {
        let error = error as? CredentialsManagerError

        textFieldStrokeColor = error == .noError ? NymColor.gray2 : NymColor.error
        credentialSubtitleColor = error == .noError ? NymColor.primary : NymColor.error
        bottomPadding = error != .noError ? 4 : 12

        errorMessageTitle = (error == .noError ? "" : error?.localizedTitle)
        ?? (CredentialsManagerError.generalError("").localizedTitle ?? "Error".localizedString)
    }

    @MainActor func credentialsDidAdd() {
        credentialText = ""
        navigateHome()
    }

    @MainActor func scannerDidScanQRCode() {
#if os(iOS)
        if isScannerDisplayed {
            isFocused = false
            isScannerDisplayed = false
            keyboardManager.hideKeyboard()
            importCredentials()
        }
#endif
    }
}
