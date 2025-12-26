import SwiftUI
import AppSettings
import CredentialsManager
import Device
#if os(iOS)
import ExternalLinkManager
import KeyboardManager
#endif
import Theme
import UIComponents

struct AddCredentialsView: View {
#if os(iOS)
    @EnvironmentObject private var keyboardManager: KeyboardManager
#endif
    @StateObject private var viewModel: AddCredentialsViewModel
    @FocusState private var isFocused: Bool

    init(viewModel: AddCredentialsViewModel) {
        _viewModel = StateObject(wrappedValue: viewModel)
    }

    var body: some View {
        VStack {
            navbar()
            GeometryReader { geometry in
#if os(iOS)
                KeyboardHostView {
                    scrollViewContent(geometry: geometry)
                }
#elseif os(macOS)
                scrollViewContent(geometry: geometry)
#endif
            }
            .frame(maxWidth: MagicNumbers.maxWidth)
        }
        .navigationBarBackButtonHidden(true)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .ignoresSafeArea(edges: [.bottom])
        .background {
            NymColor.background
                .ignoresSafeArea()
        }
#if os(iOS)
        .fullScreenCover(isPresented: $viewModel.isScannerDisplayed) {
            QRScannerView(
                viewModel: QRScannerViewModel(
                    isDisplayed: $viewModel.isScannerDisplayed,
                    scannedText: $viewModel.credentialText,
                    externalLinkManager: ExternalLinkManager.shared,
                    keyboardManager: KeyboardManager.shared
                )
            )
        }
#endif
//        .onAppear {
//            isFocused = viewModel.isFocused
//        }
        .onChange(of: isFocused) {
            viewModel.isFocused = $0
        }
    }
}

private extension AddCredentialsView {
    @ViewBuilder
    func navbar() -> some View {
        CustomNavBar(
            leftButton: CustomNavBarButton(type: .back, action: { viewModel.navigateBack() })
        )
    }

    @ViewBuilder
    func scrollViewContent(geometry: GeometryProxy) -> some View {
        ScrollView {
            VStack {
                content(safeAreaInsets: geometry.safeAreaInsets)
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
        }
        .safeAreaInset(edge: .top) {
            Color.clear.frame(height: Device.isMacOS ? 120 : 80)
        }
        .safeAreaInset(edge: .bottom) {
            Color.clear.frame(height: Device.isMacOS ? 120 : 80)
        }
        .scrollIndicators(.hidden)
        .onTapGesture {
            isFocused = false
        }
    }

    @ViewBuilder
    func content(safeAreaInsets: EdgeInsets) -> some View {
        titleSection()
            .onTapGesture {
                isFocused = false
            }

        inputView()
            .onTapGesture {
                guard !isFocused else { return }
                isFocused = true
            }
        if !viewModel.errorMessageTitle.isEmpty {
            errorMessageView(title: viewModel.errorMessageTitle)
        }
        Spacer()
            .frame(height: 8)

        HStack {
            loginButton()
// #if os(iOS)
//            qrScannerButton()
//                .padding(.trailing, 16)
// #endif
        }
        .padding(.vertical, 16)

        createAccount()
    }

    @ViewBuilder
    func titleSection() -> some View {
        titleText()
        Spacer()
            .frame(height: 16)

        subtitleText()
        Spacer()
            .frame(height: 16)
    }

    @ViewBuilder
    func titleText() -> some View {
        Text("\("addCredentials.logInto".localizedString) \("NymVPN".localizedString)")
            .textStyle(.Headline.ExtraLarge.bold)
            .foregroundStyle(NymColor.primary)
    }

    @ViewBuilder
    func subtitleText() -> some View {
        VStack {
            Text("addCredentials.enterOrPaste".localizedString)
                .textStyle(.Body.Large.regular)
                .foregroundStyle(NymColor.gray1)
                .multilineTextAlignment(.center)
            Text("addCredentials.twentyFourWords".localizedString)
                .textStyle(.Body.Large.regular)
                .foregroundStyle(NymColor.gray1)
                .multilineTextAlignment(.center)
        }
    }

    @ViewBuilder
    func inputView() -> some View {
        LazyVStack(alignment: .leading) {
            TextField("addCredentials.placeholder".localizedString, text: $viewModel.credentialText, axis: .vertical)
// https://stackoverflow.com/questions/74989806/how-to-dismiss-keyboard-in-swiftui-keyboard-when-pressing-done
//                .onSubmit {
//                    viewModel.importCredentials()
//                    isFocused = false
//                }
                .onChange(of: viewModel.credentialText) { [weak viewModel] _ in
                    if viewModel?.credentialText.last?.isNewline == .some(true) {
                        login()
                    }
                }
                .redacted(reason: .privacy)
                .submitLabel(.done)
                .textStyle(NymTextStyle.Body.Large.regular)
                .padding(16)
                .lineLimit(8, reservesSpace: true)
                .focused($isFocused)
                .textFieldStyle(PlainTextFieldStyle())
                .autocorrectionDisabled()
            Spacer()
        }
        .contentShape(
            RoundedRectangle(cornerRadius: 8)
                .inset(by: 0.5)
        )
        .frame(height: 212)
        .cornerRadius(8)
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .inset(by: 0.5)
                .stroke(viewModel.textFieldStrokeColor, lineWidth: 1)
        }
        .overlay(alignment: .topLeading) {
            Text("addCredentials.passphrase".localizedString)
                .foregroundStyle(viewModel.credentialSubtitleColor)
                .textStyle(.Body.Small.regular)
                .padding(4)
                .background(NymColor.background)
                .position(x: 70, y: 0)
        }
        .padding(EdgeInsets(top: 12, leading: 16, bottom: viewModel.bottomPadding, trailing: 16))
    }

