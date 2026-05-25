import Foundation
import MediaPlayer

func log(_ message: String) {
    fputs("[relay-helper] \(message)\n", stderr)
}

func emit(_ event: [String: String]) {
    if let data = try? JSONSerialization.data(withJSONObject: event),
       let line = String(data: data, encoding: .utf8) {
        print(line)
        fflush(stdout)
    }
}

/// Minimum elapsed-time delta (seconds) before emitting position_changed.
let positionChangeThresholdSecs = 3

// Track last emitted title to suppress duplicate track_changed events
var lastEmittedTitle: String? = nil
var lastReportedElapsedSecs: Int? = nil

func elapsedString(from info: [String: Any]?) -> String? {
    guard let elapsed = info?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? NSNumber else {
        return nil
    }
    let secs = elapsed.doubleValue.rounded()
    guard secs >= 0 else { return nil }
    return String(Int(secs))
}

func durationString(from info: [String: Any]?) -> String? {
    guard let duration = info?[MPMediaItemPropertyPlaybackDuration] as? NSNumber else {
        return nil
    }
    let secs = duration.doubleValue.rounded()
    guard secs > 0 else { return nil }
    return String(Int(secs))
}

func resetElapsedTracking() {
    lastReportedElapsedSecs = nil
}

func maybeEmitPositionChanged(newElapsedSecs: Int, source: String) {
    guard let last = lastReportedElapsedSecs else {
        lastReportedElapsedSecs = newElapsedSecs
        return
    }
    if abs(newElapsedSecs - last) >= positionChangeThresholdSecs {
        lastReportedElapsedSecs = newElapsedSecs
        emit(["event": "position_changed", "elapsed": String(newElapsedSecs)])
        log("\(source): position_changed elapsed=\(newElapsedSecs)")
    }
}

func emitTrackChanged(
    title: String,
    artist: String,
    album: String,
    elapsed: String?,
    duration: String?,
    source: String
) {
    lastEmittedTitle = title
    resetElapsedTracking()
    if let elapsed, let parsed = Int(elapsed) {
        lastReportedElapsedSecs = parsed
    }
    var event: [String: String] = ["event": "track_changed"]
    event["title"] = title
    if !artist.isEmpty { event["artist"] = artist }
    if !album.isEmpty { event["album"] = album }
    if let elapsed { event["elapsed"] = elapsed }
    if let duration { event["duration"] = duration }
    emit(event)
    log("\(source): track_changed: \(title) – \(artist)")
}

// Handle Apple Music playerInfo distributed notification
// userInfo keys: "Player State" (Playing/Paused/Stopped), "Name", "Artist", "Album"
func handlePlayerInfo(_ userInfo: [String: Any]) {
    let state = userInfo["Player State"] as? String ?? "Stopped"
    let name   = userInfo["Name"]   as? String ?? ""
    let artist = userInfo["Artist"] as? String ?? ""
    let album  = userInfo["Album"]  as? String ?? ""

    switch state {
    case "Playing":
        guard !name.isEmpty else {
            lastEmittedTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_stopped"])
            log("playing but no track name → playback_stopped")
            return
        }
        if name != lastEmittedTitle {
            let nowPlaying = MPNowPlayingInfoCenter.default().nowPlayingInfo
            let elapsed = elapsedString(from: nowPlaying)
            let duration = durationString(from: nowPlaying)
            emitTrackChanged(
                title: name,
                artist: artist,
                album: album,
                elapsed: elapsed,
                duration: duration,
                source: "Music.playerInfo"
            )
        }
    case "Paused":
        lastEmittedTitle = nil
        resetElapsedTracking()
        emit(["event": "playback_paused"])
        log("playback_paused")
    default:
        // "Stopped" or any other state
        lastEmittedTitle = nil
        resetElapsedTracking()
        emit(["event": "playback_stopped"])
        log("playback_stopped (state: \(state))")
    }
}

// Also observe MPNowPlayingInfoCenter for any player that integrates with the
// system Now Playing infrastructure (not just Apple Music)
class NowPlayingObserver: NSObject {
    var lastNowPlayingTitle: String? = nil

    func startObserving() {
        MPNowPlayingInfoCenter.default().addObserver(
            self,
            forKeyPath: "nowPlayingInfo",
            options: [.new],
            context: nil
        )
        MPNowPlayingInfoCenter.default().addObserver(
            self,
            forKeyPath: "playbackState",
            options: [.new],
            context: nil
        )
    }

    override func observeValue(
        forKeyPath keyPath: String?,
        of object: Any?,
        change: [NSKeyValueChangeKey: Any]?,
        context: UnsafeMutableRawPointer?
    ) {
        guard keyPath == "nowPlayingInfo" || keyPath == "playbackState" else {
            super.observeValue(forKeyPath: keyPath, of: object, change: change, context: context)
            return
        }

        let center = MPNowPlayingInfoCenter.default()
        let playbackState = center.playbackState
        let info = center.nowPlayingInfo

        switch playbackState {
        case .playing:
            let title  = info?[MPMediaItemPropertyTitle]  as? String ?? ""
            let artist = info?[MPMediaItemPropertyArtist] as? String ?? ""
            let album  = info?[MPMediaItemPropertyAlbumTitle] as? String ?? ""
            let elapsed = elapsedString(from: info)
            let duration = durationString(from: info)

            guard !title.isEmpty else {
                lastNowPlayingTitle = nil
                resetElapsedTracking()
                emit(["event": "playback_stopped"])
                log("MPNowPlaying: playing but no title → playback_stopped")
                return
            }
            if title != lastNowPlayingTitle {
                lastNowPlayingTitle = title
                emitTrackChanged(
                    title: title,
                    artist: artist,
                    album: album,
                    elapsed: elapsed,
                    duration: duration,
                    source: "MPNowPlaying"
                )
            } else if let elapsed, let parsed = Int(elapsed) {
                maybeEmitPositionChanged(newElapsedSecs: parsed, source: "MPNowPlaying")
            }
        case .paused:
            lastNowPlayingTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_paused"])
            log("MPNowPlaying: playback_paused")
        case .stopped, .interrupted:
            lastNowPlayingTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_stopped"])
            log("MPNowPlaying: playback_stopped")
        default:
            lastNowPlayingTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_stopped"])
            log("MPNowPlaying: playback_stopped (unknown state)")
        }
    }
}

