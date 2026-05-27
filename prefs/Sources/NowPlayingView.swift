import SwiftUI

private struct NowPlayingSnapshot: Codable {
    var title: String?
    var artist: String?
    var album: String?
    var artworkUrl: String?

    enum CodingKeys: String, CodingKey {
        case title, artist, album
        case artworkUrl = "artwork_url"
    }
}

struct NowPlayingView: View {
    @State private var snapshot: NowPlayingSnapshot?
    @State private var pollTimer: Timer?

    private var snapshotURL: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("relay/nowplaying.json")
    }

    var body: some View {
        VStack(spacing: 16) {
            if let snap = snapshot {
                if let urlStr = snap.artworkUrl, let url = URL(string: urlStr) {
                    AsyncImage(url: url) { image in
                        image.resizable().scaledToFit()
                    } placeholder: {
                        RoundedRectangle(cornerRadius: 8)
                            .fill(Color.secondary.opacity(0.2))
                    }
                    .frame(width: 120, height: 120)
                    .cornerRadius(8)
                }
                VStack(spacing: 4) {
                    Text(snap.title ?? "Unknown Title")
                        .font(.headline)
                    Text(snap.artist ?? "Unknown Artist")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    if let album = snap.album {
                        Text(album)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                }
            } else {
                Image(systemName: "music.note")
                    .font(.system(size: 48))
                    .foregroundStyle(.secondary)
                Text("Not playing")
                    .foregroundStyle(.secondary)
            }
        }
        .padding()
        .onAppear {
            loadSnapshot()
            pollTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
                loadSnapshot()
            }
        }
        .onDisappear {
            pollTimer?.invalidate()
            pollTimer = nil
        }
    }

    private func loadSnapshot() {
        guard let url = snapshotURL,
              let data = try? Data(contentsOf: url),
              let snap = try? JSONDecoder().decode(NowPlayingSnapshot.self, from: data)
        else {
            snapshot = nil
            return
        }
        snapshot = snap
    }
}
