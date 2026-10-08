import Foundation

package final class NoRedirect: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    package func urlSession(_ session: URLSession, task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
        completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}
public final class HTTPMailAPI: MailAPI, ReplyAPI, @unchecked Sendable {
    private let connection: Connection
    private let session: URLSession
    public init(connection: Connection, configuration: URLSessionConfiguration = .ephemeral) {
        self.connection = connection
        configuration.urlCache = nil
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.timeoutIntervalForRequest = 30
        configuration.timeoutIntervalForResource = 180
        session = URLSession(configuration: configuration, delegate: NoRedirect(), delegateQueue: nil)
    }
    deinit { session.invalidateAndCancel() }
    private func request<T: Decodable>(_ path: [String], query: [URLQueryItem] = [], body: [String: String]? = nil) async throws -> T {
        // Encode each segment independently so opaque message IDs cannot become a new route.
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._~"))
        let encoded = path.map { $0.addingPercentEncoding(withAllowedCharacters: allowed)! }.joined(separator: "/")
        guard var parts = URLComponents(string: connection.url.absoluteString + encoded) else { throw ClientError.invalidConnection }
        if !query.isEmpty { parts.queryItems = query }
        guard let url = parts.url else { throw ClientError.invalidConnection }
        var request = URLRequest(url: url)
        request.setValue("Bearer " + connection.token, forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let body {
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        do {
            let (data, response) = try await session.data(for: request)
            guard let response = response as? HTTPURLResponse else { throw ClientError.invalidResponse }
            switch response.statusCode {
            case 200..<300: break
            case 401: throw ClientError.unauthorized
            case 403: throw ClientError.forbidden
            case 404: throw ClientError.notFound
            default: throw ClientError.unavailable
            }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            guard let result = try? decoder.decode(T.self, from: data) else { throw ClientError.invalidResponse }
            return result
        } catch is CancellationError { throw CancellationError() }
        catch let error as ClientError { throw error }
        catch { if Task.isCancelled { throw CancellationError() }; throw ClientError.unavailable }
    }
    public func accounts() async throws -> AccountResponse { try await request(["api", "accounts"]) }
    public func messages(query: String, accountId: String?, offset: Int) async throws -> MessagePage {
        var items = [URLQueryItem(name: "q", value: query), URLQueryItem(name: "limit", value: "30"), URLQueryItem(name: "offset", value: String(offset))]
        if let accountId { items.append(URLQueryItem(name: "account_id", value: accountId)) }
        return try await request(["api", "messages"], query: items)
    }
    public func message(id: String) async throws -> MailMessage { try await request(["api", "messages", id]) }
    public func prepareReply(id: String, operationId: String, body: String) async throws -> ReplyPreview {
        try await request(["api", "messages", id, "reply", "prepare"], body: ["operation_id": operationId, "body_text": body])
    }
    public func sendReply(operationId: String) async throws -> SendStatus {
        try await request(["api", "send", operationId, "send"], body: [:])
    }
    public func sendStatus(operationId: String) async throws -> SendStatus {
        try await request(["api", "send", operationId])
    }
}
