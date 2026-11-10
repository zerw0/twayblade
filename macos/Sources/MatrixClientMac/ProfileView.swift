import MatrixBridge
import SwiftUI

struct ProfileSelection: Identifiable {
    let id: String
    let displayName: String
}

struct ProfileView: View {
    @EnvironmentObject private var model: ClientModel
    @Environment(\.dismiss) private var dismiss
    let selection: ProfileSelection

    var body: some View {
        VStack(spacing: 20) {
            HStack {
                Text(selection.id == model.userID ? "My Profile" : "Profile").font(.headline)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            Text(String((model.profile?.displayName ?? selection.displayName).prefix(1)).uppercased())
                .font(.system(size: 34, weight: .semibold)).frame(width: 80, height: 80)
                .background(.tint.opacity(0.15), in: Circle()).foregroundStyle(.tint)
            Text(model.profile?.displayName ?? selection.displayName).font(.title2.bold())
                .multilineTextAlignment(.center).textSelection(.enabled)
            Text(selection.id).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
            if model.profileLoading { ProgressView().controlSize(.small) }
            if let profile = model.profile {
                if let roomName = profile.roomName {
                    LabeledContent("Conversation", value: roomName)
                }
                if let name = profile.roomDisplayName, name != profile.displayName {
                    LabeledContent("Name in this room", value: name)
                }
                if !profile.serverProfileAvailable {
                    Text("Showing the profile saved with this conversation.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if !model.profileError.isEmpty {
                Text(model.profileError).font(.callout).foregroundStyle(.secondary)
                Button("Try Again") { model.openProfile(selection.id, displayName: selection.displayName) }
            }
            Button("Copy Matrix ID", systemImage: "doc.on.doc") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(selection.id, forType: .string)
            }
            if model.preview { Text("Offline sample profile").font(.caption).foregroundStyle(.secondary) }
        }
        .padding(24).frame(width: 360).fixedSize(horizontal: false, vertical: true)
    }
}
