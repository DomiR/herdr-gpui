import SheprdGitHub
import SheprdRemoteCore
import SwiftUI

/// The panel's Pull requests section, as the desktop rail's: each pull
/// request the agent worked on in this chat, newest first. A tap opens it
/// in the app's pull request screen. Its title and state come from the
/// phone's own lists when they know it: what the agent did says it was
/// merged, but not whether a pull request it closed or reopened is closed
/// now.
struct ChatPullRequestList: View {
    let pulls: [ChatPullRequest]

    private var direct: GitHubDirect { GitHubDirect.shared }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(pulls.reversed().enumerated()), id: \.element.id) { index, pull in
                if index > 0 { Divider().padding(.leading, 46) }
                if let route = Self.route(pull) {
                    NavigationLink(value: route) { row(pull, listed: direct.listed(route)) }
                        .buttonStyle(.plain)
                } else {
                    row(pull, listed: nil)
                }
            }
        }
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 14))
        .navigationDestination(for: PullRequestRoute.self) { route in
            PullRequestDetailScreen(route: route, title: direct.title(for: route))
        }
    }

    private func row(_ pull: ChatPullRequest, listed: GitHubSearchPull?) -> some View {
        let title = listed?.title
        let look = Self.look(pull, listed: listed)
        return HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: look.symbol)
                .foregroundStyle(look.color)
                .accessibilityLabel(look.label)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(title ?? "#\(pull.number)")
                    .font(.subheadline.weight(.medium))
                    .lineLimit(2)
                Text(Self.caption(pull, titled: title != nil))
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Image(systemName: "chevron.right")
                .font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 12).padding(.vertical, 10)
        .contentShape(.rect)
        .contextMenu {
            Button { UIPasteboard.general.string = pull.url } label: { Label("Copy Link", systemImage: "link") }
        }
        .accessibilityElement(children: .combine)
    }

    /// Where a row leads; the transcript keeps only github.com pull
    /// requests.
    static func route(_ pull: ChatPullRequest) -> PullRequestRoute? {
        guard let host = GitHubHost("github.com"), let (owner, repo) = pull.ownerAndName else { return nil }
        return PullRequestRoute(host: host, owner: owner, repo: repo, number: pull.number, url: URL(string: pull.url))
    }

    /// The repository and number, unless the title row shows the number
    /// already, then what the agent did.
    static func caption(_ pull: ChatPullRequest, titled: Bool) -> String {
        let name = titled ? "\(pull.repo) #\(pull.number)" : pull.repo
        let did = pull.actions.filter { $0 != .other }.map(\.word)
        return did.isEmpty ? name : "\(name) · \(did.joined(separator: ", "))"
    }

    /// Merged once the agent merged it, as a merge is never undone and the
    /// phone's lists may predate it; else its state as those lists know it;
    /// else unknown. Closed and reopened are kept once each in the order
    /// first seen, so they cannot tell which came last.
    static func look(_ pull: ChatPullRequest, listed: GitHubSearchPull?) -> PullRequestLook {
        if pull.actions.contains(.merged) { return .merged }
        return listed.map { PullRequestLook($0.state, draft: $0.isDraft) } ?? .unknown
    }
}

private extension ChatPullRequest.Action {
    var word: String {
        switch self {
        case .created: "created"
        case .pushed: "pushed"
        case .edited: "edited"
        case .commented: "commented"
        case .reviewed: "reviewed"
        case .ready: "marked ready"
        case .merged: "merged"
        case .closed: "closed"
        case .reopened: "reopened"
        case .other: ""
        }
    }
}
