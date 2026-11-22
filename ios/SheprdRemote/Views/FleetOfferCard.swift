import SheprdRemoteCore
import SwiftUI
import UIKit

/// A card at the top of Home for the fleet: "Join studio's fleet" once a
/// paired computer is in one, and once joined, "Your fleet has 3 more
/// computers you can control" with Add, which signs one `Link` per host.
private struct FleetOfferCard: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let membership = model.fleetMembership
        let offer = membership.offerDismissed || membership.busy ? nil : Offer(membership)
        Group {
            if let offer {
                card(offer)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.35), value: offer)
    }

    private enum Offer: Equatable {
        case join(Computer)
        case add([API.FleetHost])

        @MainActor init?(_ membership: FleetMembership) {
            if let computer = membership.joinable {
                self = .join(computer)
            } else if !membership.offers.isEmpty {
                self = .add(membership.offers)
            } else {
                return nil
            }
        }
    }

    private func card(_ offer: Offer) -> some View {
        let membership = model.fleetMembership
        let (title, detail, action): (String, String, String) = switch offer {
        case let .join(computer):
            ("\(model.displayName(computer)) is in a fleet", "Join it to control all your computers", "Join")
        case let .add(hosts):
            (
                "Your fleet has \(hosts.count == 1 ? "1 more computer" : "\(hosts.count) more computers") you can control",
                hosts.map(\.name).joined(separator: ", "),
                "Add"
            )
        }
        return HStack(spacing: 10) {
            Image(systemName: "person.3").foregroundStyle(.tint)
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(.subheadline.weight(.medium)).lineLimit(2)
                Text(detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            Button(action) {
                Task {
                    switch offer {
                    case let .join(computer): await membership.join(through: computer, name: UIDevice.current.name)
                    case let .add(hosts): await membership.link(hosts)
                    }
                }
            }
            .font(.subheadline.weight(.semibold))
            Button("Not Now", systemImage: "xmark") { membership.offerDismissed = true }
                .labelStyle(.iconOnly)
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 16).padding(.vertical, 8)
        .frame(maxWidth: .infinity)
        .glassEffect(.regular, in: .rect(cornerRadius: 20))
        .padding(.horizontal, 16).padding(.bottom, 8)
    }
}

extension View {
    /// Offers joining the fleet, or adding its other computers, above this
    /// list.
    func fleetOffer() -> some View {
        safeAreaInset(edge: .top) { FleetOfferCard() }
    }
}
