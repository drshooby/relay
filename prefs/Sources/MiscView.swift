import SwiftUI

struct MiscView: View {
    private var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "—"
    }

    var body: some View {
        Form {
            Section("Help") {
                // swiftlint:disable:next force_unwrapping
                Link(
                    "Discord Activity Sharing Settings",
                    destination: URL(
                        string: "https://support.discord.com/hc/en-us/articles/115000076487"
                    )!
                )
                // swiftlint:disable:next force_unwrapping
                Link(
                    "Relay on GitHub",
                    destination: URL(string: "https://github.com/drshooby/relay")!
                )
            }

            Section("About") {
                HStack {
                    Text("Version")
                    Spacer()
                    Text(appVersion)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }
}
