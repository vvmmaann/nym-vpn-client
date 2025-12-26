import SwiftUI
import ImpactGenerator
#if os(iOS)
import PurchasesManager
#endif
import UIComponents
import Theme
import Constants
import ExternalLinkManager

public struct CreateAccountWelcomeView: View {
    #if os(iOS)
    @EnvironmentObject private var purchasesManager: PurchasesManager
    #endif
    @Binding private var path: NavigationPath

    public var body: some View {
        VStack(spacing: 0) {
            navbar
            ScrollView {
                VStack(spacing: 0) {
                    Spacer()
                        .frame(height: 48)
                    logoView
                    Spacer()
                        .frame(height: 48)
                    welcomeTitle
                    Spacer()
                        .frame(height: 24)
                    maximumPrivacySection
                    Divider()
                        .frame(height: 1)
                        .overlay(NymColor.gray2)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 24)
                    quickSetupSection
                    Divider()
                        .frame(height: 1)
                        .overlay(NymColor.gray2)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 24)
                    alreadyHaveAnAccountSection
                }
                .frame(maxWidth: MagicNumbers.moreMaxWidth)
                .padding(.horizontal, 32)
                Spacer()
            }
        }
        .navigationBarBackButtonHidden(true)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            NymColor.background
                .ignoresSafeArea()
        }
    }

    public init(path: Binding<NavigationPath>) {
        _path = path
    }
}

private extension CreateAccountWelcomeView {
    var navbar: some View {
        CustomNavBar(
            useElevationBackground: true,
            isLogoImageHidden: true,
            leftButton: CustomNavBarButton(type: .back, action: { navigateHome() })
        )
    }

    var logoView: some View {
        GenericImage(imageName: "logoText")
            .frame(width: 110, height: 30)
            .accessibilityLabel("NymVPN".localizedString)
    }

    var welcomeTitle: some View {
        Text("\("addCredentials.welcome.Title".localizedString) \("NymVPN".localizedString)")
            .textStyle(.Headline.Large.regular)
            .foregroundStyle(NymColor.primary)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 16)
    }

    var maximumPrivacySection: some View {
        VStack(alignment: .leading) {
            HStack {
                Text("🔒 \("createAccount.maximumPrivacy".localizedString)")
                    .textStyle(.Body.Large.regular)
                    .foregroundStyle(NymColor.primary)
                Spacer()
            }
            .padding(.bottom, 8)

            HStack {
                Text("createAccount.maximumPrivacyDetails".localizedString)
                    .textStyle(.Body.Medium.regular)
                    .foregroundStyle(NymColor.gray1)
                    .multilineTextAlignment(.leading)
                Spacer()
            }
            .padding(.bottom, 16)

            GenericButton(title: "createAccount.createAnonymousAccountButton".localizedString)
                .onTapGesture {
                    navigateToCreateAccount()
                }
                .accessibilityAction {
                    navigateToCreateAccount()
                }
        }
    }

    var quickSetupSection: some View {
        VStack(alignment: .leading) {
            HStack {
                Text("⚡️ \("createAccount.quickSetup".localizedString)")
                    .textStyle(.Body.Large.regular)
                    .foregroundStyle(NymColor.primary)
                Spacer()
            }
            .padding(.bottom, 8)

            HStack {
                Text("createAccount.quickSetupDetails".localizedString)
                    .textStyle(.Body.Medium.regular)
                    .foregroundStyle(NymColor.gray1)
                    .multilineTextAlignment(.leading)
                Spacer()
            }
            .padding(.bottom, 16)

            GenericButton(
                title: "createAccount.continueWithSocialAccountButton".localizedString,
                style: .primaryBorderOnly
            )
            .onTapGesture {
                navigateToSocialLogin()
            }
            .accessibilityAction {
                navigateToSocialLogin()
            }
        }
    }

    var alreadyHaveAnAccountSection: some View {
        VStack(alignment: .leading) {
            HStack {
                Text("createAccount.alreadyHaveAccount".localizedString)
                    .textStyle(.Body.Large.regular)
                    .foregroundStyle(NymColor.primary)
                Spacer()
            }
            .padding(.bottom, 8)

            GenericButton(
                title: "createAccount.loginWithPassphrase".localizedString,
                style: .primaryBorderOnly
            )
            .padding(.bottom, 24)
            .onTapGesture {
                navigateToLogin()
            }
            .accessibilityAction {
                navigateToLogin()
            }
        }
        .environment(\.openURL, OpenURLAction { url in
            guard url.absoluteString == "login" else { return .discarded }
            navigateToLogin()
            return .handled
        })
    }
}

// MARK: - Actions -
private extension CreateAccountWelcomeView {
    func navigateHome() {
        path = .init()
    }

    func navigateToCreateAccount() {
        #if os(iOS)
        ImpactGenerator.shared.impact()
        path.append(SettingLink.generatePassphrase)
        #elseif os(macOS)
        try? ExternalLinkManager.shared.openExternalURL(urlString: signUpLink)
        #endif
    }

    func navigateToLogin() {
        ImpactGenerator.shared.impact()
        path.append(SettingLink.addCredentials)
    }
    
    func navigateToSocialLogin() {
        // TODO
    }

    var signUpLink: String {
        // TODO: read once the link is updated in the api
//        if let link = configurationManager.accountLinks?.signUp, !link.isEmpty {
//            return link
//        } else {
            Constants.pricingURL.rawValue
//        }
    }
}
