import Foundation
import MatrixBridge

struct MessageGroup: Identifiable {
    let id: Int
    var messages: [NativeMessage]
}

func groupedMessages(_ messages: [NativeMessage], calendar: Calendar = .current) -> [MessageGroup] {
    var groups: [MessageGroup] = []
    for (index, message) in messages.enumerated() {
        let delta = groups.last?.messages.last.map { message.timestampMs.subtractingReportingOverflow($0.timestampMs) }
        if let previous = groups.last?.messages.last,
           previous.sender == message.sender,
           let delta, !delta.overflow, delta.partialValue >= 0, delta.partialValue < 300_000,
           calendar.isDate(Date(timeIntervalSince1970: Double(previous.timestampMs) / 1000),
                           inSameDayAs: Date(timeIntervalSince1970: Double(message.timestampMs) / 1000)) {
            groups[groups.count - 1].messages.append(message)
        } else {
            groups.append(MessageGroup(id: index, messages: [message]))
        }
    }
    return groups
}
