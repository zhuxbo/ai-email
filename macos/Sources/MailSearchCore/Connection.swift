import Foundation

public struct Connection: Codable, Equatable, Sendable {
    public let url: URL
    public let token: String
    public init(address: String, token: String, allowLocalHTTP: Bool = false) throws {
        guard var parts = URLComponents(string: address.trimmingCharacters(in: .whitespacesAndNewlines)),
              let host = parts.host, !host.isEmpty, parts.user == nil, parts.password == nil,
              parts.query == nil, parts.fragment == nil,
              !token.isEmpty, token.unicodeScalars.allSatisfy({ (33...126).contains(Int($0.value)) })
        else { throw ClientError.invalidConnection }
        var localHTTP = false
        #if DEBUG
        localHTTP = allowLocalHTTP && parts.scheme == "http" && ["localhost", "127.0.0.1", "[::1]", "::1"].contains(host)
        #endif
        guard parts.scheme == "https" || localHTTP else { throw ClientError.invalidConnection }
        if !parts.path.hasSuffix("/") { parts.path += "/" }
        guard let url = parts.url else { throw ClientError.invalidConnection }
        self.url = url
        self.token = token
    }
    public func validateReplacement(pendingServerAddress: String?, busy: Bool) throws {
        guard !busy, pendingServerAddress == nil || pendingServerAddress == url.absoluteString
        else { throw ClientError.pendingReply }
    }
}
public enum ClientError: Error, LocalizedError {
    case invalidConnection, unauthorized, forbidden, notFound, unavailable, invalidResponse, keychain, invalidReply, pendingReply
    public var errorDescription: String? {
        switch self {
        case .invalidConnection: return "请输入 HTTPS 服务根地址和有效的访问令牌。"
        case .unauthorized: return "认证失败，请检查访问令牌。"
        case .forbidden: return "此令牌没有回复权限，请在设置中使用可写令牌。"
        case .notFound: return "邮件已不存在或超出保留范围，请刷新搜索。"
        case .unavailable: return "请求失败，请检查服务器连接后重试。"
        case .invalidResponse: return "服务器响应无效，请确认服务地址和版本。"
        case .keychain: return "无法访问钥匙串，未保存连接或发送记录。"
        case .invalidReply: return "请输入回复正文。"
        case .pendingReply: return "有未确认的发送记录，请先查询状态或核对邮箱。"
        }
    }
}
