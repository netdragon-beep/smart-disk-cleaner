use crate::models::{
    MigrationAction, MigrationActionKind, MigrationActionStep, MigrationAdvisorOutput,
    MigrationCategory, MigrationDocSource, MigrationDocSourceKind, MigrationObjectKind,
    MigrationOpportunity, MigrationPlan, MigrationSupportLevel, RiskLevel, ScanReport,
};
use crate::platform::{
    is_macos_system_sensitive_path, is_macos_user_space_path, is_windows_system_sensitive_path,
    is_windows_user_space_path, normalize_path,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const ONE_CLICK_LIMIT: usize = 8;
const ONE_CLICK_SIZE_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;
const DOWNLOAD_ARCHIVE_THRESHOLD_BYTES: u64 = 128 * 1024 * 1024;

pub fn build_migration_advice(
    report: &ScanReport,
    target_root: Option<&Path>,
) -> MigrationAdvisorOutput {
    let target_root = target_root
        .filter(|path| !path.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_target_root);

    let mut opportunities = Vec::new();
    opportunities.extend(build_one_click_file_opportunities(report, &target_root));

    if let Some(item) = build_wechat_data_opportunity(report, &target_root) {
        opportunities.push(item);
    }
    if let Some(item) = build_conda_package_cache_opportunity(report, &target_root) {
        opportunities.push(item);
    }
    if let Some(item) = build_conda_envs_opportunity(report, &target_root) {
        opportunities.push(item);
    }
    if let Some(item) = build_conda_installation_opportunity(report, &target_root) {
        opportunities.push(item);
    }

    opportunities.sort_by(|left, right| {
        support_rank(left.support_level)
            .cmp(&support_rank(right.support_level))
            .then_with(|| right.estimated_size.cmp(&left.estimated_size))
            .then_with(|| left.title.cmp(&right.title))
    });
    let plans = build_plans_from_opportunities(&opportunities);

    MigrationAdvisorOutput {
        source: "local_rules".to_string(),
        summary: build_summary(&opportunities),
        opportunities,
        plans,
    }
}

fn build_one_click_file_opportunities(
    report: &ScanReport,
    target_root: &Path,
) -> Vec<MigrationOpportunity> {
    let mut candidates = report
        .scanned_files
        .iter()
        .filter(|file| is_one_click_candidate(&file.path, file.size))
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .size
            .cmp(&left.size)
            .then_with(|| left.path.cmp(&right.path))
    });
    candidates.truncate(ONE_CLICK_LIMIT);

    candidates
        .into_iter()
        .map(|file| {
            let relative = relative_from_drive_root(&file.path).unwrap_or_else(|| {
                PathBuf::from(
                    file.path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("migrated-file"),
                )
            });
            let target_path = target_root.join("files").join(&relative);
            let target_dir = target_path
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| target_root.join("files"));

            let category = if is_download_archive_candidate(&file.path, file.size) {
                MigrationCategory::DownloadArchives
            } else {
                MigrationCategory::LargeFiles
            };
            let tags = if category == MigrationCategory::DownloadArchives {
                vec!["下载目录".to_string(), "安装包/压缩包".to_string()]
            } else {
                vec!["大文件".to_string(), "可一键迁移".to_string()]
            };
            let file_name = file
                .path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("未命名文件");

            MigrationOpportunity {
                id: format!("{}:{}", category_key(category), file.path.to_string_lossy()),
                title: format!("迁移 {file_name}"),
                category,
                support_level: MigrationSupportLevel::OneClick,
                risk: if is_download_archive_candidate(&file.path, file.size) {
                    RiskLevel::Low
                } else {
                    RiskLevel::Medium
                },
                estimated_size: file.size,
                source_path: file.path.clone(),
                recommended_target_dir: target_dir,
                recommended_target_path: target_path,
                reason: "这类大文件通常不依赖固定安装路径，迁移到其他盘后能立刻释放 C 盘空间。"
                    .to_string(),
                blocked_processes: Vec::new(),
                required_steps: vec![
                    step(
                        "确认内容用途",
                        "先确认该文件不是正在编辑或被某个程序持续占用的工作文件。",
                        true,
                    ),
                    step(
                        "执行迁移",
                        "迁移助手会保留文件名，并自动在目标目录创建缺失的文件夹。",
                        true,
                    ),
                    step(
                        "迁移后验证",
                        "确认目标盘文件可正常打开，再决定是否删除旧快捷方式或更新引用。",
                        true,
                    ),
                ],
                one_click_paths: vec![file.path.clone()],
                tags,
            }
        })
        .collect()
}

