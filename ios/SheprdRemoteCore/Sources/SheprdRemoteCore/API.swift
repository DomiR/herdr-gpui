import Foundation

/// The JSON the daemon's API takes and returns (`crates/sheprd-remote-control/src/web/api.rs`).
public enum API {
    /// `POST /api/pair/claim`.
    public struct ClaimRequest: Encodable, Equatable, Sendable {
        public let token: String
        public let name: String
        public let publicKey: String
        /// This phone's keys for the computer's relays, stored with the
        /// device when the desktop approves; nil while it has none.
        public let relay: RelayKeys?

        public init(token: String, name: String, publicKey: String, relay: RelayKeys? = nil) {
            self.token = token
            self.name = name
            self.publicKey = publicKey
            self.relay = relay
        }

        enum CodingKeys: String, CodingKey {
            case token, name, relay
            case publicKey = "public_key"
        }
    }

    public struct ClaimResponse: Decodable, Equatable, Sendable {
        public let claim: String
        /// The 6-digit matching code both screens show.
        public let code: String
    }

    /// `GET /api/pair/claim/{claim}`.
    public enum ClaimStatus: Decodable, Equatable, Sendable {
        case waiting
        case approved(Approval)
        case denied
        case expired

        public struct Approval: Decodable, Equatable, Sendable {
            public let device: String
            public let origins: [String]
            public let fingerprint: Fingerprint
            public let computer: String
        }

        enum CodingKeys: String, CodingKey { case state }

        public init(from decoder: Decoder) throws {
            let state = try decoder.container(keyedBy: CodingKeys.self).decode(String.self, forKey: .state)
            switch state {
            case "approved": self = .approved(try Approval(from: decoder))
            case "denied": self = .denied
            case "expired": self = .expired
            default: self = .waiting
            }
        }
    }

    /// `POST /api/session/challenge`.
    public struct ChallengeRequest: Encodable, Equatable, Sendable {
        public let device: String
        public init(device: String) { self.device = device }
    }

    public struct Challenge: Decodable, Equatable, Sendable {
        public let challenge: String
        public let nonce: String
    }

    /// `POST /api/session`.
    public struct SessionRequest: Encodable, Equatable, Sendable {
        public let challenge: String
        public let signature: String
        public init(challenge: String, signature: String) {
            self.challenge = challenge
            self.signature = signature
        }
    }

    public struct Session: Decodable, Equatable, Sendable {
        /// The bearer token.
        public let token: String
        /// When it stops being accepted, in milliseconds since 1970.
        public let expiresMs: UInt64

        enum CodingKeys: String, CodingKey {
            case token
            case expiresMs = "expires_ms"
        }
    }

    /// `POST /api/apns/test`: asks a computer to send one sealed test alert
    /// now, carrying `id`.
    public struct PushTestRequest: Encodable, Equatable, Sendable {
        public let id: String
        public init(id: String) { self.id = id }
    }

    /// What APNs told the computer about a test push.
    public struct PushTestAnswer: Decodable, Equatable, Sendable {
        public enum Outcome: String, Decodable, Sendable {
            case delivered, gone, limited, refused, unreachable
        }

        public let outcome: Outcome
        /// For `refused`: APNs's HTTP status and its reason, one of Apple's
        /// fixed names such as `BadDeviceToken`.
        public let status: Int?
        public let reason: String?
    }

    /// Where a computer sends one kind of push: a token as lowercase hex,
    /// which computers keep but never send to, so the app no longer sends
    /// one, or a handle the
    /// phone sealed to the push relay, written with the push relay it goes
    /// through (`crates/sheprd-remote-control/src/apns/recipient.rs`).
    public enum PushRecipient: Encodable, Equatable, Sendable, ExpressibleByStringLiteral {
        case token(String)
        case pushRelayHandle(String, environment: PushRelayEnvironment)

        public init(stringLiteral token: String) {
            self = .token(token)
        }

        private enum Keys: String, CodingKey {
            case environment, handle
        }

