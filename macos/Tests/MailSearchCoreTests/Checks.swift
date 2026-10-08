import Foundation
import MailSearchCore

struct CheckFailure: Error { let message: String }
func expect(_ condition: @autoclosure () -> Bool, _ message: String = #function, file: String = #fileID, line: Int = #line) throws {
    if !condition() { throw CheckFailure(message: "\(file):\(line) \(message)") }
}
func expectThrow(_ body: () throws -> Void) throws {
    do { try body() } catch { return }
    throw CheckFailure(message: "Expected an error")
}
func decode<T: Decodable>(_ json: String) throws -> T {
    let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(T.self, from: Data(json.utf8))
}
@main struct Checks {
    @MainActor static func main() async {
        do {
            try checkConnection()
            try await checkAPI()
            try await checkControllers()
            print("PASS: connection, API, search races, pagination and reply recovery checks")
        } catch {
            fputs("FAIL: \(error)\n", stderr)
            exit(1)
        }
    }
}
