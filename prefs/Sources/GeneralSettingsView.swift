import SwiftUI
import ServiceManagement

struct GeneralSettingsView: View {
    @ObservedObject var config: ConfigStore
    @State private var launchAtLogin: Bool = false

    private var loginItem: SMAppService { .mainApp }

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Launch Relay at login", isOn: $launchAtLogin)
                    .onChange(of: launchAtLogin) { _, newValue in
                        if newValue {
                            try? loginItem.register()
                        } else {
                            try? loginItem.unregister()
                        }
                    }
            }

            Section("Playback") {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Track change debounce: \(Int(config.debounceMs)) ms")
                    Slider(value: $config.debounceMs, in: 500...3000, step: 100)
                        .onChange(of: config.debounceMs) { _, _ in config.save() }
                    Text(
                        "How long Relay waits before publishing a track change to Discord. "
                        + "Lower = faster on skips; higher = smoother during rapid skipping."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }

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
        }
        .formStyle(.grouped)
        .onAppear {
            launchAtLogin = loginItem.status == .enabled
        }
    }
}
