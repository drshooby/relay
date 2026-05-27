import Foundation

/// Observed config state for the prefs window.
/// Reads/writes ~/Library/Application Support/relay/config.toml directly.
@MainActor
final class ConfigStore: ObservableObject {
    // Display section
    @Published var showTitle: Bool = true
    @Published var showArtist: Bool = true
    @Published var showAlbum: Bool = true
    @Published var showArtwork: Bool = true
    // Playback section
    @Published var debounceMs: Double = 1500

    private var configURL: URL? {
        guard let support = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        ).first else { return nil }
        return support.appendingPathComponent("relay/config.toml")
    }

    func load() {
        guard let url = configURL,
              let content = try? String(contentsOf: url, encoding: .utf8) else {
            return  // missing file — use defaults
        }
        parseTOML(content)
    }

    func save() {
        guard let url = configURL else { return }
        let dir = url.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let content = buildTOML()
        try? content.write(to: url, atomically: true, encoding: .utf8)
    }

    // MARK: - Hand-rolled TOML parser (display + playback sections only)

    private func parseTOML(_ content: String) {
        var inDisplay = false
        var inPlayback = false

        for rawLine in content.components(separatedBy: "\n") {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("#") || line.isEmpty { continue }

            if line == "[display]" { inDisplay = true; inPlayback = false; continue }
            if line == "[playback]" { inPlayback = true; inDisplay = false; continue }
            if line.hasPrefix("[") { inDisplay = false; inPlayback = false; continue }

            let parts = line.components(separatedBy: "=").map {
                $0.trimmingCharacters(in: .whitespaces)
            }
            guard parts.count >= 2 else { continue }
            let key = parts[0]
            let valueStr = parts[1...].joined(separator: "=")
                .trimmingCharacters(in: .whitespaces)

            if inDisplay {
                switch key {
                case "show_title":   showTitle   = valueStr == "true"
                case "show_artist":  showArtist  = valueStr == "true"
                case "show_album":   showAlbum   = valueStr == "true"
                case "show_artwork": showArtwork = valueStr == "true"
                default: break
                }
            } else if inPlayback {
                if key == "debounce_ms", let v = Double(valueStr) {
                    debounceMs = v
                }
            }
        }
    }

    private func buildTOML() -> String {
        func boolStr(_ v: Bool) -> String { v ? "true" : "false" }
        return """
        [display]
        show_title = \(boolStr(showTitle))
        show_artist = \(boolStr(showArtist))
        show_album = \(boolStr(showAlbum))
        show_artwork = \(boolStr(showArtwork))

        [playback]
        debounce_ms = \(Int(debounceMs))
        """
    }
}
