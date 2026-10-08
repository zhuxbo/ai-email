import Foundation

public struct Account: Decodable, Identifiable, Sendable {
    public let id: String
    public let email: String
    public let displayName: String
}
public struct SyncStatus: Decodable, Sendable {
    public let accountId: String
    public let status: String
    public let lastSyncedAt: String?
    public let error: String?
}
public struct AccountResponse: Decodable, Sendable {
    public let accounts: [Account]
    public let canWrite: Bool?
    public let sync: [SyncStatus]
}
public struct MailMessage: Decodable, Identifiable, Sendable {
    public let id: String
    public let accountId: String
    public let accountEmail: String
    public let folder: String
    public let subject: String
    public let fromAddress: String
    public let toAddresses: [String]
    public let receivedAt: String
    public let bodyPreview: String
    public let bodyText: String?
    public let ccAddresses: [String]?
}
public struct MessagePage: Decodable, Sendable {
    public let items: [MailMessage]
    public let total: Int
    public let limit: Int
    public let offset: Int
    public let sync: [SyncStatus]
}
public protocol MailAPI: Sendable {
    func accounts() async throws -> AccountResponse
    func messages(query: String, accountId: String?, offset: Int) async throws -> MessagePage
    func message(id: String) async throws -> MailMessage
}

public struct ReplyPreview: Codable, Sendable {
    public let operationId: String
    public let status: String
    public let messageId: String
    public let accountId: String
    public let accountEmail: String
    public let to: [String]
    public let subject: String
    public let bodyText: String
}
public struct SendStatus: Decodable, Sendable {
    public let operationId: String
    public let status: String
    public let messageId: String
}
public struct PendingReply: Codable, Sendable {
    public let serverAddress: String
    public let preview: ReplyPreview
    public init(serverAddress: String, preview: ReplyPreview) {
        self.serverAddress = serverAddress
        self.preview = preview
    }
}
public protocol ReplyAPI: Sendable {
    func prepareReply(id: String, operationId: String, body: String) async throws -> ReplyPreview
    func sendReply(operationId: String) async throws -> SendStatus
    func sendStatus(operationId: String) async throws -> SendStatus
}