// Query Apple Music directly via AppleScript and emit the corresponding event.
// MPNowPlayingInfoCenter is per-process on macOS — another app's now-playing data is
// not readable from this process, so we cannot rely on it for an on-demand snapshot.
// AppleScript triggers a one-time Apple Events permission prompt the first time it runs.
// Fields are joined with U+001F (unit separator) to survive titles containing pipes/tabs.
// Called both at startup and in response to {"command":"refresh"} from Rust.
func emitCurrentState(observer: NowPlayingObserver, reason: String) {
    let source = """
    tell application "Music"
        if it is not running then return "stopped"
        try
            set s to player state as text
            if s is "playing" then
                set t to name of current track
                set a to artist of current track
                set al to album of current track
                set pos to player position
                set dur to duration of current track
                return "playing" & (ASCII character 31) & t & (ASCII character 31) & a & (ASCII character 31) & al & (ASCII character 31) & (pos as text) & (ASCII character 31) & (dur as text)
            else if s is "paused" then
                return "paused"
            else
                return "stopped"
            end if
        on error
            return "stopped"
        end try
    end tell
    """
    var errorDict: NSDictionary?
    guard let script = NSAppleScript(source: source) else { return }
    let descriptor = script.executeAndReturnError(&errorDict)
    if let err = errorDict {
        log("\(reason): AppleScript query failed: \(err)")
        return
    }
    guard let result = descriptor.stringValue, !result.isEmpty else { return }
    let parts = result.components(separatedBy: "\u{001F}")
    switch parts.first {
    case "playing":
        guard parts.count >= 4 else { return }
        let title = parts[1], artist = parts[2], album = parts[3]
        guard !title.isEmpty else { return }
        let elapsed: String? = {
            guard parts.count >= 5 else { return nil }
            let pos = parts[4].trimmingCharacters(in: .whitespacesAndNewlines)
            guard !pos.isEmpty, let secs = Double(pos), secs >= 0 else { return nil }
            return String(Int(secs.rounded()))
        }()
        let duration: String? = {
            guard parts.count >= 6 else { return nil }
            let dur = parts[5].trimmingCharacters(in: .whitespacesAndNewlines)
            guard !dur.isEmpty, let secs = Double(dur), secs > 0 else { return nil }
            return String(Int(secs.rounded()))
        }()
        lastEmittedTitle = title
        observer.lastNowPlayingTitle = title
        emitTrackChanged(
            title: title,
            artist: artist,
            album: album,
            elapsed: elapsed,
            duration: duration,
            source: reason
        )
    case "paused":
        lastEmittedTitle = nil
        observer.lastNowPlayingTitle = nil
        resetElapsedTracking()
        emit(["event": "playback_paused"])
        log("\(reason): playback_paused")
    default: // "stopped" or anything unexpected
        lastEmittedTitle = nil
        observer.lastNowPlayingTitle = nil
        resetElapsedTracking()
        emit(["event": "playback_stopped"])
        log("\(reason): playback_stopped")
    }
}

// Set up Apple Music distributed notification observer (primary source for Apple Music)
DistributedNotificationCenter.default().addObserver(
    forName: NSNotification.Name("com.apple.Music.playerInfo"),
    object: nil,
    queue: .main
) { notification in
    let userInfo = notification.userInfo as? [String: Any] ?? [:]
    handlePlayerInfo(userInfo)
}

// Set up MPNowPlayingInfoCenter KVO observer (secondary; catches other MRCC-integrated players)
let nowPlayingObserver = NowPlayingObserver()
nowPlayingObserver.startObserving()
emitCurrentState(observer: nowPlayingObserver, reason: "startup")

// Read newline-delimited JSON commands from stdin. The only command today is
// {"command":"refresh"} — Rust sends it after reconnecting to Discord so the
// displayed activity gets corrected to the current Music.app state.
// readabilityHandler runs on a private dispatch queue; bounce to .main before
// touching any shared state (lastEmittedTitle, observer.lastNowPlayingTitle).
var stdinBuffer = Data()
FileHandle.standardInput.readabilityHandler = { handle in
    let chunk = handle.availableData
    if chunk.isEmpty { return } // parent closed stdin — keep running until SIGTERM
    stdinBuffer.append(chunk)
    while let nlIndex = stdinBuffer.firstIndex(of: 0x0A) {
        let lineData = stdinBuffer.subdata(in: 0..<nlIndex)
        stdinBuffer.removeSubrange(0...nlIndex)
        guard !lineData.isEmpty else { continue }
        guard let obj = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any],
              let cmd = obj["command"] as? String else {
            let line = String(data: lineData, encoding: .utf8) ?? "<binary>"
            log("ignoring malformed stdin line: \(line)")
            continue
        }
        switch cmd {
        case "refresh":
            DispatchQueue.main.async {
                emitCurrentState(observer: nowPlayingObserver, reason: "refresh")
            }
        default:
            log("unknown command: \(cmd)")
        }
    }
}

signal(SIGTERM) { _ in exit(0) }
signal(SIGINT)  { _ in exit(0) }

log("relay-helper started")
RunLoop.main.run()