fn build_wechat_data_opportunity(
    report: &ScanReport,
    target_root: &Path,
) -> Option<MigrationOpportunity> {
    let root = aggregate_directory_by_marker(report, "wechat files")?;
    let target_path = target_root.join("app-data").join("WeChat Files");
    let target_dir = target_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| target_root.join("app-data"));

    Some(MigrationOpportunity {
        id: format!("wechat:{}", root.path.to_string_lossy()),
        title: "微信数据目录迁移建议".to_string(),
        category: MigrationCategory::WechatData,
        support_level: MigrationSupportLevel::Guided,
        risk: RiskLevel::Medium,
        estimated_size: root.total_size,
        source_path: root.path,
        recommended_target_dir: target_dir,
        recommended_target_path: target_path,
        reason: "检测到微信数据目录。它通常体积大、释放空间明显，但迁移前必须关闭微信并在迁移后验证聊天记录和文件索引。".to_string(),
        blocked_processes: vec!["WeChat.exe".to_string(), "Weixin.exe".to_string()],
        required_steps: vec![
            step("关闭微信进程", "迁移前需要退出微信客户端，避免文件被占用。", true),
            step("移动数据目录", "建议将整个 WeChat Files 目录迁到其他盘的专用数据目录。", true),
            step("更新微信存储位置", "如果新版微信提供存储路径设置，应指向新位置；否则需要高级兼容方案，如目录联接。", true),
            step("重启并验证", "重新启动微信，确认聊天记录、图片和文件缓存都能正常访问。", true),
        ],
        one_click_paths: Vec::new(),
        tags: vec!["应用数据".to_string(), "需关闭进程".to_string()],
    })
}

fn build_conda_package_cache_opportunity(
    report: &ScanReport,
    target_root: &Path,
) -> Option<MigrationOpportunity> {
    let root = aggregate_conda_directory(report, "pkgs")?;
    let target_path = target_root.join("conda").join("pkgs");
    let target_dir = target_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| target_root.join("conda"));

    Some(MigrationOpportunity {
        id: format!("conda-pkgs:{}", root.path.to_string_lossy()),
        title: "Conda 包缓存迁移建议".to_string(),
        category: MigrationCategory::CondaPackageCache,
        support_level: MigrationSupportLevel::Guided,
        risk: RiskLevel::Medium,
        estimated_size: root.total_size,
        source_path: root.path,
        recommended_target_dir: target_dir,
        recommended_target_path: target_path,
        reason: "检测到 Conda 的 pkgs 缓存目录。它通常很大，但更稳妥的做法是先修改 Conda 的缓存目录配置，再迁移或重新下载缓存。".to_string(),
        blocked_processes: vec!["python.exe".to_string(), "conda.exe".to_string()],
        required_steps: vec![
            step("停止 Conda 相关任务", "关闭正在运行的 Python、Jupyter、终端和包管理任务。", true),
            step("调整缓存目录配置", "优先通过 .condarc 的 pkgs_dirs 或对应环境变量，把缓存目录指向目标盘。", true),
            step("迁移或重建缓存", "配置生效后再迁移已有缓存，或清理后让 Conda 在新路径重新生成。", true),
            step("执行 conda info 验证", "确认新的包缓存目录已经被 Conda 正常识别。", true),
        ],
        one_click_paths: Vec::new(),
        tags: vec!["开发环境".to_string(), "需改配置".to_string()],
    })
}

