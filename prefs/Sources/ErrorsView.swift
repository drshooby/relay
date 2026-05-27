import SwiftUI

private struct ErrorEntry: Identifiable {
    let id = UUID()
    let ts: String
    let component: String
    let message: String
}

struct ErrorsView: View {
    @State private var entries: [ErrorEntry] = []
    @State private var pollTimer: Timer?
    @State private var clearMessage: String?

    private var logURL: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent("relay/errors.jsonl")
    }

    var body: some View {
        VStack(spacing: 0) {
            if entries.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "checkmark.circle")
                        .font(.system(size: 40))
                        .foregroundStyle(.secondary)
                    Text("No errors recorded.")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                    Text("The tray icon turns red when something needs attention.")
                        .font(.subheadline)
                        .foregroundStyle(.tertiary)
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding()
            } else {
                List(entries) { entry in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(entry.component.uppercased())
                                .font(.caption2)
                                .fontWeight(.semibold)
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(Color.secondary.opacity(0.15))
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                            Spacer()
                            Text(entry.ts)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                        Text(entry.message)
                            .font(.body)
                    }
                    .padding(.vertical, 4)
                }
                .listStyle(.inset)
            }

            Divider()

            HStack(spacing: 12) {
                Button("Copy All") {
                    copyAll()
                }
                .buttonStyle(.bordered)

                Button("Clear", role: .destructive) {
                    clearLog()
                }
                .buttonStyle(.bordered)

                if let msg = clearMessage {
                    Text(msg)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer()
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
        }
        .onAppear {
            loadEntries()
            pollTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { _ in
                loadEntries()
            }
        }
        .onDisappear {
            pollTimer?.invalidate()
            pollTimer = nil
        }
    }

    // MARK: - Data loading

    private func loadEntries() {
        guard let url = logURL,
              let content = try? String(contentsOf: url, encoding: .utf8) else {
            entries = []
            return
        }

        // Parse newline-delimited JSON, reverse chronological (last line first).
        let lines = content.components(separatedBy: "\n")
            .filter { !$0.isEmpty }
            .reversed()

        entries = lines.compactMap { line -> ErrorEntry? in
            guard
                let data = line.data(using: .utf8),
                let obj = try? JSONSerialization.jsonObject(with: data) as? [String: String],
                let ts = obj["ts"],
                let component = obj["component"],
                let message = obj["message"]
            else { return nil }
            return ErrorEntry(ts: ts, component: component, message: message)
        }
    }

    // MARK: - Actions

    private func copyAll() {
        let text = entries
            .map { "[\($0.ts)] \($0.component): \($0.message)" }
            .joined(separator: "\n")
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        clearMessage = "Copied."
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            if clearMessage == "Copied." { clearMessage = nil }
        }
    }

    private func clearLog() {
        guard let url = logURL else { return }
        do {
            try FileManager.default.removeItem(at: url)
            entries = []
            clearMessage = "Log cleared."
        } catch {
            clearMessage = "Failed: \(error.localizedDescription)"
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            if clearMessage == "Log cleared." { clearMessage = nil }
        }
    }
}
