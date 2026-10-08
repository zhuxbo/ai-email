import Foundation
import Combine

@MainActor public final class SearchStore: ObservableObject {
    @Published public private(set) var accounts: [Account] = []
    @Published public private(set) var sync: [SyncStatus] = []
    @Published public private(set) var canWrite = false
    @Published public private(set) var items: [MailMessage] = []
    @Published public private(set) var total = 0
    @Published public private(set) var nextOffset = 0
    @Published public private(set) var loading = false
    @Published public private(set) var error: String?
    @Published public private(set) var accountsError: String?
    @Published public private(set) var selectedId: String?
    @Published public private(set) var detail: MailMessage?
    @Published public private(set) var detailLoading = false
    @Published public private(set) var detailError: String?
    private let api: any MailAPI
    private var query = ""
    private var accountId: String?
    private var generation = 0
    private var detailGeneration = 0
    private var queryTask: Task<Void, Never>?
    private var detailTask: Task<Void, Never>?
    private var accountTask: Task<Void, Never>?
    public init(api: any MailAPI) { self.api = api }
    deinit { queryTask?.cancel(); detailTask?.cancel(); accountTask?.cancel() }
    public func refreshAccounts() {
        accountTask?.cancel()
        accountTask = Task {
            do {
                let response = try await api.accounts()
                guard !Task.isCancelled else { return }
                accounts = response.accounts; sync = response.sync
                canWrite = response.canWrite == true; accountsError = nil
            } catch { if !Task.isCancelled { accountsError = error.localizedDescription; canWrite = false } }
        }
    }
    public func search(query: String, accountId: String?) {
        generation += 1; queryTask?.cancel()
        self.query = query; self.accountId = accountId
        items = []; total = 0; nextOffset = 0; error = nil
        openMessage(id: nil)
        fetch(offset: 0)
    }
    public func loadMore() {
        guard !loading, nextOffset < total else { return }
        fetch(offset: nextOffset)
    }
    private func fetch(offset: Int) {
        let current = generation, query = query, accountId = accountId
        loading = true; error = nil
        queryTask = Task {
            do {
                let page = try await api.messages(query: query, accountId: accountId, offset: offset)
                guard generation == current, !Task.isCancelled else { return }
                var seen = Set<String>()
                items = (offset == 0 ? page.items : items + page.items).filter { seen.insert($0.id).inserted }
                total = page.total; sync = page.sync
                nextOffset = page.items.isEmpty ? page.total : offset + page.items.count
                loading = false
            } catch {
                guard generation == current, !Task.isCancelled else { return }
                self.error = error.localizedDescription; loading = false
            }
        }
    }
    public func openMessage(id: String?) {
        detailGeneration += 1; detailTask?.cancel()
        selectedId = id; detail = nil; detailError = nil; detailLoading = id != nil
        guard let id else { return }
        let current = detailGeneration
        detailTask = Task {
            do {
                let value = try await api.message(id: id)
                guard current == detailGeneration, !Task.isCancelled else { return }
                detail = value; detailLoading = false
            } catch {
                guard current == detailGeneration, !Task.isCancelled else { return }
                detailError = error.localizedDescription; detailLoading = false
            }
        }
    }
}
