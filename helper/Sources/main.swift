import AppKit
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
/// Must match `POSITION_CHANGE_THRESHOLD_SECS` in `src/constants.rs`.
let positionChangeThresholdSecs = 3

// Track last emitted title to suppress duplicate track_changed events
var lastEmittedTitle: String? = nil
var lastReportedElapsedSecs: Int? = nil
// Track title at the moment of pause so resume can be detected and routed to AppleScript
var lastPausedTitle: String? = nil

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
            lastPausedTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_stopped"])
            log("playing but no track name → playback_stopped")
            return
        }
        if name == lastPausedTitle {
            // Resume on same track — use AppleScript for accurate position
            lastPausedTitle = nil
            emitCurrentState(observer: nowPlayingObserver, reason: "Music.playerInfo.resume")
        } else if name != lastEmittedTitle {
            // New track (or first play after clear)
            lastPausedTitle = nil
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
        lastPausedTitle = lastEmittedTitle
        lastEmittedTitle = nil
        resetElapsedTracking()
        emit(["event": "playback_paused"])
        log("playback_paused")
    default:
        // "Stopped" or any other state
        lastEmittedTitle = nil
        lastPausedTitle = nil
        resetElapsedTracking()
        emit(["event": "playback_stopped"])
        log("playback_stopped (state: \(state))")
    }
}

// Also observe MPNowPlayingInfoCenter for any player that integrates with the
// system Now Playing infrastructure (not just Apple Music)
class NowPlayingObserver: NSObject {
    var lastNowPlayingTitle: String? = nil
    // Title saved at pause time — used to detect resume and trigger AppleScript fallback
    var lastPausedNowPlayingTitle: String? = nil

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
                lastPausedNowPlayingTitle = nil
                resetElapsedTracking()
                emit(["event": "playback_stopped"])
                log("MPNowPlaying: playing but no title → playback_stopped")
                return
            }
            if title == lastPausedNowPlayingTitle {
                // Resume on same track — use AppleScript for accurate position
                lastPausedNowPlayingTitle = nil
                emitCurrentState(observer: self, reason: "MPNowPlaying.resume")
            } else if title != lastNowPlayingTitle {
                // New track (or first play after clear)
                lastPausedNowPlayingTitle = nil
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
            lastPausedNowPlayingTitle = lastNowPlayingTitle
            lastNowPlayingTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_paused"])
            log("MPNowPlaying: playback_paused")
        case .stopped, .interrupted:
            lastNowPlayingTitle = nil
            lastPausedNowPlayingTitle = nil
            resetElapsedTracking()
            emit(["event": "playback_stopped"])
            log("MPNowPlaying: playback_stopped")
        default:
            lastNowPlayingTitle = nil
            lastPausedNowPlayingTitle = nil
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
//
// On the resume path, Music.app sometimes hasn't updated player position yet when
// the AppleScript runs. If elapsed is missing, we retry once after 200 ms. If the
// second attempt also yields no position, we skip emitting track_changed entirely so
// the Rust pipeline can use its cached+projected position instead.
func emitCurrentState(observer: NowPlayingObserver, reason: String, isRetry: Bool = false) {
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
        let errNum = (err[NSAppleScript.errorNumber] as? NSNumber)?.intValue ?? 0
        if errNum == -1743 {
            emit(["event": "permission_denied"])
            log("\(reason): permission_denied (errAEEventNotPermitted)")
            return
        }
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
        // On the resume path, Music.app can lag updating player position,
        // briefly reporting 0 before the real position is available.
        // Treat elapsed==nil OR (resume + elapsed=="0") as unreliable: retry
        // once after 200 ms. If still unreliable, skip track_changed entirely
        // so the Rust pipeline keeps its cached+projected position instead.
        let elapsedUnreliable = elapsed == nil || (reason.contains("resume") && elapsed == "0")
        if elapsedUnreliable {
            if isRetry {
                log("\(reason): elapsed still unreliable after retry, skipping track_changed")
                return
            } else {
                log("\(reason): elapsed unreliable (\(elapsed ?? "nil")), retrying in 200ms")
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
                    emitCurrentState(observer: observer, reason: reason, isRetry: true)
                }
                return
            }
        }
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

// Observe Music.app lifecycle so Discord activity clears immediately on quit/crash (#33)
NSWorkspace.shared.notificationCenter.addObserver(
    forName: NSWorkspace.didTerminateApplicationNotification,
    object: nil,
    queue: .main
) { note in
    guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
          app.bundleIdentifier == "com.apple.Music" else { return }
    lastEmittedTitle = nil
    lastPausedTitle = nil
    nowPlayingObserver.lastNowPlayingTitle = nil
    nowPlayingObserver.lastPausedNowPlayingTitle = nil
    resetElapsedTracking()
    emit(["event": "playback_stopped"])
    log("Music.app terminated → playback_stopped")
}

NSWorkspace.shared.notificationCenter.addObserver(
    forName: NSWorkspace.didLaunchApplicationNotification,
    object: nil,
    queue: .main
) { note in
    guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
          app.bundleIdentifier == "com.apple.Music" else { return }
    emit(["event": "playback_stopped"])
    log("Music.app launched → playback_stopped (resets stale state)")
}

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
