import Foundation

extension Computer {
    /// The addresses signing in to the computer tries, in order: the one
    /// that answered `last`, the rest of its own, then `fallback` (its relay
    /// proxy's), each once. A fleet member or linked host may answer only
    /// through its relay, so a registration that tried the computer's own
    /// addresses alone would never reach it.
    public func sessionOrigins(last: String?, fallback: [String]) -> [String] {
        let listed = origins + fallback
        // A last address no longer listed, such as a stopped proxy's port,
        // is not tried.
        let first = last.map { listed.contains($0) ? [$0] : [] } ?? []
        var ordered: [String] = []
        for origin in first + listed where !ordered.contains(origin) {
            ordered.append(origin)
        }
        return ordered
    }
}