    @ViewBuilder
    func errorMessageView(title: String) -> some View {
        HStack {
            Text(title)
                .foregroundStyle(NymColor.error)
                .lineLimit(nil)
                .textStyle(.Body.Small.regular)
            Spacer()
        }
        .padding(EdgeInsets(top: 0, leading: 28, bottom: 16, trailing: 28))
    }

    @ViewBuilder
    func loginButton() -> some View {
        GenericButton(title: "addCredentials.Login.Title".localizedString)
            .padding(.horizontal, 16)
            .onTapGesture {
                login()
            }
    }

//    @ViewBuilder
//    func qrScannerButton() -> some View {
//        GenericImage(systemImageName: viewModel.scannerIconName)
//            .frame(width: 56, height: 56)
//            .foregroundStyle(NymColor.connectTitle)
//            .background(NymColor.primaryOrange)
//            .cornerRadius(8)
//            .onTapGesture {
//                Task { @MainActor in
//                    viewModel.isScannerDisplayed.toggle()
//                }
//            }
//    }

    @ViewBuilder
    func createAccount() -> some View {
        VStack(spacing: 32) {
            GenericButton(
                title: "addCredentials.continueWithSocialAccountButton".localizedString,
                style: .primaryBorderOnly
            )
            .onTapGesture {
                viewModel.navigateToSocialLogin()
            }
            .accessibilityAction {
                viewModel.navigateToSocialLogin()
            }

            Text("addCredentials.newToNymVPN".localizedString)
                .textStyle(.Body.Medium.regular)
                .foregroundStyle(NymColor.gray1)

            GenericButton(
                title: "addCredentials.createAnonymousAccountButton".localizedString,
                style: .primaryBorderOnly
            )
            .onTapGesture {
                viewModel.navigateToCreateAccount()
            }
            .accessibilityAction {
                viewModel.navigateToCreateAccount()
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 16)
    }
}

private extension AddCredentialsView {
    func login() {
        viewModel.importCredentials()
        isFocused = false
    }
}
