@preconcurrency import ActivityKit
import Foundation

/// The iPhone's Live Activity (docs/design.md §6), as the engine publishes
/// it (`Update.liveActivity`, decisions 40 and 65): none while the engine
/// says none, else one activity for the shift, started by the app (iOS
/// ends it after 8 hours; the next state starts another) and updated with
/// each state. Monokulo's pushes move it while Till is in the back.
///
/// The changes run one after another, in the order the engine sent them.
@MainActor
final class LiveActivityController {
    /// The activity's push token, each time iOS gives one, for phase I5 to
    /// register with Monokulo's push targets through the relay. Nothing
    /// hears it until then.
    var onPushToken: (@MainActor (Data) -> Void)?

    private var activity: Activity<TillActivityAttributes>?
    /// The last state the engine published, kept to start the activity
    /// again when Till comes forward (iOS only starts one from the front).
    private var latest: TillActivityAttributes.ContentState?
    private var attributes = TillActivityAttributes(storeName: "", location: "")
    private var queue: Task<Void, Never>?
    private var tokens: Task<Void, Never>?

    /// Shows `state`, or ends the activity for nil. `attributes` are used
    /// only when an activity starts.
    func show(_ state: TillActivityAttributes.ContentState?, attributes: TillActivityAttributes) {
        latest = state
        self.attributes = attributes
        enqueue { controller in await controller.apply(state) }
    }

    /// Till came forward: starts the activity if one should be up and
    /// isn't (a start from the back fails, and iOS ends one after 8 hours).
    func resume() {
        guard let latest else { return }
        enqueue { controller in
            if controller.current() == nil {
                await controller.apply(latest)
            }
        }
    }

    private func enqueue(_ work: @escaping @MainActor @Sendable (LiveActivityController) async -> Void) {
        let previous = queue
        queue = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            await work(self)
        }
    }

    private func apply(_ state: TillActivityAttributes.ContentState?) async {
        guard let state else {
            await endAll()
            return
        }
        let content = ActivityContent(state: state, staleDate: state.staleDate)
        if let activity = current() {
            await activity.update(content)
            return
        }
        guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        do {
            let started = try Activity<TillActivityAttributes>.request(
                attributes: attributes,
                content: content,
                pushType: .token
            )
            adopt(started)
        } catch {
            NSLog("till: the Live Activity didn't start: %@", String(describing: error))
        }
    }

    /// The activity still up: this run's, or one a previous run started.
    private func current() -> Activity<TillActivityAttributes>? {
        if let activity, LiveActivityController.isUp(activity.activityState) {
            return activity
        }
        activity = nil
        if let earlier = Activity<TillActivityAttributes>.activities.first(where: { LiveActivityController.isUp($0.activityState) }) {
            adopt(earlier)
            return earlier
        }
        return nil
    }

    private func adopt(_ activity: Activity<TillActivityAttributes>) {
        self.activity = activity
        tokens?.cancel()
        tokens = Task { [weak self] in
            for await token in activity.pushTokenUpdates {
                self?.onPushToken?(token)
            }
        }
    }

    /// Ends every activity of Till's, at once: no shift, or the switch off.
    private func endAll() async {
        tokens?.cancel()
        tokens = nil
        activity = nil
        for activity in Activity<TillActivityAttributes>.activities {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
    }

    /// Whether an activity in `state` can still be updated.
    private static func isUp(_ state: ActivityState) -> Bool {
        state == .active || state == .stale
    }
}
