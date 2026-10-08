import Foundation
import Security

@MainActor public protocol CredentialStore {
    func connection() throws -> Connection?
    func saveConnection(_ value: Connection?) throws
    func pendingReply() throws -> PendingReply?
    func savePendingReply(_ value: PendingReply?) throws
}
@MainActor public final class KeychainVault: CredentialStore {
    private let service = "com.aiemail.search.macos"
    public init() {}
    private func query(_ name: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
         kSecAttrAccount as String: name, kSecAttrSynchronizable as String: false]
    }
    private func read<T: Decodable>(_ name: String) throws -> T? {
        var attributes = query(name)
        attributes[kSecReturnData as String] = true
        attributes[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(attributes as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data,
              let value = try? JSONDecoder().decode(T.self, from: data) else { throw ClientError.keychain }
        return value
    }
    private func write<T: Encodable>(_ name: String, _ value: T?) throws {
        let attributes = query(name)
        guard let value else {
            let status = SecItemDelete(attributes as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else { throw ClientError.keychain }
            return
        }
        let data = try JSONEncoder().encode(value)
        let update = [kSecValueData as String: data]
        var status = SecItemUpdate(attributes as CFDictionary, update as CFDictionary)
        if status == errSecItemNotFound {
            var item = attributes
            item[kSecValueData as String] = data
            item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            status = SecItemAdd(item as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw ClientError.keychain }
    }
    public func connection() throws -> Connection? {
        guard let saved: Connection = try read("connection") else { return nil }
        return try Connection(address: saved.url.absoluteString, token: saved.token, allowLocalHTTP: true)
    }
    public func saveConnection(_ value: Connection?) throws { try write("connection", value) }
    public func pendingReply() throws -> PendingReply? { try read("pending-reply") }
    public func savePendingReply(_ value: PendingReply?) throws { try write("pending-reply", value) }
}