fn build_conda_envs_opportunity(
    report: &ScanReport,
    target_root: &Path,
) -> Option<MigrationOpportunity> {
    let root = aggregate_conda_directory(report, "envs")?;
    let target_path = target_root.join("conda").join("envs");
    let target_dir = target_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| target_root.join("conda"));

    Some(MigrationOpportunity {
        id: format!("conda-envs:{}", root.path.to_string_lossy()),
        title: "Conda 环境目录迁移建议".to_string(),
        category: MigrationCategory::CondaEnvironments,
        support_level: MigrationSupportLevel::Guided,
        risk: RiskLevel::High,
        estimated_size: root.total_size,
        source_path: root.path,
        recommended_target_dir: target_dir,
        recommended_target_path: target_path,
        reason: "检测到 Conda envs 目录。环境内部常包含绝对路径和脚本引用，直接硬搬目录容易导致环境损坏，更适合做配置切换或重建式迁移。".to_string(),
        blocked_processes: vec!["python.exe".to_string(), "conda.exe".to_string()],
        required_steps: vec![
            step("盘点关键环境", "先确认哪些环境仍在使用，并记录 Python 版本和核心依赖。", true),
            step("导出环境定义", "优先使用 conda env export 等方式导出环境描述，避免直接硬迁。", true),
            step("设置新环境目录", "通过 .condarc 的 envs_dirs 或环境变量 CONDA_ENVS_PATH 指向目标盘。", true),
            step("在目标盘重建环境", "按照导出的定义重新创建关键环境，成功后再清理旧环境。", true),
        ],
        one_click_paths: Vec::new(),
        tags: vec!["开发环境".to_string(), "高风险".to_string()],
    })
}

fn build_conda_installation_opportunity(
    report: &ScanReport,
    target_root: &Path,
) -> Option<MigrationOpportunity> {
    let root = aggregate_conda_install_root(report)?;
    let target_path = target_root.join("conda").join(
        root.path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Miniconda3"),
    );
    let target_dir = target_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| target_root.join("conda"));

    Some(MigrationOpportunity {
        id: format!("conda-root:{}", root.path.to_string_lossy()),
        title: "Conda 安装目录迁移建议".to_string(),
        category: MigrationCategory::CondaInstallation,
        support_level: MigrationSupportLevel::Manual,
        risk: RiskLevel::High,
        estimated_size: root.total_size,
        source_path: root.path,
        recommended_target_dir: target_dir,
        recommended_target_path: target_path,
        reason: "检测到 Miniconda/Anaconda 安装目录。基础安装目录与 PATH、快捷方式和脚本强相关，不建议直接一键移动。".to_string(),
        blocked_processes: vec!["python.exe".to_string(), "conda.exe".to_string()],
        required_steps: vec![
            step("避免直接硬迁 base", "不要直接整体移动 Miniconda/Anaconda 根目录。", true),
            step("重新安装到目标盘", "更稳妥的方式是在目标盘重新安装 Conda。", true),
            step("迁移环境和缓存策略", "安装完成后再配置 envs_dirs、pkgs_dirs，并逐步迁移环境。", true),
            step("更新 PATH 与工具入口", "确认 PATH、终端启动脚本和 IDE Python 解释器都已切换到新安装路径。", true),
        ],
        one_click_paths: Vec::new(),
        tags: vec!["开发环境".to_string(), "需重装".to_string()],
    })
}

fn build_summary(opportunities: &[MigrationOpportunity]) -> String {
    if opportunities.is_empty() {
        return "当前扫描结果中没有检测到适合迁移助手处理的高价值对象。建议优先扫描 C 盘用户目录、下载目录、微信数据目录或 Conda 安装目录。".to_string();
    }

    let one_click = opportunities
        .iter()
        .filter(|item| item.support_level == MigrationSupportLevel::OneClick)
        .count();
    let guided = opportunities
        .iter()
        .filter(|item| item.support_level == MigrationSupportLevel::Guided)
        .count();
    let manual = opportunities
        .iter()
        .filter(|item| item.support_level == MigrationSupportLevel::Manual)
        .count();
    let total_size = opportunities
        .iter()
        .map(|item| item.estimated_size)
        .sum::<u64>();

    format!(
        "已识别 {} 个迁移机会，预计涉及 {}。其中可一键迁移 {} 项，引导式迁移 {} 项，人工处理 {} 项。建议先处理下载目录中的大文件，再处理微信数据和 Conda 这类需要配置变更的目录。",
        opportunities.len(),
        format_bytes(total_size),
        one_click,
        guided,
        manual
    )
}

fn support_rank(level: MigrationSupportLevel) -> u8 {
    match level {
        MigrationSupportLevel::OneClick => 0,
        MigrationSupportLevel::Guided => 1,
        MigrationSupportLevel::Manual => 2,
    }
}

