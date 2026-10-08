import SwiftUI
import MailSearchCore

@main @MainActor struct MailSearchApp: App {
    @StateObject private var session = ClientSession()
    var body: some Scene {
        Window("AI Email", id: "main") {
            RootView(session: session).frame(minWidth: 860, minHeight: 540)
        }.defaultSize(width: 1080, height: 740)
    }
}
struct RootView: View {
    @ObservedObject var session: ClientSession
    @State private var settings = false
    var body: some View {
        Group {
            if let search = session.search, let reply = session.reply {
                SearchView(store: search, reply: reply)
            } else {
                VStack(spacing: 16) {
                    Image(systemName: "envelope.badge.magnifyingglass").font(.system(size: 48)).foregroundStyle(.tint)
                    Text("搜索所有邮箱").font(.title2.weight(.semibold))
                    Text("连接你的 AI Email 服务器，搜索、阅读并回复邮件。").foregroundStyle(.secondary)
                    if let error = session.error { Text(error).foregroundStyle(.red) }
                    Button("设置服务器连接") { settings = true }.buttonStyle(.borderedProminent)
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .toolbar {
            ToolbarItem(placement: .automatic) {
                Button { settings = true } label: { Label("连接设置", systemImage: "gearshape") }
                    .help("服务器地址与访问令牌")
            }
        }
        .sheet(isPresented: $settings) { ConnectionView(session: session) }
    }
}
struct ConnectionView: View {
    @ObservedObject var session: ClientSession
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @State private var token = ""
    @State private var allowHTTP = false
    @State private var error: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("服务器连接").font(.title2.bold())
            TextField("https://mail.example.com/", text: $address).textFieldStyle(.roundedBorder)
                .accessibilityLabel("服务器根地址")
            SecureField("访问令牌", text: $token).textFieldStyle(.roundedBorder)
            Text("只读令牌用于搜索和阅读；可写令牌还可回复邮件。令牌保存在 macOS 钥匙串。").font(.callout).foregroundStyle(.secondary)
            #if DEBUG
            Toggle("允许本机 HTTP（调试）", isOn: $allowHTTP)
            #endif
            if let error { Text(error).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true) }
            HStack {
                if session.connection != nil {
                    Button("断开连接") {
                        do { try session.disconnect(); dismiss() } catch { self.error = error.localizedDescription }
                    }
                }
                Spacer()
                Button("取消") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("保存并连接") {
                    do { try session.configure(address: address, token: token, allowLocalHTTP: allowHTTP); dismiss() }
                    catch { self.error = error.localizedDescription }
                }.buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
            }
        }.padding(28).frame(width: 460)
            .onAppear { address = session.connection?.url.absoluteString ?? ""; token = session.connection?.token ?? "" }
    }
}
struct SearchView: View {
    @ObservedObject var store: SearchStore
    @ObservedObject var reply: ReplyController
    @State private var query = ""
    @State private var account = ""
    @State private var showReply = false
    private func search() { store.search(query: query, accountId: account.isEmpty ? nil : account) }
    var body: some View {
        HSplitView {
            VStack(spacing: 0) {
                VStack(spacing: 10) {
                    HStack {
                        TextField("搜索标题、正文、邮箱", text: $query).textFieldStyle(.roundedBorder).onSubmit(search)
                        Button(action: search) { Image(systemName: "magnifyingglass") }.help("搜索")
                    }
                    Picker("邮箱", selection: $account) {
                        Text("全部邮箱").tag("")
                        ForEach(store.accounts) { Text($0.displayName.isEmpty ? $0.email : "\($0.displayName) · \($0.email)").tag($0.id) }
                    }.onChange(of: account) { _ in search() }
                    HStack {
                        Text("\(store.total) 封邮件").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button { store.refreshAccounts(); search() } label: { Image(systemName: "arrow.clockwise") }.help("刷新")
                    }
                }.padding(16)
                if let error = store.accountsError { notice(error) }
                if store.sync.contains(where: { $0.status == "error" }) {
                    notice("部分邮箱同步失败，搜索结果可能不是最新。")
                }
                if let error = store.error { notice(error) }
                Divider()
                if store.items.isEmpty {
                    VStack(spacing: 12) {
                        if store.loading { ProgressView() }
                        else { Image(systemName: "magnifyingglass").font(.largeTitle).foregroundStyle(.secondary)
                            Text(store.error == nil ? "没有找到邮件" : "暂时无法搜索").font(.headline)
                            Text("试试其他关键词，或刷新服务器数据。").font(.callout).foregroundStyle(.secondary)
                        }
                    }.frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    List(selection: Binding(get: { store.selectedId }, set: { store.openMessage(id: $0) })) {
                        ForEach(store.items) { mail in
                            VStack(alignment: .leading, spacing: 5) {
                                Text(mail.subject.isEmpty ? "（无标题）" : mail.subject).font(.headline).lineLimit(2)
                                Text(mail.fromAddress).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                                Text(mail.bodyPreview).font(.callout).foregroundStyle(.secondary).lineLimit(2)
                                HStack { Text(mail.accountEmail).lineLimit(1); Spacer(); Text(dateLabel(mail.receivedAt)).lineLimit(1) }.font(.caption2).foregroundStyle(.secondary)
                            }.padding(.vertical, 6).tag(mail.id)
                        }
                    }.listStyle(.inset)
                    if store.loading { ProgressView().padding(8) }
                    else if store.nextOffset < store.total { Button("加载更多") { store.loadMore() }.padding(10) }
                }
            }.frame(minWidth: 300, idealWidth: 350, maxWidth: 440)
            VStack(spacing: 0) {
                if reply.pending != nil {
                    HStack {
                        Label("有一条回复需要核对发送状态", systemImage: "exclamationmark.circle")
                        Spacer(); Button("查看") { showReply = true }
                    }.font(.callout).padding(12).background(Color.yellow.opacity(0.12))
                }
                if store.detailLoading { ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity) }
                else if let error = store.detailError {
                    VStack(spacing: 12) { Text(error); Button("重新加载") { store.openMessage(id: store.selectedId) } }.frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if let mail = store.detail {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 16) {
                            HStack(alignment: .top) {
                                Text(mail.subject.isEmpty ? "（无标题）" : mail.subject).font(.title2.bold()).textSelection(.enabled)
                                Spacer()
                                Button("回复") { reply.resetDraft(); showReply = true }
                                    .disabled(!store.canWrite || reply.pending != nil)
                            }
                            VStack(alignment: .leading, spacing: 5) {
                                Text("发件人：\(mail.fromAddress)")
                                Text("收件人：\(mail.toAddresses.joined(separator: ", "))")
                                if let cc = mail.ccAddresses, !cc.isEmpty { Text("抄送：\(cc.joined(separator: ", "))") }
                                Text("所属邮箱：\(mail.accountEmail)")
                                Text(dateLabel(mail.receivedAt))
                            }.font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                            if !store.canWrite { Text("如需回复，请在连接设置中使用可写令牌。").font(.caption).foregroundStyle(.secondary) }
                            Divider()
                            Text(verbatim: mail.bodyText?.isEmpty == false ? mail.bodyText! : "（无正文）")
                                .font(.body).lineSpacing(5).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        }.padding(28).frame(maxWidth: 780, alignment: .leading).frame(maxWidth: .infinity)
                    }
                } else {
                    VStack(spacing: 12) { Image(systemName: "envelope.open").font(.system(size: 42)).foregroundStyle(.secondary); Text("选择邮件查看内容").foregroundStyle(.secondary) }.frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }.frame(minWidth: 440, maxWidth: .infinity, maxHeight: .infinity)
        }.sheet(isPresented: $showReply) { ReplyView(controller: reply, messageId: store.detail?.id) }
    }
    private func notice(_ text: String) -> some View { Text(text).font(.caption).foregroundStyle(.orange).frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.bottom, 8) }
}
func dateLabel(_ value: String) -> String {
    let formatter = ISO8601DateFormatter()
    if let date = formatter.date(from: value) { return date.formatted(date: .abbreviated, time: .shortened) }
    formatter.formatOptions.insert(.withFractionalSeconds)
    return formatter.date(from: value)?.formatted(date: .abbreviated, time: .shortened) ?? value
}
struct ReplyView: View {
    @ObservedObject var controller: ReplyController
    let messageId: String?
    @Environment(\.dismiss) private var dismiss
    @State private var bodyText = ""
    @State private var confirmClear = false
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("回复邮件").font(.title2.bold())
            if let pending = controller.pending, !controller.serverMatches {
                Text("该记录属于 \(pending.serverAddress)。请先核对原服务器和邮箱，不会向当前服务器发送。")
            }
            if let preview = controller.preview {
                Text("发件邮箱：\(preview.accountEmail)")
                Text("收件人：\(preview.to.joined(separator: ", "))").textSelection(.enabled)
                Text("主题：\(preview.subject)")
                ScrollView { Text(verbatim: preview.bodyText).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled) }
                    .padding(12).frame(height: 180).background(Color(nsColor: .textBackgroundColor)).cornerRadius(6)
                if controller.pending == nil, controller.status != "sent" { Button("修改正文") { controller.resetDraft() } }
            } else if controller.pending == nil {
                Text("输入简单回复，下一步核对收件人与正文。").foregroundStyle(.secondary)
                TextEditor(text: $bodyText).font(.body).padding(6).frame(height: 200).overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.3)))
            }
            if let status = controller.status { Text(statusLabel(status)).foregroundStyle(status == "sent" ? Color.green : Color.secondary).fixedSize(horizontal: false, vertical: true) }
            if let error = controller.error { Text(error).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true) }
            HStack {
                if controller.pending != nil {
                    Button("已核对，清除记录") { confirmClear = true }.disabled(controller.busy)
                    Button("查询状态") { Task { await controller.checkStatus() } }.disabled(controller.busy || !controller.serverMatches)
                }
                Spacer()
                if controller.busy { ProgressView().controlSize(.small) }
                Button("关闭") { dismiss() }.disabled(controller.busy).keyboardShortcut(.cancelAction)
                if controller.preview == nil && controller.pending == nil {
                    Button("预览回复") { if let messageId { Task { await controller.prepare(id: messageId, body: bodyText) } } }
                        .disabled(controller.busy || bodyText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || messageId == nil)
                        .buttonStyle(.borderedProminent)
                } else if controller.status != "sent" {
                    Button("确认发送") { Task { await controller.send() } }.disabled(!controller.canSend).buttonStyle(.borderedProminent)
                }
            }
        }.padding(24).frame(width: 600).interactiveDismissDisabled(controller.busy)
            .task { if controller.pending != nil { await controller.checkStatus() } }
            .alert("确认已核对邮箱？", isPresented: $confirmClear) {
                Button("取消", role: .cancel) {}
                Button("清除本地记录", role: .destructive) { controller.clearAfterVerification(); dismiss() }
            } message: { Text("清除记录不会撤回已发送的邮件。结果未知时，请先确认是否已经发出，避免重复回复。") }
    }
    private func statusLabel(_ status: String) -> String {
        switch status {
        case "prepared": return "已准备，请核对后确认发送。"
        case "sending": return "服务器正在处理，请查询状态。"
        case "sent": return "回复已被邮件服务器接受。"
        case "failed": return "发送失败，邮件服务器未确认接受。请核对后再处理。"
        case "expired": return "此回复准备已过期。"
        default: return "发送结果尚未确认。请查询状态或核对邮箱，不要重复发送。"
        }
    }
}
