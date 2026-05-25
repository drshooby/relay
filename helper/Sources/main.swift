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

// Track last emitted title to suppress duplicate track_changed events
var lastEmittedTitle: String? = nil

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
            emit(["event": "playback_stopped"])
            log("playing but no track name → playback_stopped")
            return
        }
        if name != lastEmittedTitle {
            lastEmittedTitle = name
            var event: [String: String] = ["event": "track_changed"]
            event["title"]  = name
            if !artist.isEmpty { event["artist"] = artist }
            if !album.isEmpty  { event["album"]  = album  }
            emit(event)
            log("track_changed: \(name) – \(artist)")
        }
    case "Paused":
        lastEmittedTitle = nil
        emit(["event": "playback_paused"])
        log("playback_paused")
    default:
        // "Stopped" or any other state
        lastEmittedTitle = nil
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

            guard !title.isEmpty else {
                lastNowPlayingTitle = nil
                emit(["event": "playback_stopped"])
                log("MPNowPlaying: playing but no title → playback_stopped")
                return
            }
            if title != lastNowPlayingTitle {
                lastNowPlayingTitle = title
                var event: [String: String] = ["event": "track_changed"]
                event["title"]  = title
                if !artist.isEmpty { event["artist"] = artist }
                if !album.isEmpty  { event["album"]  = album  }
                emit(event)
                log("MPNowPlaying track_changed: \(title)")
            }
        case .paused:
            lastNowPlayingTitle = nil
            emit(["event": "playback_paused"])
            log("MPNowPlaying: playback_paused")
        case .stopped, .interrupted:
            lastNowPlayingTitle = nil
            emit(["event": "playback_stopped"])
            log("MPNowPlaying: playback_stopped")
        default:
            lastNowPlayingTitle = nil
            emit(["event": "playback_stopped"])
            log("MPNowPlaying: playback_stopped (unknown state)")
        }
    }
}

// Query Apple Music directly via AppleScript at startup and emit an initial event.
// MPNowPlayingInfoCenter is per-process on macOS — another app's now-playing data is
// not readable from this process, so we cannot rely on it for the startup snapshot.
// AppleScript triggers a one-time Apple Events permission prompt the first time it runs.
// Fields are joined with U+001F (unit separator) to survive titles containing pipes/tabs.
func emitStartupState(observer: NowPlayingObserver) {
    let source = """
    tell application "Music"
        if it is not running then return ""
        try
            if player state is playing then
                set t to name of current track
                set a to artist of current track
                set al to album of current track
                return t & (ASCII character 31) & a & (ASCII character 31) & al
            end if
            return ""
        on error
            return ""
        end try
    end tell
    """
    var errorDict: NSDictionary?
    guard let script = NSAppleScript(source: source) else { return }
    let descriptor = script.executeAndReturnError(&errorDict)
    if let err = errorDict {
        log("startup: AppleScript query failed: \(err)")
        return
    }
    guard let result = descriptor.stringValue, !result.isEmpty else { return }
    let parts = result.components(separatedBy: "\u{001F}")
    guard parts.count == 3 else { return }
    let title = parts[0], artist = parts[1], album = parts[2]
    guard !title.isEmpty else { return }
    lastEmittedTitle = title
    observer.lastNowPlayingTitle = title
    var event: [String: String] = ["event": "track_changed"]
    event["title"]  = title
    if !artist.isEmpty { event["artist"] = artist }
    if !album.isEmpty  { event["album"]  = album  }
    emit(event)
    log("startup: track_changed: \(title) – \(artist)")
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
emitStartupState(observer: nowPlayingObserver)

signal(SIGTERM) { _ in exit(0) }
signal(SIGINT)  { _ in exit(0) }

log("relay-helper started")
RunLoop.main.run()
