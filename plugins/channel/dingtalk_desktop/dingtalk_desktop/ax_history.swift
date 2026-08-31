import ApplicationServices
import Foundation

struct Input: Decodable {
    let pid: pid_t
    let limit: Int
}

struct Message: Encodable {
    let text: String
    let incoming: Bool
}

struct Output: Encodable {
    let ok: Bool
    let messages: [Message]
    let error: String?
}

func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    let error = AXUIElementCopyAttributeValue(
        element,
        name as CFString,
        &value
    )
    return error == .success ? value : nil
}

func stringAttribute(_ element: AXUIElement, _ name: String) -> String {
    return attribute(element, name) as? String ?? ""
}

func elementsAttribute(
    _ element: AXUIElement,
    _ name: String
) -> [AXUIElement] {
    return attribute(element, name) as? [AXUIElement] ?? []
}

func findIdentifier(
    _ element: AXUIElement,
    _ wanted: String,
    _ depth: Int,
    _ visited: inout Set<CFHashCode>
) -> AXUIElement? {
    let key = CFHash(element)
    guard !visited.contains(key) else {
        return nil
    }
    visited.insert(key)
    if stringAttribute(element, kAXIdentifierAttribute) == wanted {
        return element
    }
    if stringAttribute(element, kAXRoleAttribute) == kAXTableRole {
        return nil
    }
    guard depth > 0 else {
        return nil
    }
    for child in elementsAttribute(element, kAXChildrenAttribute) {
        if let found = findIdentifier(
            child,
            wanted,
            depth - 1,
            &visited
        ) {
            return found
        }
    }
    return nil
}

func findTable(
    _ element: AXUIElement,
    _ depth: Int,
    _ visited: inout Set<CFHashCode>
) -> AXUIElement? {
    let key = CFHash(element)
    guard !visited.contains(key) else {
        return nil
    }
    visited.insert(key)
    if stringAttribute(element, kAXRoleAttribute) == kAXTableRole {
        return element
    }
    guard depth > 0 else {
        return nil
    }
    for child in elementsAttribute(element, kAXChildrenAttribute) {
        if let found = findTable(child, depth - 1, &visited) {
            return found
        }
    }
    return nil
}

func collectRow(
    _ element: AXUIElement,
    _ depth: Int,
    _ texts: inout [String],
    _ receiving: inout Bool,
    _ sending: inout Bool,
    _ visited: inout Set<CFHashCode>
) {
    let key = CFHash(element)
    guard !visited.contains(key) else {
        return
    }
    visited.insert(key)
    let semantics = (
        stringAttribute(element, kAXDescriptionAttribute)
        + " "
        + stringAttribute(element, kAXHelpAttribute)
    ).lowercased()
    receiving = receiving || semantics.contains("session msg receiving")
    sending = sending || semantics.contains("session msg sending")
    let role = stringAttribute(element, kAXRoleAttribute)
    if role == kAXStaticTextRole || role == kAXTextAreaRole {
        let text = stringAttribute(element, kAXValueAttribute)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if !text.isEmpty {
            texts.append(text)
        }
    }
    guard depth > 0 else {
        return
    }
    for child in elementsAttribute(element, kAXChildrenAttribute) {
        collectRow(
            child,
            depth - 1,
            &texts,
            &receiving,
            &sending,
            &visited
        )
    }
}

func readMessages(_ input: Input) throws -> [Message] {
    let app = AXUIElementCreateApplication(input.pid)
    AXUIElementSetMessagingTimeout(app, 2.0)
    var identifierVisited = Set<CFHashCode>()
    guard let window = elementsAttribute(app, kAXWindowsAttribute).first,
          let chat = findIdentifier(
              window,
              "ChatSplitView",
              8,
              &identifierVisited
          ) else {
        throw NSError(
            domain: "DingTalkDesktop",
            code: 1,
            userInfo: [
                NSLocalizedDescriptionKey:
                    "DingTalk semantic message table is unavailable"
            ]
        )
    }
    var tableVisited = Set<CFHashCode>()
    guard let table = findTable(chat, 6, &tableVisited) else {
        throw NSError(
            domain: "DingTalkDesktop",
            code: 2,
            userInfo: [
                NSLocalizedDescriptionKey:
                    "DingTalk semantic message table is unavailable"
            ]
        )
    }
    let rows = elementsAttribute(table, kAXRowsAttribute)
    let limit = max(4, min(input.limit, 30))
    var messages: [Message] = []
    for row in rows.suffix(limit) {
        var texts: [String] = []
        var receiving = false
        var sending = false
        var rowVisited = Set<CFHashCode>()
        collectRow(
            row,
            8,
            &texts,
            &receiving,
            &sending,
            &rowVisited
        )
        guard receiving != sending,
              let text = texts.max(by: { $0.count < $1.count }) else {
            continue
        }
        messages.append(Message(text: text, incoming: receiving))
    }
    return messages
}

do {
    let input = try JSONDecoder().decode(
        Input.self,
        from: FileHandle.standardInput.readDataToEndOfFile()
    )
    let output = Output(
        ok: true,
        messages: try readMessages(input),
        error: nil
    )
    print(String(data: try JSONEncoder().encode(output), encoding: .utf8)!)
} catch {
    let output = Output(ok: false, messages: [], error: error.localizedDescription)
    print(String(data: try JSONEncoder().encode(output), encoding: .utf8)!)
}