fn is_one_click_candidate(path: &Path, size: u64) -> bool {
    if size < ONE_CLICK_SIZE_THRESHOLD_BYTES && !is_download_archive_candidate(path, size) {
        return false;
    }
    if !is_user_space_path(path) || is_system_sensitive_path(path) || is_app_specific_path(path) {
        return false;
    }

    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        let ext = extension.to_ascii_lowercase();
        return matches!(
            ext.as_str(),
            "zip"
                | "7z"
                | "rar"
                | "iso"
                | "msi"
                | "exe"
                | "mp4"
                | "mkv"
                | "mov"
                | "avi"
                | "wmv"
                | "mp3"
                | "flac"
                | "wav"
                | "jpg"
                | "jpeg"
                | "png"
                | "psd"
                | "pdf"
                | "ppt"
                | "pptx"
                | "doc"
                | "docx"
                | "blend"
                | "obj"
        );
    }

    false
}

fn is_download_archive_candidate(path: &Path, size: u64) -> bool {
    if size < DOWNLOAD_ARCHIVE_THRESHOLD_BYTES {
        return false;
    }
    let path_text = normalize_path(path);
    (path_text.contains("/downloads/") || path_text.contains("/desktop/"))
        && matches!(
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .as_deref(),
            Some("zip" | "7z" | "rar" | "iso" | "msi" | "exe")
        )
}

fn is_user_space_path(path: &Path) -> bool {
    is_windows_user_space_path(path) || is_macos_user_space_path(path)
}

fn is_system_sensitive_path(path: &Path) -> bool {
    is_windows_system_sensitive_path(path) || is_macos_system_sensitive_path(path)
}

fn is_app_specific_path(path: &Path) -> bool {
    let text = normalize_path(path);
    text.contains("/wechat files/")
        || text.contains("/library/application support/tencent/xinwechat/")
        || text.contains("/library/containers/com.tencent.xinwechat/")
        || text.contains("/anaconda3/")
        || text.contains("/miniconda3/")
        || text.contains("/miniforge3/")
        || text.contains("/mambaforge/")
        || text.contains("/.conda/")
}

fn relative_from_drive_root(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    match components.next()? {
        Component::Prefix(_) => {}
        _ => return None,
    }
    match components.next()? {
        Component::RootDir => {}
        _ => return None,
    }
    Some(components.as_path().to_path_buf())
}

fn default_target_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/Users/Shared"))
            .join("SmartDiskCleanerMigration")
    } else {
        PathBuf::from(r"D:\SmartDiskCleanerMigration")
    }
}

fn step(title: &str, detail: &str, required: bool) -> MigrationActionStep {
    MigrationActionStep {
        title: title.to_string(),
        detail: detail.to_string(),
        required,
    }
}

fn category_key(category: MigrationCategory) -> &'static str {
    match category {
        MigrationCategory::LargeFiles => "large",
        MigrationCategory::DownloadArchives => "download_archive",
        MigrationCategory::WechatData => "wechat",
        MigrationCategory::CondaPackageCache => "conda_pkgs",
        MigrationCategory::CondaEnvironments => "conda_envs",
        MigrationCategory::CondaInstallation => "conda_install",
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let bytes = bytes as f64;
    if bytes < MB {
        return format!("{:.1} KB", bytes / KB);
    }
    if bytes < GB {
        return format!("{:.1} MB", bytes / MB);
    }
    format!("{:.2} GB", bytes / GB)
}

fn build_plans_from_opportunities(opportunities: &[MigrationOpportunity]) -> Vec<MigrationPlan> {
    opportunities
        .iter()
        .map(build_plan_from_opportunity)
        .collect()
}

