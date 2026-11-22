import Foundation

/// A pull request the agent worked on in this chat:
/// `sheprd_chat_protocol::PullRequest`, kept by GitHub repository and
/// number.
public struct ChatPullRequest: Codable, Hashable, Sendable, Identifiable {
    /// The most a transcript keeps, as `MAX_PULL_REQUESTS`.
    public static let limit = 64

    /// What the agent did to it, each once, in the order first seen.
    public var actions: [Action]
    public var number: UInt64
    /// `OWNER/REPO`.
    public var repo: String
    /// `https://github.com/OWNER/REPO/pull/NUMBER`.
    public var url: String

    public var id: String { url }

    public init(repo: String, number: UInt64, actions: [Action] = []) {
        self.actions = actions
        self.number = number
        self.repo = repo
        url = "https://github.com/\(repo)/pull/\(number)"
    }

    enum CodingKeys: String, CodingKey { case actions, number, repo, url }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        actions = try c.decodeIfPresent([Action].self, forKey: .actions) ?? []
        number = try c.decode(UInt64.self, forKey: .number)
        repo = try c.decode(String.self, forKey: .repo)
        url = try c.decode(String.self, forKey: .url)
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if !actions.isEmpty { try c.encode(actions, forKey: .actions) }
        try c.encode(number, forKey: .number)
        try c.encode(repo, forKey: .repo)
        try c.encode(url, forKey: .url)
    }

    /// The account and repository name, when `repo` names both.
    public var ownerAndName: (owner: String, name: String)? {
        let parts = repo.split(separator: "/", omittingEmptySubsequences: false)
        guard parts.count == 2, !parts[0].isEmpty, !parts[1].isEmpty else { return nil }
        return (String(parts[0]), String(parts[1]))
    }

    /// Whether `other` is the same pull request: GitHub reads owners and
    /// repositories in any case.
    public func isSame(as other: Self) -> Bool {
        number == other.number && repo.lowercased() == other.repo.lowercased()
    }

    /// Something the agent did to a pull request.
    public enum Action: String, Codable, Hashable, Sendable {
        case created, pushed, edited, commented, reviewed, ready, merged, closed, reopened
        /// An action from a newer program.
        case other

        public init(from decoder: Decoder) throws {
            self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
        }
    }
}

extension Transcript {
    /// Adds `pull`, or the actions it does not have yet to the one already
    /// kept: `add_pull_request` in `change.rs`.
    mutating func record(_ pull: ChatPullRequest) throws(ChangeError) {
        guard pull.number > 0, pull.ownerAndName != nil, Set(pull.actions).count == pull.actions.count else {
            throw .pullRequest
        }
        if let index = pullRequests.firstIndex(where: { $0.isSame(as: pull) }) {
            for action in pull.actions where !pullRequests[index].actions.contains(action) {
                pullRequests[index].actions.append(action)
            }
            return
        }
        guard pullRequests.count < ChatPullRequest.limit else { throw .pullRequestLimit }
        pullRequests.append(pull)
    }
}
