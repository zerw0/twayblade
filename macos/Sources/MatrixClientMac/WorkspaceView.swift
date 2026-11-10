import MatrixBridge
import SwiftUI

struct WorkspaceView: View {
    @EnvironmentObject private var model: ClientModel
    @FocusState private var searchFocused: Bool

    var body: some View {
        HStack(spacing: 0) {
            spaceRail
            NavigationSplitView {
                VStack(spacing: 0) {
                    HStack {
                        Text(model.navigationTitle).font(.headline).lineLimit(1)
                        Spacer()
                        Button { searchFocused = true } label: { Image(systemName: "magnifyingglass") }
                            .buttonStyle(.borderless).help("Find a conversation")
                    }.padding(.horizontal, 16).padding(.top, 18).padding(.bottom, 12)
                    TextField("Search", text: $model.search)
                        .textFieldStyle(.roundedBorder).focused($searchFocused)
                        .padding(.horizontal, 12).padding(.bottom, 10)
                    List(selection: Binding(get: { model.selectedRoom }, set: { if let id = $0 { model.selectRoom(id) } })) {
                        Section("Conversations") {
                            ForEach(model.visibleRooms) { room in
                                HStack(spacing: 9) {
                                    Image(systemName: "number").foregroundStyle(.secondary)
                                    Text(room.name).lineLimit(2)
                                        .fontWeight(room.unread > 0 ? .semibold : .regular)
                                    Spacer(minLength: 2)
                                    if room.unread > 0 { Text("\(room.unread)").font(.caption2.weight(.semibold)).foregroundStyle(.secondary) }
                                }.tag(room.id).padding(.vertical, 3)
                            }
                        }
                    }.listStyle(.sidebar)
                    if model.visibleRooms.isEmpty {
                        Text(model.spacesLoaded ? "No conversations here yet" : "Organizing your rooms…")
                            .font(.caption).foregroundStyle(.secondary).padding()
                    }
                    accountMenu.padding(12)
                }
                .navigationSplitViewColumnWidth(min: 210, ideal: 250, max: 340)
            } detail: {
                if let room = model.currentRoom { ConversationView(room: room) }
                else {
                    ContentUnavailableView("Choose a conversation", systemImage: "bubble.left.and.bubble.right",
                        description: Text("Select a room from the sidebar."))
                }
            }
        }
        .sheet(item: $model.profileSelection, onDismiss: { model.closeProfile() }) { selection in
            ProfileView(selection: selection).environmentObject(model)
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusConversationSearch)) { _ in searchFocused = true }
    }

    private var spaceRail: some View {
        VStack(spacing: 10) {
            railButton(title: "Home", symbol: "house", selected: model.selectedSpace == nil) { model.selectSpace(nil) }
            Divider().padding(.vertical, 4)
            ScrollView {
                VStack(spacing: 10) {
                    ForEach(model.spaces) { space in
                        Button { model.selectSpace(space.id) } label: {
                            VStack(spacing: 2) {
                                Text(String(space.name.prefix(1)).uppercased()).font(.headline)
                                if model.unread(space) > 0 { Text("\(model.unread(space))").font(.system(size: 9)) }
                                if space.level > 0 { Image(systemName: "arrow.turn.down.right").font(.system(size: 8)) }
                            }.frame(width: 38).frame(minHeight: 38)
                        }
                        .buttonStyle(.plain).padding(3)
                        .background(model.selectedSpace == space.id ? Color.accentColor.opacity(0.2) : .clear,
                                    in: RoundedRectangle(cornerRadius: 12))
                        .help(space.name).accessibilityLabel(space.name)
                    }
                }
            }
            Spacer(minLength: 0)
        }.padding(.horizontal, 8).padding(.top, 16).frame(width: 56)
            .background(.regularMaterial)
    }

    private func railButton(title: String, symbol: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol).font(.system(size: 18)).frame(width: 38, height: 38) }
            .buttonStyle(.plain).foregroundStyle(selected ? Color.accentColor : .secondary)
            .background(selected ? Color.accentColor.opacity(0.18) : .clear, in: RoundedRectangle(cornerRadius: 12))
            .help(title).accessibilityLabel(title)
    }

    private var accountMenu: some View {
        Menu {
            Button("My Profile") { model.openProfile(model.userID, displayName: "You") }
            Text(model.status)
            Divider()
            Button("Sign Out") { Task { await model.logout() } }.disabled(model.preview || model.busy)
        } label: {
            Label(model.preview ? "Offline preview" : "Account", systemImage: "person.crop.circle")
                .font(.callout).frame(maxWidth: .infinity, alignment: .leading)
        }.menuStyle(.borderlessButton)
    }
}