fn build_plan_from_opportunity(opportunity: &MigrationOpportunity) -> MigrationPlan {
    let object_kind = match opportunity.category {
        MigrationCategory::LargeFiles | MigrationCategory::DownloadArchives => {
            MigrationObjectKind::File
        }
        MigrationCategory::WechatData => MigrationObjectKind::AppData,
        MigrationCategory::CondaPackageCache => MigrationObjectKind::PackageCache,
        MigrationCategory::CondaEnvironments => MigrationObjectKind::EnvironmentStore,
        MigrationCategory::CondaInstallation => MigrationObjectKind::InstallationRoot,
    };

    let mut actions = Vec::new();
    let plan_id = format!("plan:{}", opportunity.id);

    for (index, process_name) in opportunity.blocked_processes.iter().enumerate() {
        actions.push(MigrationAction {
            id: format!("{plan_id}:stop:{index}"),
            kind: MigrationActionKind::StopProcess,
            title: format!("关闭进程 {process_name}"),
            detail: "在迁移前关闭相关进程，避免文件占用导致迁移失败。".to_string(),
            required: true,
            enabled_by_default: true,
            params: json!({
                "processName": process_name,
            }),
        });
    }

    if let Some((key, value)) = conda_config_entry_for_plan(opportunity) {
        actions.push(MigrationAction {
            id: format!("{plan_id}:condarc"),
            kind: MigrationActionKind::UpdateYamlList,
            title: "更新 .condarc".to_string(),
            detail: "把 Conda 配置中的目录列表更新到新的目标路径。".to_string(),
            required: opportunity.category == MigrationCategory::CondaPackageCache,
            enabled_by_default: true,
            params: json!({
                "path": "~/.condarc",
                "key": key,
                "value": value,
            }),
        });

        let env_name = if opportunity.category == MigrationCategory::CondaPackageCache {
            "CONDA_PKGS_DIRS"
        } else {
            "CONDA_ENVS_PATH"
        };

        actions.push(MigrationAction {
            id: format!("{plan_id}:env"),
            kind: MigrationActionKind::SetEnvVar,
            title: format!("更新环境变量 {env_name}"),
            detail: "同步修改用户环境变量，让命令行和图形界面都能识别新路径。".to_string(),
            required: false,
            enabled_by_default: false,
            params: json!({
                "name": env_name,
                "value": value,
            }),
        });
    }

    actions.push(MigrationAction {
        id: format!("{plan_id}:move"),
        kind: MigrationActionKind::MovePath,
        title: "移动路径".to_string(),
        detail: "把识别出的源路径迁移到推荐目标路径。".to_string(),
        required: true,
        enabled_by_default: true,
        params: json!({
            "sourcePath": opportunity.source_path,
            "targetDir": opportunity.recommended_target_dir,
            "targetPath": opportunity.recommended_target_path,
            "allowSpecialPaths": opportunity.category != MigrationCategory::LargeFiles
                && opportunity.category != MigrationCategory::DownloadArchives,
        }),
    });

    if supports_compat_link(opportunity.category) {
        actions.push(MigrationAction {
            id: format!("{plan_id}:junction"),
            kind: MigrationActionKind::CreateJunction,
            title: "创建兼容 Junction".to_string(),
            detail: "在原路径创建目录联接，减少依赖固定路径的软件出错概率。".to_string(),
            required: false,
            enabled_by_default: true,
            params: json!({
                "sourcePath": opportunity.source_path,
                "targetPath": opportunity.recommended_target_path,
            }),
        });
    }

    actions.push(MigrationAction {
        id: format!("{plan_id}:verify"),
        kind: MigrationActionKind::VerifyPathExists,
        title: "验证迁移结果".to_string(),
        detail: "确认目标路径存在，基础迁移步骤完成。".to_string(),
        required: true,
        enabled_by_default: true,
        params: json!({
            "path": opportunity.recommended_target_path,
        }),
    });

    MigrationPlan {
        id: plan_id,
        title: opportunity.title.clone(),
        category: opportunity.category,
        object_kind,
        support_level: opportunity.support_level,
        risk: opportunity.risk,
        estimated_size: opportunity.estimated_size,
        source_path: opportunity.source_path.clone(),
        recommended_target_dir: opportunity.recommended_target_dir.clone(),
        recommended_target_path: opportunity.recommended_target_path.clone(),
        summary: format!(
            "{}，预计处理 {}。",
            category_summary(opportunity.category),
            format_bytes(opportunity.estimated_size)
        ),
        rationale: opportunity.reason.clone(),
        tags: opportunity.tags.clone(),
        doc_sources: doc_sources_for_plan(opportunity.category),
        actions,
        verification_steps: opportunity.required_steps.clone(),
    }
}

