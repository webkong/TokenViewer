import SwiftUI

/// Shown inline in rows; delegates to AgentTagClusterView in SkillListView.
/// Kept for backward compatibility / alternate usage.
struct SkillAgentBadgesView: View {
    let skill: SkillEntry
    let agents: [SkillAgent]

    var body: some View {
        AgentTagClusterView(skill: skill, agents: agents)
    }
}
