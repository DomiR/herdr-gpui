import Foundation

/// Whether a request may have been taken though its answer said otherwise
/// or never came: `status` is the HTTP status answered, nil when none came
/// back. A refusal (4xx) means the computer did not take it; no answer, or
/// a failure on its side, may hide one that did, so what the request
/// carries (a join's keys, a link's) is kept until the fleet says.
public func fleetRequestMayHaveLanded(status: Int?) -> Bool {
    guard let status else { return true }
    return status >= 500
}
