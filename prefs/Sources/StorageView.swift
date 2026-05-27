import SwiftUI
import AppKit

private struct StorageRow: Identifiable {
    let id = UUID()
    let label: String
    let path: URL
}

struct StorageView: View {
    @State private var rows: [StorageRow] = []
    @State private var clearMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Relay reads and writes only these locations on your disk.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .padding(.horizontal)

            List(rows) { row in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(row.label).font(.headline)
                        Text(row.path.path)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Text(sizeDescription(for: row.path))
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                    Spacer()
                    Button("Reveal") {
                        NSWorkspace.shared.selectFile(
                            row.path.path,
                            inFileViewerRootedAtPath: row.path.deletingLastPathComponent().path
                        )
                    }
                    .buttonStyle(.borderless)
                }
                .padding(.vertical, 4)
            }

            HStack {
                Button("Clear Artwork Cache") {
                    clearArtworkCache()
                }
                .buttonStyle(.bordered)

                if let msg = clearMessage {
                    Text(msg)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal)
            .padding(.bottom, 8)
        }
        .onAppear { buildRows() }
    }

    private func buildRows() {
        let fm = FileManager.default
        guard let support = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
        else { return }

        let relaySupport = support.appendingPathComponent("relay")

        var newRows: [StorageRow] = [
            StorageRow(
                label: "Config",
                path: relaySupport.appendingPathComponent("config.toml")
            ),
            StorageRow(
                label: "Now-playing snapshot",
                path: relaySupport.appendingPathComponent("nowplaying.json")
            ),
            StorageRow(
                label: "First-run marker",
                path: relaySupport.appendingPathComponent(".permission-prompt-shown")
            ),
            StorageRow(
                label: "Artwork cache",
                path: relaySupport.appendingPathComponent("artwork_cache.json")
            ),
        ]

        // Helper binary and prefs app: resolve relative to Relay.app bundle Resources.
        // RelayPreferences.app is nested inside Relay.app/Contents/Resources/.
        // Its own bundle is at Contents/Resources/RelayPreferences.app/, so the
        // parent Resources folder is three levels up from our MacOS executable.
        if let exe = Bundle.main.executableURL {
            let resources = exe
                .deletingLastPathComponent()  // .../MacOS
                .deletingLastPathComponent()  // .../Contents
                .appendingPathComponent("Resources")
            newRows.append(StorageRow(
                label: "Helper binary",
                path: resources.appendingPathComponent("relay-helper")
            ))
            newRows.append(StorageRow(
                label: "Prefs app",
                path: resources.appendingPathComponent("RelayPreferences.app")
            ))
        }

        // Logs directory (optional).
        if let library = fm.urls(for: .libraryDirectory, in: .userDomainMask).first {
            let logsDir = library.appendingPathComponent("Logs/relay")
            newRows.append(StorageRow(label: "Logs", path: logsDir))
        }

        rows = newRows
    }

    private func sizeDescription(for url: URL) -> String {
        let fm = FileManager.default
        guard fm.fileExists(atPath: url.path) else { return "Not present" }
        if let attrs = try? fm.attributesOfItem(atPath: url.path),
           let size = attrs[.size] as? Int64 {
            return ByteCountFormatter.string(fromByteCount: size, countStyle: .file)
        }
        return "—"
    }

    private func clearArtworkCache() {
        guard let support = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        ).first else { return }
        let cacheFile = support
            .appendingPathComponent("relay")
            .appendingPathComponent("artwork_cache.json")
        do {
            try FileManager.default.removeItem(at: cacheFile)
            clearMessage = "Artwork cache cleared."
        } catch {
            clearMessage = "Failed to clear cache: \(error.localizedDescription)"
        }
    }
}
