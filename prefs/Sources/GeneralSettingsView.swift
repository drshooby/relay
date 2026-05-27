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
                VStack(alignment: .leading, spacing: 4) {
                    Text("Track change debounce: \(Int(config.debounceMs)) ms")
                    Slider(value: $config.debounceMs, in: 200...5000, step: 100)
                        .onChange(of: config.debounceMs) { _, _ in config.save() }
                }
            }
        }
        .formStyle(.grouped)
        .onAppear {
            launchAtLogin = loginItem.status == .enabled
        }
    }
}
