import SwiftUI

/// One-time onboarding sheet explaining the Skills feature's core mechanism:
/// where the shared library lives and how per-agent symlinks control visibility.
/// Shown automatically the first time the user opens Skill Manager, and
/// re-openable at any time via the header's help button.
struct SkillOnboardingSheet: View {
    @Environment(\.dismiss) private var dismiss
    private let l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Image(systemName: "puzzlepiece.extension.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(Color.accentColor)
                Text(l10n.skillOnboardingTitle)
                    .font(.system(size: 17, weight: .bold))
                Spacer()
            }

            VStack(alignment: .leading, spacing: 14) {
                OnboardingStep(
                    number: "1",
                    icon: "folder.fill",
                    title: l10n.skillOnboardingStep1Title,
                    description: l10n.skillOnboardingStep1Desc
                )
                OnboardingStep(
                    number: "2",
                    icon: "link",
                    title: l10n.skillOnboardingStep2Title,
                    description: l10n.skillOnboardingStep2Desc
                )
                OnboardingStep(
                    number: "3",
                    icon: "arrow.triangle.swap",
                    title: l10n.skillOnboardingStep3Title,
                    description: l10n.skillOnboardingStep3Desc
                )
            }

            HStack {
                Spacer()
                Button(l10n.skillOnboardingGotIt) {
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.regular)
            }
        }
        .padding(22)
        .frame(width: 440)
    }
}

private struct OnboardingStep: View {
    let number: String
    let icon: String
    let title: String
    let description: String

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            ZStack {
                Circle()
                    .fill(Color.accentColor.opacity(0.12))
                    .frame(width: 28, height: 28)
                Text(number)
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(Color.accentColor)
            }
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Image(systemName: icon)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    Text(title)
                        .font(.system(size: 13, weight: .semibold))
                }
                Text(description)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
