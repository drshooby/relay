import SwiftUI

@main
struct RelayPreferencesApp: App {
    @StateObject private var config = ConfigStore()

    var body: some Scene {
        Window("Relay Preferences", id: "prefs") {
            TabView {
                GeneralSettingsView(config: config)
                    .tabItem { Label("General", systemImage: "gear") }
                DisplaySettingsView(config: config)
                    .tabItem { Label("Display", systemImage: "eye") }
                NowPlayingView()
                    .tabItem { Label("Now Playing", systemImage: "music.note") }
                StorageView()
                    .tabItem { Label("Storage", systemImage: "internaldrive") }
            }
            .padding()
            .frame(minWidth: 480, minHeight: 360)
            .onAppear { config.load() }
        }
        .windowResizability(.contentSize)
        .commands {
            // Hide File > New — this app has no documents.
            CommandGroup(replacing: .newItem) {}
        }
    }
}
