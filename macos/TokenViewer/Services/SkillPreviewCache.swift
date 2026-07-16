import Foundation

actor SkillPreviewCache {
    static let shared = SkillPreviewCache()

    static let maximumTextBytes = 2 * 1024 * 1024
    private static let maximumTreeDepth = 8
    private static let maximumNodesPerSkill = 1_000
    private static let maximumChildrenPerDirectory = 250

    private var previewTasks: [String: Task<PreparedSkillPreview, Never>] = [:]
    private var fileTasks: [String: Task<SkillFileLoadResult, Never>] = [:]

    func prewarm(_ skills: [SkillEntry]) {
        previewTasks.values.forEach { $0.cancel() }
        fileTasks.values.forEach { $0.cancel() }
        previewTasks.removeAll(keepingCapacity: true)
        fileTasks.removeAll(keepingCapacity: true)

        for skill in skills {
            let key = Self.cacheKey(for: skill)
            guard previewTasks[key] == nil else { continue }
            previewTasks[key] = Self.makePreviewTask(for: skill, priority: .utility)
        }
    }

    func preparedPreview(for preview: SkillMarkdownPreview) async -> PreparedSkillPreview {
        let key = Self.cacheKey(for: preview.skill)
        let task: Task<PreparedSkillPreview, Never>
        if let existing = previewTasks[key] {
            task = existing
        } else {
            let created = Self.makePreviewTask(for: preview.skill, priority: .userInitiated)
            previewTasks[key] = created
            task = created
        }
        let prepared = await task.value
        if fileTasks[prepared.primaryFilePath] == nil {
            let primaryContent = prepared.primaryContent
            fileTasks[prepared.primaryFilePath] = Task { primaryContent }
        }
        return prepared
    }

    func loadFile(at path: String) async -> SkillFileLoadResult {
        let normalizedPath = Self.standardizedPath(path)
        let task: Task<SkillFileLoadResult, Never>
        if let existing = fileTasks[normalizedPath] {
            task = existing
        } else {
            let created = Task.detached(priority: .userInitiated) {
                Self.readTextFile(at: normalizedPath)
            }
            fileTasks[normalizedPath] = created
            task = created
        }
        return await task.value
    }

    func invalidate() {
        previewTasks.values.forEach { $0.cancel() }
        fileTasks.values.forEach { $0.cancel() }
        previewTasks.removeAll()
        fileTasks.removeAll()
    }

    static func descriptor(for skill: SkillEntry) -> SkillMarkdownPreview {
        let skillDir = standardizedPath(skill.sourceDir)
        return SkillMarkdownPreview(
            skill: skill,
            filePath: URL(fileURLWithPath: skillDir).appendingPathComponent("SKILL.md").path
        )
    }

    private static func makePreviewTask(
        for skill: SkillEntry,
        priority: TaskPriority
    ) -> Task<PreparedSkillPreview, Never> {
        Task.detached(priority: priority) {
            if Task.isCancelled {
                return PreparedSkillPreview(
                    fileTree: nil,
                    primaryFilePath: Self.descriptor(for: skill).filePath,
                    primaryContent: .unreadable("Cancelled")
                )
            }

            let descriptor = Self.descriptor(for: skill)
            let rootPath = Self.standardizedPath(skill.sourceDir)
            let primaryPath = Self.primaryFilePath(
                in: rootPath,
                fallback: descriptor.filePath
            )
            var remainingNodes = Self.maximumNodesPerSkill
            let tree = Self.buildTree(
                url: URL(fileURLWithPath: rootPath),
                depth: 0,
                remainingNodes: &remainingNodes
            )
            let content = Self.readTextFile(at: primaryPath)
            return PreparedSkillPreview(
                fileTree: tree,
                primaryFilePath: primaryPath,
                primaryContent: content
            )
        }
    }

    private static func primaryFilePath(in rootPath: String, fallback: String) -> String {
        let fileManager = FileManager.default
        for name in ["SKILL.md", "skill.md"] {
            let path = URL(fileURLWithPath: rootPath).appendingPathComponent(name).path
            if fileManager.fileExists(atPath: path) {
                return path
            }
        }
        return fallback
    }

    private static func readTextFile(at path: String) -> SkillFileLoadResult {
        guard !Task.isCancelled else { return .unreadable("Cancelled") }
        let url = URL(fileURLWithPath: standardizedPath(path))
        guard FileManager.default.fileExists(atPath: url.path) else { return .missing }

        do {
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            let data = try handle.read(upToCount: maximumTextBytes + 1) ?? Data()
            guard !looksBinary(data) else { return .notText }

            let isTruncated = data.count > maximumTextBytes
            let visibleData = data.prefix(min(data.count, maximumTextBytes))
            let text: String
            if isTruncated {
                text = String(decoding: visibleData, as: UTF8.self)
            } else if let decoded = String(data: Data(visibleData), encoding: .utf8) {
                text = decoded
            } else {
                return .notText
            }
            return .loaded(SkillFileContent(text: text, isTruncated: isTruncated))
        } catch {
            return .unreadable(error.localizedDescription)
        }
    }

    private static func looksBinary(_ data: Data) -> Bool {
        data.prefix(8_192).contains(0)
    }

    private static func buildTree(
        url: URL,
        depth: Int,
        remainingNodes: inout Int
    ) -> SkillFileNode? {
        guard !Task.isCancelled, remainingNodes > 0 else { return nil }

        let keys: Set<URLResourceKey> = [.isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey]
        guard let values = try? url.resourceValues(forKeys: keys) else { return nil }
        remainingNodes -= 1

        let isDirectory = values.isDirectory == true && values.isSymbolicLink != true
        var children: [SkillFileNode] = []
        if isDirectory && depth < maximumTreeDepth && remainingNodes > 0 {
            let contents = (try? FileManager.default.contentsOfDirectory(
                at: url,
                includingPropertiesForKeys: Array(keys),
                options: [.skipsPackageDescendants]
            )) ?? []

            let sorted = contents
                .filter { !ignoredNames.contains($0.lastPathComponent) }
                .sorted { lhs, rhs in
                    let lhsIsDirectory = resourceIsDirectory(lhs)
                    let rhsIsDirectory = resourceIsDirectory(rhs)
                    if lhsIsDirectory != rhsIsDirectory {
                        return lhsIsDirectory && !rhsIsDirectory
                    }
                    return lhs.lastPathComponent.localizedStandardCompare(rhs.lastPathComponent)
                        == .orderedAscending
                }

            for childURL in sorted.prefix(maximumChildrenPerDirectory) {
                guard remainingNodes > 0, !Task.isCancelled else { break }
                if let child = buildTree(
                    url: childURL,
                    depth: depth + 1,
                    remainingNodes: &remainingNodes
                ) {
                    children.append(child)
                }
            }
        }

        return SkillFileNode(
            path: url.path,
            name: url.lastPathComponent.isEmpty ? url.path : url.lastPathComponent,
            isDirectory: isDirectory,
            sizeBytes: values.fileSize.map(Int64.init),
            children: children
        )
    }

    private static func resourceIsDirectory(_ url: URL) -> Bool {
        (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
    }

    private static func cacheKey(for skill: SkillEntry) -> String {
        "\(skill.id)|\(standardizedPath(skill.sourceDir))"
    }

    private static func standardizedPath(_ path: String) -> String {
        (NSString(string: path).expandingTildeInPath as NSString).standardizingPath
    }

    private static let ignoredNames: Set<String> = [
        ".DS_Store",
        ".git",
        ".idea",
        ".vscode",
        "__pycache__",
        "node_modules",
    ]
}
