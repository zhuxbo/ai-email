import Foundation
import MailSearchCore

final class StubProtocol: URLProtocol {
    static var handler: ((URLRequest) throws -> (Int, Data))?
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        do {
            let (status, data) = try Self.handler!(request)
            client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch { client?.urlProtocol(self, didFailWithError: error) }
    }
    override func stopLoading() {}
}
func checkAPI() async throws {
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [StubProtocol.self]
    let api = HTTPMailAPI(connection: try Connection(address: "https://mail.example.com/base", token: "secret"), configuration: configuration)
    var checkedRequest = false
    StubProtocol.handler = { request in
        try expect(request.httpMethod == "GET")
        try expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer secret")
        try expect(request.url?.path == "/base/api/messages")
        let query = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)!.queryItems!
        try expect(query.first { $0.name == "q" }?.value == "证书 100%_&")
        try expect(query.first { $0.name == "account_id" }?.value == "work")
        try expect(query.first { $0.name == "offset" }?.value == "30")
        checkedRequest = true
        return (200, Data(#"{"items":[],"total":0,"limit":30,"offset":30,"sync":[{"account_id":"work","status":"ok","last_synced_at":null,"error":null}]}"#.utf8))
    }
    let page = try await api.messages(query: "证书 100%_&", accountId: "work", offset: 30)
    try expect(checkedRequest && page.sync.first?.accountId == "work")
    StubProtocol.handler = { request in
        try expect(request.url!.absoluteString.contains("a%2Fb%3F"))
        return (404, Data())
    }
    var errorText = ""
    do { _ = try await api.message(id: "a/b?") } catch { errorText = error.localizedDescription }
    try expect(errorText == "邮件已不存在或超出保留范围，请刷新搜索。")
    StubProtocol.handler = { _ in (401, Data("secret mail body".utf8)) }
    errorText = ""
    do { _ = try await api.accounts() } catch { errorText = error.localizedDescription }
    try expect(errorText == "认证失败，请检查访问令牌。")
    let target = URLRequest(url: URL(string: "https://attacker.example")!)
    let redirected: URLRequest? = await withCheckedContinuation { continuation in
        NoRedirect().urlSession(URLSession.shared, task: URLSession.shared.dataTask(with: target), willPerformHTTPRedirection: HTTPURLResponse(url: target.url!, statusCode: 302, httpVersion: nil, headerFields: nil)!, newRequest: target) { continuation.resume(returning: $0) }
    }
    try expect(redirected == nil)
}
