// The effect views tap through the app's Feedback, which the mockups do not
// build; this stands in for it so they compile.
@MainActor
final class Feedback {
    static let shared = Feedback()
    enum Touch { case tap }
    func touch(_: Touch) {}
}
