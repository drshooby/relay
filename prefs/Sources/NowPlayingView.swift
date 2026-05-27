import SwiftUI

private struct NowPlayingSnapshot: Codable {
    var title: String?
    var artist: String?
    var album: String?
    var artworkUrl: String?
    var playing: Bool?
    var elapsedSecs: Double?
    var durationSecs: Double?
    var observedAtUnixMs: Double?

    enum CodingKeys: String, CodingKey {
        case title, artist, album
        case artworkUrl = "artwork_url"
        case playing
        case elapsedSecs = "elapsed_secs"
        case durationSecs = "duration_secs"
        case observedAtUnixMs = "observed_at_unix_ms"
    }
}

/// Format seconds into m:ss (under 1 hour) or h:mm:ss (1 hour+).
private func formatTime(_ seconds: Double) -> String {
    guard seconds.isFinite && seconds >= 0 else { return "0:00" }
    let total = Int(seconds)
    let h = total / 3600
    let m = (total % 3600) / 60
    let s = total % 60
    if h > 0 {
        return String(format: "%d:%02d:%02d", h, m, s)
    } else {
        return String(format: "%d:%02d", m, s)
    }
}

struct NowPlayingView: View {
    @State private var snapshot: NowPlayingSnapshot?
    @State private var pollTimer: Timer?
    /// High-frequency "now" for smooth ticking. Updated every 0.25 s.
    @State private var now: Date = Date()

    private var snapshotURL: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("relay/nowplaying.json")
    }

    // MARK: - Computed playback position

    private var currentElapsed: Double? {
        guard let snap = snapshot,
              let elapsed = snap.elapsedSecs else { return nil }
        guard snap.playing == true,
              let observedAtMs = snap.observedAtUnixMs else { return elapsed }
        let observedAt = Date(timeIntervalSince1970: observedAtMs / 1000.0)
        let delta = now.timeIntervalSince(observedAt)
        let projected = elapsed + max(0, delta)
        if let dur = snap.durationSecs {
            return min(projected, dur)
        }
        return projected
    }

    private var progress: Double {
        guard let elapsed = currentElapsed,
              let duration = snapshot?.durationSecs,
              duration > 0 else { return 0 }
        return min(elapsed / duration, 1.0)
    }

    // MARK: - Body

    var body: some View {
        VStack {
            Spacer()

            if let snap = snapshot {
                VStack(spacing: 16) {
                    // Artwork
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

                    // Track info
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

                    // Progress bar — only when duration is known
                    if let duration = snap.durationSecs, duration > 0,
                       let elapsed = currentElapsed {
                        let isPlaying = snap.playing ?? true
                        VStack(spacing: 4) {
                            ProgressView(value: progress)
                                .progressViewStyle(.linear)
                                .tint(.accentColor)
                                .animation(.linear(duration: 0.25), value: progress)

                            HStack {
                                HStack(spacing: 4) {
                                    if !isPlaying {
                                        Image(systemName: "pause.fill")
                                            .font(.caption2)
                                            .foregroundStyle(.tertiary)
                                    }
                                    Text(formatTime(elapsed))
                                        .font(.caption.monospacedDigit())
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                Text(formatTime(duration))
                                    .font(.caption.monospacedDigit())
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                    }
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
            // 1 s poll to pick up new snapshot data from the pipeline.
            pollTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
                loadSnapshot()
            }
        }
        .onDisappear {
            pollTimer?.invalidate()
            pollTimer = nil
        }
        // 0.25 s tick to smoothly advance the progress bar locally.
        .onReceive(
            Timer.publish(every: 0.25, on: .main, in: .common).autoconnect()
        ) { tick in
            now = tick
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
