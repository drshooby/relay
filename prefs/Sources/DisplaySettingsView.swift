import SwiftUI

struct DisplaySettingsView: View {
    @ObservedObject var config: ConfigStore

    var body: some View {
        Form {
            Section("Discord Card Fields") {
                Toggle("Show track title", isOn: $config.showTitle)
                    .onChange(of: config.showTitle) { _, _ in config.save() }
                Toggle("Show artist", isOn: $config.showArtist)
                    .onChange(of: config.showArtist) { _, _ in config.save() }
                Toggle("Show album", isOn: $config.showAlbum)
                    .onChange(of: config.showAlbum) { _, _ in config.save() }
                Toggle("Show artwork", isOn: $config.showArtwork)
                    .onChange(of: config.showArtwork) { _, _ in config.save() }
            }
            Section("Help") {
                // swiftlint:disable:next force_unwrapping
                Link(
                    "About Discord Activity Sharing",
                    destination: URL(
                        string: "https://support.discord.com/hc/en-us/articles/115000076487"
                    )!
                )
            }
        }
        .formStyle(.grouped)
    }
}
