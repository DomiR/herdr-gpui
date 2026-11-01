# The Orchestrator's Linear section: the board, its filters, the Connect
# Linear and New issue dialogs, and the pull request grouping by creator.

# What a finished Linear action did, in the banner.
orchestrator-linear-done-connected = Linear connected
orchestrator-linear-done-issue-created = Linear issue created
orchestrator-linear-done-key-removed = Linear key removed

# Filters and the list's switches.
orchestrator-linear-facets-status = STATUS
orchestrator-linear-filter-current-cycle = Current cycle
orchestrator-linear-layout-list = List
orchestrator-linear-layout-board = Board
orchestrator-linear-hide-statuses = Hide statuses
orchestrator-linear-no-statuses-yet = No statuses listed yet

# An issue's tracker details.
orchestrator-linear-assignee = Assignee
orchestrator-linear-cycle = Cycle
orchestrator-linear-no-status = No status
orchestrator-linear-no-assignee = No assignee
# $assignee is a person's name.
orchestrator-linear-assigned-to = assigned to { $assignee }
# A cycle without a name of its own.
orchestrator-linear-cycle-number = Cycle { $number }
# $cycle is a cycle's name, the one running now.
orchestrator-linear-cycle-current = { $cycle } (current)
# $label is a pull request's number, such as #12.
orchestrator-linear-open-pull-request = Open pull request { $label }
# How many pull requests an issue links, when more than one.
orchestrator-linear-pull-requests = { $count } PRs
orchestrator-linear-go-to-worktree = Go to worktree

# Pull requests grouped by who opened them.
orchestrator-linear-grouping-creator = Creator
orchestrator-linear-authors-everyone = Everyone
orchestrator-linear-unknown-author = Unknown author

# Connect Linear.
orchestrator-linear-connect = Connect Linear
orchestrator-linear-connect-hint = The Orchestrator lists the open Linear issues assigned to you. Create a personal API key in Linear under Settings, Security & access, and paste it here.
orchestrator-linear-open-settings = Open Linear settings
orchestrator-linear-variable-set = LINEAR_API_KEY is set, so it is used instead of a saved key.
orchestrator-linear-remove-key = Remove key
orchestrator-linear-save = Save
orchestrator-linear-paste-key = Paste a Linear personal API key.
orchestrator-linear-busy = The Orchestrator is busy. Try again in a moment.

# New issue.
orchestrator-linear-new-issue = New issue
orchestrator-linear-issue-title = Issue title
orchestrator-linear-add-description = Add description…
orchestrator-linear-try-again = Try again
orchestrator-linear-reading-teams = Reading your Linear teams…
# $issue is the new issue's identifier, such as ENG-12.
orchestrator-linear-created = Created { $issue }
orchestrator-linear-creating = Creating…
orchestrator-linear-create-issue = Create issue
orchestrator-linear-create-more = Create more
orchestrator-linear-wait-for-teams = Wait for your Linear teams to load.
orchestrator-linear-give-title = Give the issue a title.
orchestrator-linear-nothing-to-pick = Nothing to pick in this team
orchestrator-linear-team = Team
orchestrator-linear-status = Status
orchestrator-linear-estimate = Estimate
orchestrator-linear-labels = Labels
orchestrator-linear-project = Project
orchestrator-linear-no-project = No project
orchestrator-linear-no-estimate = No estimate
# An estimate in Linear's points.
orchestrator-linear-points = { $count ->
    [one] { $count } point
   *[other] { $count } points
}
orchestrator-linear-no-cycle = No cycle
orchestrator-linear-priority-urgent = Urgent
orchestrator-linear-priority-high = High
orchestrator-linear-priority-medium = Medium
orchestrator-linear-priority-low = Low