fn conda_config_entry_for_plan(
    opportunity: &MigrationOpportunity,
) -> Option<(&'static str, String)> {
    match opportunity.category {
        MigrationCategory::CondaPackageCache => Some((
            "pkgs_dirs",
            opportunity
                .recommended_target_path
                .to_string_lossy()
                .to_string(),
        )),
        MigrationCategory::CondaEnvironments => Some((
            "envs_dirs",
            opportunity
                .recommended_target_path
                .to_string_lossy()
                .to_string(),
        )),
        _ => None,
    }
}

fn supports_compat_link(category: MigrationCategory) -> bool {
    matches!(
        category,
        MigrationCategory::WechatData
            | MigrationCategory::CondaPackageCache
            | MigrationCategory::CondaEnvironments
    )
}

fn category_summary(category: MigrationCategory) -> &'static str {
    match category {
        MigrationCategory::LargeFiles => "这是一个普通大文件迁移计划",
        MigrationCategory::DownloadArchives => "这是一个下载目录归档迁移计划",
        MigrationCategory::WechatData => "这是一个应用数据目录迁移计划",
        MigrationCategory::CondaPackageCache => "这是一个开发环境缓存迁移计划",
        MigrationCategory::CondaEnvironments => "这是一个开发环境目录迁移计划",
        MigrationCategory::CondaInstallation => "这是一个安装根目录迁移计划",
    }
}

fn doc_sources_for_plan(category: MigrationCategory) -> Vec<MigrationDocSource> {
    let mut values = vec![MigrationDocSource {
        title: "本地扫描证据".to_string(),
        kind: MigrationDocSourceKind::LocalObservation,
        uri: None,
        note: "根据当前扫描结果识别源路径、占用体积和目录结构。".to_string(),
    }];

    values.push(MigrationDocSource {
        title: "内置迁移规则".to_string(),
        kind: MigrationDocSourceKind::LocalRule,
        uri: None,
        note: "当前计划由本地规则引擎生成，后续可替换为 LLM + 文档检索规划。".to_string(),
    });

    if matches!(
        category,
        MigrationCategory::CondaPackageCache | MigrationCategory::CondaEnvironments
    ) {
        values.push(MigrationDocSource {
            title: "Conda 自定义目录配置".to_string(),
            kind: MigrationDocSourceKind::OfficialDoc,
            uri: Some(
                "https://docs.conda.io/projects/conda/en/stable/user-guide/configuration/custom-env-and-pkg-locations.html"
                    .to_string(),
            ),
            note: "作为 Conda envs_dirs / pkgs_dirs 迁移策略的官方依据。".to_string(),
        });
    }

    if category == MigrationCategory::CondaInstallation {
        values.push(MigrationDocSource {
            title: "Conda 配置说明".to_string(),
            kind: MigrationDocSourceKind::OfficialDoc,
            uri: Some(
                "https://docs.conda.io/projects/conda/en/latest/user-guide/configuration/settings.html"
                    .to_string(),
            ),
            note: "提示安装根目录更适合重装和重建，而不是直接硬搬。".to_string(),
        });
    }

    values
}

#[derive(Debug)]
struct DirectoryAggregate {
    path: PathBuf,
    total_size: u64,
}

fn aggregate_directory_by_marker(report: &ScanReport, marker: &str) -> Option<DirectoryAggregate> {
    let mut aggregates = BTreeMap::<PathBuf, u64>::new();
    let marker = marker.to_ascii_lowercase();

    for file in &report.scanned_files {
        if let Some(root) = ancestor_up_to_component(&file.path, &marker) {
            *aggregates.entry(root).or_default() += file.size;
        }
    }

    aggregates
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(path, total_size)| DirectoryAggregate { path, total_size })
}

fn aggregate_conda_directory(
    report: &ScanReport,
    leaf_component: &str,
) -> Option<DirectoryAggregate> {
    let mut aggregates = BTreeMap::<PathBuf, u64>::new();
    for file in &report.scanned_files {
        if let Some(root) = conda_directory_root(&file.path, leaf_component) {
            *aggregates.entry(root).or_default() += file.size;
        }
    }

    aggregates
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(path, total_size)| DirectoryAggregate { path, total_size })
}

fn aggregate_conda_install_root(report: &ScanReport) -> Option<DirectoryAggregate> {
    let mut aggregates = BTreeMap::<PathBuf, u64>::new();
    for file in &report.scanned_files {
        if let Some(root) = conda_install_root(&file.path) {
            *aggregates.entry(root).or_default() += file.size;
        }
    }

    aggregates
        .into_iter()
        .filter(|(_, total_size)| *total_size >= 512 * 1024 * 1024)
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
        .map(|(path, total_size)| DirectoryAggregate { path, total_size })
}

