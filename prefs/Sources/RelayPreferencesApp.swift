import SwiftUI

@main
struct RelayPreferencesApp: App {
    @StateObject private var config = ConfigStore()

    var body: some Scene {
        Window("Relay Preferences", id: "prefs") {
            TabView {
                GeneralSettingsView(config: config)
                    .tabItem { Label("General", systemImage: "gear") }
                NowPlayingView()
                    .tabItem { Label("Now Playing", systemImage: "music.note") }
                StorageView()
                    .tabItem { Label("Storage", systemImage: "internaldrive") }
                ErrorsView()
                    .tabItem { Label("Errors", systemImage: "exclamationmark.triangle") }
                MiscView()
                    .tabItem { Label("Misc", systemImage: "ellipsis.circle") }
            }
            .padding()
            .frame(minWidth: 480, minHeight: 420)
            .onAppear { config.load() }
        }
        .windowResizability(.contentSize)
        .commands {
            // Hide File > New — this app has no documents.
            CommandGroup(replacing: .newItem) {}
        }
    }
}
