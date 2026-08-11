import Foundation

struct ModelProviderDefinition: Identifiable, Hashable {
    let id: String
    let displayName: String
    let logoFile: String
    let brandColor: String
}

/// Canonical model-vendor registry. Coding agents belong in `AgentRegistry`;
/// this registry is only for vendors inferred from model identifiers.
enum ModelProviderRegistry {
    private static let definitions: [String: ModelProviderDefinition] = [
        "openai": .init(id: "openai", displayName: "OpenAI", logoFile: "chatgpt", brandColor: "#3b82f6"),
        "anthropic": .init(id: "anthropic", displayName: "Anthropic", logoFile: "claude-code", brandColor: "#d97757"),
        "google": .init(id: "google", displayName: "Google", logoFile: "gemini", brandColor: "#2196f3"),
        "xai": .init(id: "xai", displayName: "xAI", logoFile: "grok", brandColor: "#73737f"),
        "deepseek": .init(id: "deepseek", displayName: "DeepSeek", logoFile: "deepseek", brandColor: "#4d6bfe"),
        "moonshot": .init(id: "moonshot", displayName: "Moonshot AI", logoFile: "kimi", brandColor: "#a38cfa"),
        "minimax": .init(id: "minimax", displayName: "MiniMax", logoFile: "minimax", brandColor: "#ff6b35"),
        "alibaba": .init(id: "alibaba", displayName: "Alibaba", logoFile: "qwen", brandColor: "#1e90ff"),
        "zhipu": .init(id: "zhipu", displayName: "Zhipu AI", logoFile: "glm", brandColor: "#4f5cf5"),
        "xiaomi": .init(id: "xiaomi", displayName: "Xiaomi", logoFile: "mimo", brandColor: "#ff6900"),
        "meituan": .init(id: "meituan", displayName: "Meituan", logoFile: "longcat", brandColor: "#ffd100"),
        "stepfun": .init(id: "stepfun", displayName: "StepFun", logoFile: "stepfun", brandColor: "#605bff"),
        "baidu": .init(id: "baidu", displayName: "Baidu", logoFile: "wenxin", brandColor: "#2932e1"),
        "tencent": .init(id: "tencent", displayName: "Tencent", logoFile: "hunyuan", brandColor: "#0052d9"),
        "hy3": .init(id: "hy3", displayName: "HY3", logoFile: "hy3", brandColor: "#111827"),
        "meta": .init(id: "meta", displayName: "Meta", logoFile: "llama", brandColor: "#0668e1"),
        "mistral": .init(id: "mistral", displayName: "Mistral AI", logoFile: "mistral", brandColor: "#ff7000"),
        "cohere": .init(id: "cohere", displayName: "Cohere", logoFile: "cohere", brandColor: "#39594d"),
        "01ai": .init(id: "01ai", displayName: "01.AI", logoFile: "yi", brandColor: "#111827"),
        "perplexity": .init(id: "perplexity", displayName: "Perplexity", logoFile: "perplexity", brandColor: "#20808d"),
    ]

    static var all: [ModelProviderDefinition] {
        definitions.values.sorted { $0.displayName < $1.displayName }
    }

    static func resolve(model: String) -> ModelProviderDefinition? {
        let value = model.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let id: String? = if value.hasPrefix("claude") || value == "anthropic" || value.hasPrefix("provider:anthropic") {
            "anthropic"
        } else if value.hasPrefix("gpt-") || value.hasPrefix("o1") || value.hasPrefix("o3") || value.hasPrefix("o4") || value.hasPrefix("o5") || value.hasPrefix("codex") || value == "openai" || value.hasPrefix("provider:openai") {
            "openai"
        } else if value.hasPrefix("gemini") || value.hasPrefix("provider:google") {
            "google"
        } else if value.hasPrefix("grok") || value.hasPrefix("provider:xai") {
            "xai"
        } else if value.contains("deepseek") {
            "deepseek"
        } else if value.hasPrefix("kimi") || value.hasPrefix("moonshot") || value.hasPrefix("provider:moonshot") {
            "moonshot"
        } else if value.hasPrefix("minimax") || value.hasPrefix("provider:minimax") {
            "minimax"
        } else if value.hasPrefix("qwen") || value.hasPrefix("provider:qwen") || value.hasPrefix("provider:alibaba") || value.hasPrefix("provider:dashscope") {
            "alibaba"
        } else if value.hasPrefix("glm") || value.hasPrefix("chatglm") || value.hasPrefix("provider:zhipu") {
            "zhipu"
        } else if value.hasPrefix("mimo") || value.hasPrefix("provider:xiaomi") {
            "xiaomi"
        } else if value.hasPrefix("longcat") {
            "meituan"
        } else if value.hasPrefix("step-") || value.hasPrefix("step1") || value.hasPrefix("step2") || value.hasPrefix("provider:stepfun") {
            "stepfun"
        } else if value.hasPrefix("ernie") || value.hasPrefix("wenxin") || value.hasPrefix("provider:baidu") {
            "baidu"
        } else if value.hasPrefix("hunyuan") || value.hasPrefix("provider:tencent") {
            "tencent"
        } else if value.hasPrefix("hy3") {
            "hy3"
        } else if value.hasPrefix("llama") || value.hasPrefix("provider:meta") {
            "meta"
        } else if value.hasPrefix("mistral") || value.hasPrefix("mixtral") || value.hasPrefix("codestral") || value.hasPrefix("provider:mistral") {
            "mistral"
        } else if value.hasPrefix("command") || value.hasPrefix("provider:cohere") {
            "cohere"
        } else if value.hasPrefix("yi-") || value.hasPrefix("provider:01ai") || value.hasPrefix("provider:zeroone") {
            "01ai"
        } else if value.hasPrefix("sonar") || value.hasPrefix("provider:perplexity") {
            "perplexity"
        } else {
            nil
        }
        return id.flatMap { definitions[$0] }
    }
}
