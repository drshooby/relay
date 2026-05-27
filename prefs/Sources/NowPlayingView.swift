import SwiftUI

private struct NowPlayingSnapshot: Codable {
    var title: String?
    var artist: String?
    var album: String?
    var artworkUrl: String?
    var playing: Bool?

    enum CodingKeys: String, CodingKey {
        case title, artist, album
        case artworkUrl = "artwork_url"
        case playing
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
        VStack {
            Spacer()

            if let snap = snapshot {
                VStack(spacing: 16) {
                    if let urlStr = snap.artworkUrl, let url = URL(string: urlStr) {
                        AsyncImage(url: url) { phase in
                            switch phase {
                            case .success(let image):
                                image
                                    .resizable()
                                    .scaledToFill()
                            case .failure:
                                artworkPlaceholder
                            case .empty:
                                artworkPlaceholder
                            @unknown default:
                                artworkPlaceholder
                            }
                        }
                        .frame(width: 180, height: 180)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .shadow(color: .black.opacity(0.25), radius: 12, x: 0, y: 4)
                    } else {
                        artworkPlaceholder
                            .frame(width: 180, height: 180)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                    }

                    VStack(spacing: 6) {
                        Text(snap.title ?? "Unknown Title")
                            .font(.title2)
                            .fontWeight(.semibold)
                            .multilineTextAlignment(.center)
                            .lineLimit(2)

                        Text(snap.artist ?? "Unknown Artist")
                            .font(.body)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                            .lineLimit(1)

                        if let album = snap.album {
                            Text(album)
                                .font(.subheadline)
                                .foregroundStyle(.tertiary)
                                .multilineTextAlignment(.center)
                                .lineLimit(1)
                        }
                    }

                    let isPlaying = snap.playing ?? true
                    HStack(spacing: 5) {
                        Image(systemName: isPlaying ? "play.fill" : "pause.fill")
                            .font(.caption2)
                        Text(isPlaying ? "Playing" : "Paused")
                            .font(.caption)
                            .fontWeight(.medium)
                    }
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 4)
                    .background(
                        Capsule()
                            .fill(Color.secondary.opacity(0.12))
                    )
                }
                .padding(.horizontal, 24)
            } else {
                VStack(spacing: 12) {
                    Image(systemName: "music.note")
                        .font(.system(size: 52))
                        .foregroundStyle(.secondary)
                    Text("Not Playing")
                        .font(.title3)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

    private var artworkPlaceholder: some View {
        RoundedRectangle(cornerRadius: 12)
            .fill(Color.secondary.opacity(0.15))
            .overlay(
                Image(systemName: "music.note")
                    .font(.system(size: 40))
                    .foregroundStyle(.secondary.opacity(0.5))
            )
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