fn ancestor_up_to_component(path: &Path, marker: &str) -> Option<PathBuf> {
    let mut built = PathBuf::new();
    for component in path.components() {
        built.push(component.as_os_str());
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if value == marker {
            return Some(built);
        }
    }
    None
}

fn conda_directory_root(path: &Path, leaf_component: &str) -> Option<PathBuf> {
    let mut built = PathBuf::new();
    let mut saw_install_root = false;

    for component in path.components() {
        built.push(component.as_os_str());
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if matches!(
            value.as_str(),
            "anaconda3" | "miniconda3" | "miniforge3" | "mambaforge"
        ) {
            saw_install_root = true;
        }
        if saw_install_root && value == leaf_component {
            return Some(built);
        }
    }
    None
}

fn conda_install_root(path: &Path) -> Option<PathBuf> {
    let mut built = PathBuf::new();
    for component in path.components() {
        built.push(component.as_os_str());
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if matches!(
            value.as_str(),
            "anaconda3" | "miniconda3" | "miniforge3" | "mambaforge"
        ) {
            return Some(built);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::build_migration_advice;
    use crate::models::{
        AdvisorOutput, AnalysisResult, DedupResult, FileRecord, PathIssue, ScanReport,
    };
    use chrono::Utc;
    use std::path::PathBuf;

    fn file(path: &str, size: u64) -> FileRecord {
        FileRecord {
            path: PathBuf::from(path),
            size,
            extension: PathBuf::from(path)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase()),
            modified_at: None,
            is_empty: false,
        }
    }

    fn empty_report(files: Vec<FileRecord>) -> ScanReport {
        ScanReport {
            generated_at: Utc::now(),
            scan_duration_ms: 0,
            root: PathBuf::from(r"C:\Users\demo"),
            scanned_files: files,
            analysis: AnalysisResult {
                total_files: 0,
                total_size: 0,
                empty_files: Vec::new(),
                empty_dirs: Vec::new(),
                large_files: Vec::new(),
                temporary_files: Vec::new(),
                archive_files: Vec::new(),
                type_breakdown: Vec::new(),
            },
            dedup: DedupResult {
                groups: Vec::new(),
                failures: Vec::<PathIssue>::new(),
            },
            modules: Vec::new(),
            advisor: AdvisorOutput {
                source: "local_rules".to_string(),
                summary: String::new(),
                suggestions: Vec::new(),
            },
            failures: Vec::new(),
        }
    }

    #[test]
    fn surfaces_wechat_and_conda_opportunities() {
        let report = empty_report(vec![
            file(
                r"C:\Users\demo\Documents\WeChat Files\wxid_1\FileStorage\Video\clip.mp4",
                1024,
            ),
            file(
                r"C:\Users\demo\Miniconda3\pkgs\python-3.11.egg-info\record.json",
                2048,
            ),
            file(r"C:\Users\demo\Miniconda3\envs\data\python.exe", 4096),
        ]);

        let advice = build_migration_advice(&report, Some(PathBuf::from(r"E:\Archive").as_path()));
        assert!(advice
            .opportunities
            .iter()
            .any(|item| item.title.contains("微信数据目录")));
        assert!(advice
            .opportunities
            .iter()
            .any(|item| item.title.contains("Conda 包缓存")));
        assert!(advice
            .opportunities
            .iter()
            .any(|item| item.title.contains("Conda 环境目录")));
    }

    #[test]
    fn keeps_system_files_out_of_one_click_candidates() {
        let report = empty_report(vec![
            file(r"C:\Windows\Temp\huge.iso", 800 * 1024 * 1024),
            file(r"C:\Users\demo\Downloads\movie.mkv", 900 * 1024 * 1024),
        ]);

        let advice = build_migration_advice(&report, None);
        assert_eq!(
            advice
                .opportunities
                .iter()
                .filter(|item| !item.one_click_paths.is_empty())
                .count(),
            1
        );
        assert!(advice
            .opportunities
            .iter()
            .all(|item| item.source_path != PathBuf::from(r"C:\Windows\Temp\huge.iso")));
    }
}