        public func encode(to encoder: Encoder) throws {
            switch self {
            case let .token(token):
                var container = encoder.singleValueContainer()
                try container.encode(token)
            case let .pushRelayHandle(handle, environment):
                var container = encoder.container(keyedBy: Keys.self)
                try container.encode(environment, forKey: .environment)
                try container.encode(handle, forKey: .handle)
            }
        }
    }

    /// How a computer said it delivers pushes, in its answer to
    /// `POST /api/apns`: only through the push relay. Older computers do
    /// not say, and some said `own_key`, a mode computers no longer have,
    /// which reads as none.
    public enum PushDelivery: String, Decodable, Equatable, Sendable {
        case pushRelay = "push_relay"
    }

    /// The answer to `POST /api/apns`.
    public struct APNsAnswer: Decodable, Equatable, Sendable {
        public let delivery: PushDelivery?

        public init(delivery: PushDelivery?) {
            self.delivery = delivery
        }

        enum CodingKeys: String, CodingKey {
            case delivery
        }

        /// A delivery this phone does not know reads as none, so a newer
        /// computer's answer is not an error.
        public init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            delivery = try? container.decodeIfPresent(PushDelivery.self, forKey: .delivery)
        }
    }

    /// `POST /api/apns`: the app's push-to-start token and one token per
    /// Live Activity it shows for that computer, or push relay handles for
    /// them. The daemon replaces its list of activities with this one.
    public struct APNsRegistration: Encodable, Equatable, Sendable {
        public struct Activity: Encodable, Equatable, Sendable {
            public let pane: String
            public let token: PushRecipient
            public init(pane: String, token: PushRecipient) {
                self.pane = pane
                self.token = token
            }
        }

        public let pushToStart: PushRecipient?
        public let activities: [Activity]
        /// The app's own APNs token, for sealed alerts.
        public let alert: PushRecipient?
        /// The public key alerts are sealed to (`NotificationSeal`), as
        /// base64url X9.63 bytes.
        public let notifyKey: String?
        /// Whether this phone keeps its Live Activities generic. Sent only
        /// when true, so a phone on the default still registers with older
        /// daemons, which refuse a key they do not know.
        public let genericActivities: Bool
        /// Whether a missing push-to-start or alert recipient goes out as an
        /// explicit `null`, which makes the computer drop the one it holds
        /// (`apns::Update`). Left out, it keeps it. Older daemons read the
        /// `null` as left out.
        public let clearsRecipients: Bool

        public init(
            pushToStart: PushRecipient?,
            activities: [Activity],
            alert: PushRecipient? = nil,
            notifyKey: String? = nil,
            genericActivities: Bool = false,
            clearsRecipients: Bool = false
        ) {
            self.pushToStart = pushToStart
            self.activities = activities
            self.alert = alert
            self.notifyKey = notifyKey
            self.genericActivities = genericActivities
            self.clearsRecipients = clearsRecipients
        }

        /// What a computer gets while this phone's push relay is off: no
        /// tokens, since computers push only through the push relay, and an
        /// explicit `null` for the push-to-start and alert recipients and an
        /// empty activity list, so it drops every handle it held. That stops
        /// its pushes even when revoking the handles at the push relay
        /// failed or was refused.
        public static func pushRelayOff(genericActivities: Bool) -> Self {
            Self(pushToStart: nil, activities: [], genericActivities: genericActivities, clearsRecipients: true)
        }

        enum CodingKeys: String, CodingKey {
            case activityDetail = "activity_detail"
            case pushToStart = "push_to_start"
            case notifyKey = "notify_key"
            case alert
            case activities
        }

        public func encode(to encoder: Encoder) throws {
            var container = encoder.container(keyedBy: CodingKeys.self)
            if let pushToStart {
                try container.encode(pushToStart, forKey: .pushToStart)
            } else if clearsRecipients {
                try container.encodeNil(forKey: .pushToStart)
            }
            try container.encode(activities, forKey: .activities)
            if let alert {
                try container.encode(alert, forKey: .alert)
            } else if clearsRecipients {
                try container.encodeNil(forKey: .alert)
            }
            try container.encodeIfPresent(notifyKey, forKey: .notifyKey)
            if genericActivities {
                try container.encode("generic", forKey: .activityDetail)
            }
        }
    }
}