struct ConversationView: View {
    @EnvironmentObject private var model: ClientModel
    let room: NativeRoom
    @State private var followingLatest = true

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "number").font(.title3).foregroundStyle(.tint)
                VStack(alignment: .leading, spacing: 2) {
                    Text(room.name).font(.headline)
                    Text(model.currentSpace?.name ?? "Conversation").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
            }.padding(.horizontal, 20).padding(.vertical, 12)
                .background(.bar)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        Button(model.paginating ? "Loading…" : "Load Earlier") {
                            followingLatest = false
                            Task { await model.loadEarlier() }
                        }.buttonStyle(.borderless).font(.caption).disabled(model.preview || model.paginating || !model.timelineReady)
                        if model.messages.isEmpty {
                            Text(model.timelineReady ? "No messages to display yet." : "Loading conversation…")
                                .foregroundStyle(.secondary).padding()
                        }
                        ForEach(groupedMessages(model.messages)) { group in
                            messageGroup(group)
                        }
                        Color.clear.frame(height: 1).id("latest")
                            .onAppear { followingLatest = true }
                            .onDisappear { followingLatest = false }
                    }.padding(20)
                }
                .onChange(of: model.messages.count) { _, _ in
                    if followingLatest && !model.paginating { proxy.scrollTo("latest", anchor: .bottom) }
                }
                .onChange(of: room.id) { _, _ in followingLatest = true; proxy.scrollTo("latest", anchor: .bottom) }
                .overlay(alignment: .bottom) {
                    if !followingLatest {
                        Button("Jump to Latest", systemImage: "arrow.down") {
                            followingLatest = true; proxy.scrollTo("latest", anchor: .bottom)
                        }.buttonStyle(.bordered).padding(8)
                    }
                }
            }
            if !model.timelineStatus.isEmpty {
                Text(model.timelineStatus).font(.caption).foregroundStyle(.secondary).padding(.horizontal)
            }
            composer.padding(16)
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Message", text: Binding(get: { model.drafts[room.id] ?? "" }, set: { model.drafts[room.id] = $0 }), axis: .vertical)
                .textFieldStyle(.plain).lineLimit(1...6)
                .onSubmit { Task { await model.send() } }
                .disabled(model.preview || !model.timelineReady || model.busy)
            Button { Task { await model.send() } } label: {
                Image(systemName: "arrow.up").font(.system(size: 13, weight: .semibold)).frame(width: 22, height: 22)
            }.buttonStyle(.borderedProminent).clipShape(Circle())
                .disabled(model.preview || !model.timelineReady || model.sending.contains(room.id) ||
                          (model.drafts[room.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .help("Send message")
        }.padding(12).nativeComposerGlass()
    }

    private func messageGroup(_ group: MessageGroup) -> some View {
        let message = group.messages[0]
        let outgoing = message.sender == model.userID
        return HStack(alignment: .top, spacing: 10) {
            if outgoing { Spacer(minLength: 72) }
            if !outgoing {
                Button { model.openProfile(message.sender, displayName: message.displayName) } label: {
                    Text(String(message.displayName.prefix(1)).uppercased())
                        .font(.caption.weight(.semibold)).frame(width: 28, height: 28)
                        .background(Color.accentColor.opacity(0.15), in: Circle()).foregroundStyle(.tint)
                }.buttonStyle(.plain).help("View profile").accessibilityLabel("View \(message.displayName)'s profile")
            }
            VStack(alignment: outgoing ? .trailing : .leading, spacing: 5) {
                HStack(spacing: 8) {
                    Button { model.openProfile(message.sender, displayName: message.displayName) } label: {
                        Text(message.displayName).font(.caption.weight(.semibold))
                    }.buttonStyle(.plain).help("View profile")
                    Text(Date(timeIntervalSince1970: Double(message.timestampMs) / 1000), style: .time)
                        .font(.caption2).foregroundStyle(.secondary)
                }
                ForEach(Array(group.messages.enumerated()), id: \.offset) { _, entry in
                    VStack(alignment: outgoing ? .trailing : .leading, spacing: 3) {
                        Text(entry.body).textSelection(.enabled).padding(.horizontal, 12).padding(.vertical, 8)
                            .foregroundStyle(outgoing ? Color.white : Color.primary)
                            .background(outgoing ? Color.accentColor : Color(nsColor: .controlBackgroundColor),
                                        in: RoundedRectangle(cornerRadius: 17))
                            .help(Date(timeIntervalSince1970: Double(entry.timestampMs) / 1000).formatted(date: .abbreviated, time: .standard))
                        if !entry.delivery.isEmpty { Text(entry.delivery).font(.caption2).foregroundStyle(.secondary) }
                    }
                }
            }
            if !outgoing { Spacer(minLength: 50) }
        }.frame(maxWidth: .infinity, alignment: outgoing ? .trailing : .leading)
    }
}

extension View {
    @ViewBuilder func nativeComposerGlass() -> some View {
        if #available(macOS 26.0, *) {
            self.glassEffect(.regular, in: RoundedRectangle(cornerRadius: 22))
        } else {
            self.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
        }
    }
}
