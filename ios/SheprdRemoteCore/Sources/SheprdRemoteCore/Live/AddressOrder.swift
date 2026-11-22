import Foundation

/// The order a computer's addresses are raced in. Each waits a little for
/// the one before it, so an earlier one wins whenever it answers at all.
public enum AddressOrder {
    /// `origins` with Tailscale's after the rest, keeping each group's
    /// order. On the same network a Tailscale address still answers, and
    /// listed first it won; its path can then stop carrying traffic while
    /// the live socket looks open, and sends hang on it. Away from home
    /// the local addresses fail at once and Tailscale's still get through.
    public static func preferred(_ origins: [String]) -> [String] {
        origins.filter { !isTailscale($0) } + origins.filter(isTailscale)
    }

    /// Whether `origin` reaches its computer through Tailscale: a
    /// `ts.net` name, or an address in the ranges Tailscale hands out
    /// (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`).
    public static func isTailscale(_ origin: String) -> Bool {
        guard let host = URLComponents(string: origin)?.host?.lowercased() else { return false }
        let bare = host.trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        if bare == "ts.net" || bare.hasSuffix(".ts.net") { return true }
        if bare.hasPrefix("fd7a:115c:a1e0:") { return true }
        let octets = bare.split(separator: ".", omittingEmptySubsequences: false).compactMap { UInt8($0) }
        return octets.count == 4 && octets[0] == 100 && (64...127).contains(octets[1])
    }
}
