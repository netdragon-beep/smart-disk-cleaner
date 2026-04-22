use crate::ai_advisor::AdvisorConfig;
use crate::models::{
    ProcessAiFollowUpAnswer, ProcessAiFollowUpTurn, ProcessAiInsight, ProcessRecord,
    ProcessSuggestedAction, RiskLevel,
};
use anyhow::{anyhow, Result};
use humansize::{format_size, BINARY};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::path::Path;
use tracing::warn;

pub async fn explain_process(
    process: &ProcessRecord,
    config: &AdvisorConfig,
) -> Result<ProcessAiInsight> {
    let local_insight = build_local_process_insight(process);

    let Some(api_key) = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(local_insight);
    };

    match request_remote_process_insight(process, &local_insight, config, api_key).await {
        Ok(insight) => Ok(insight),
        Err(error) => {
            warn!("AI process insight failed, falling back to local rules: {error}");
            let mut fallback = local_insight;
            fallback.remote_attempted = true;
            fallback.used_fallback = true;
            fallback.fallback_reason = Some(error.to_string());
            Ok(fallback)
        }
    }
}

pub async fn answer_process_follow_up(
    process: &ProcessRecord,
    insight: &ProcessAiInsight,
    question: &str,
    history: &[ProcessAiFollowUpTurn],
    config: &AdvisorConfig,
) -> Result<ProcessAiFollowUpAnswer> {
    let question = question.trim();
    if question.is_empty() {
        return Err(anyhow!("追问内容不能为空"));
    }

    let local_answer = build_local_follow_up_answer(process, insight, question, history);

    let Some(api_key) = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(local_answer);
    };

    match request_remote_process_follow_up(process, insight, question, history, config, api_key)
        .await
    {
        Ok(answer) => Ok(answer),
        Err(error) => {
            warn!("AI process follow-up failed, falling back to local rules: {error}");
            let mut fallback = local_answer;
            fallback.remote_attempted = true;
            fallback.used_fallback = true;
            fallback.fallback_reason = Some(error.to_string());
            Ok(fallback)
        }
    }
}

pub fn classify_process_metadata(
    name: &str,
    exe_path: Option<&Path>,
    command: &[String],
    pid: u32,
) -> (String, bool) {
    let normalized_name = name.trim().to_ascii_lowercase();
    let exe_text = exe_path
        .map(|path| path.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let command_text = command.join(" ").to_ascii_lowercase();
    let in_windows_dir = exe_text.starts_with("c:\\windows\\");
    let in_system_dir = exe_text.starts_with("c:\\windows\\system32")
        || exe_text.starts_with("c:\\windows\\syswow64")
        || exe_text.starts_with("c:\\windows\\winsxs");

    let critical_names = [
        "system",
        "registry",
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "winlogon.exe",
        "svchost.exe",
        "fontdrvhost.exe",
        "memory compression",
        "secure system",
    ];

    if pid <= 4 || critical_names.contains(&normalized_name.as_str()) {
        return ("system_critical".to_string(), true);
    }

    let security_names = [
        "msmpeng.exe",
        "securityhealthservice.exe",
        "avp.exe",
        "avgsvc.exe",
        "360tray.exe",
        "360safe.exe",
    ];
    if security_names.contains(&normalized_name.as_str()) {
        return ("security".to_string(), false);
    }

    let browser_names = [
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "brave.exe",
        "opera.exe",
        "qqbrowser.exe",
        "360se.exe",
    ];
    if browser_names.contains(&normalized_name.as_str()) {
        return ("browser".to_string(), false);
    }

    let development_names = [
        "code.exe",
        "cursor.exe",
        "windsurf.exe",
        "devenv.exe",
        "idea64.exe",
        "pycharm64.exe",
        "webstorm64.exe",
        "clion64.exe",
        "rider64.exe",
        "goland64.exe",
        "androidstudio64.exe",
        "rustrover64.exe",
        "powershell.exe",
        "pwsh.exe",
        "cmd.exe",
        "node.exe",
        "npm.exe",
        "pnpm.exe",
        "yarn.exe",
        "cargo.exe",
        "rustc.exe",
        "python.exe",
        "java.exe",
    ];
    if development_names.contains(&normalized_name.as_str()) {
        return ("development".to_string(), false);
    }

    let virtualization_names = [
        "docker desktop.exe",
        "com.docker.backend.exe",
        "vmmem",
        "vmmemwsl",
        "vmware.exe",
        "vmware-vmx.exe",
        "virtualboxvm.exe",
        "qemu-system-x86_64.exe",
    ];
    if virtualization_names.contains(&normalized_name.as_str())
        || command_text.contains("docker")
        || command_text.contains("wsl")
    {
        return ("virtualization".to_string(), false);
    }

    let sync_names = [
        "onedrive.exe",
        "dropbox.exe",
        "baidunetdisk.exe",
        "baidunetdiskhost.exe",
    ];
    if sync_names.contains(&normalized_name.as_str()) {
        return ("sync".to_string(), false);
    }

    let helper_keywords = [
        "helper",
        "crashpad",
        "updater",
        "update",
        "installer",
        "setup",
    ];
    if helper_keywords
        .iter()
        .any(|keyword| normalized_name.contains(keyword) || command_text.contains(keyword))
    {
        return ("background_helper".to_string(), false);
    }

    if in_system_dir {
        return ("system_service".to_string(), false);
    }

    if in_windows_dir {
        return ("windows_component".to_string(), false);
    }

    ("user_app".to_string(), false)
}

pub fn compute_resource_score(
    cpu_usage: f32,
    memory_bytes: u64,
    disk_read_bytes: u64,
    disk_written_bytes: u64,
) -> f32 {
    let memory_mb = memory_bytes as f32 / (1024.0 * 1024.0);
    let disk_mb = (disk_read_bytes + disk_written_bytes) as f32 / (1024.0 * 1024.0);
    cpu_usage * 1.4 + memory_mb * 0.06 + disk_mb * 2.0
}

fn build_local_process_insight(process: &ProcessRecord) -> ProcessAiInsight {
    let (risk, suggested_action) = local_risk_and_action(process);
    let summary = local_summary(process, risk, suggested_action);
    let reason = local_reason(process, risk, suggested_action);

    ProcessAiInsight {
        pid: process.pid,
        name: process.name.clone(),
        source: "local_rules".to_string(),
        summary,
        suggested_action,
        risk,
        reason,
        remote_attempted: false,
        used_fallback: false,
        fallback_reason: None,
    }
}

fn build_local_follow_up_answer(
    process: &ProcessRecord,
    insight: &ProcessAiInsight,
    question: &str,
    history: &[ProcessAiFollowUpTurn],
) -> ProcessAiFollowUpAnswer {
    let normalized = question.to_ascii_lowercase();
    let answer = if contains_any(
        &normalized,
        &["结束", "关闭", "kill", "terminate", "删", "删除"],
    ) {
        format!(
            "{} 当前建议是“{}”，风险等级为“{}”。{}",
            process.name,
            process_action_label(insight.suggested_action),
            risk_label(insight.risk),
            match insight.suggested_action {
                ProcessSuggestedAction::AvoidEnd => {
                    "从当前分类和路径看，它更接近系统关键、安全防护或重要后台进程，不建议直接结束。"
                }
                ProcessSuggestedAction::EndAfterSave => {
                    "它更像用户正在使用或可能正在写入数据的应用，建议先保存工作，再考虑结束。"
                }
                ProcessSuggestedAction::Review => {
                    "它不一定不能结束，但最好先确认是否承担同步、更新、容器或后台服务职责。"
                }
                ProcessSuggestedAction::SafeToEnd => {
                    "从当前元数据看更像辅助进程或短时后台任务，确认无依赖后通常可以尝试结束。"
                }
            }
        )
    } else if contains_any(&normalized, &["做什么", "用途", "是什么", "干嘛"]) {
        let exe_path = process
            .exe_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "未知路径".to_string());
        let command = if process.command.is_empty() {
            "没有采集到命令行参数".to_string()
        } else {
            process.command.join(" ")
        };

        format!(
            "{} 大概率属于“{}”。可执行路径：{}。命令行：{}。{}",
            process.name,
            category_label(&process.category),
            exe_path,
            command,
            insight.summary
        )
    } else if contains_any(
        &normalized,
        &["卡", "占用", "cpu", "内存", "磁盘", "慢", "高", "资源"],
    ) {
        format!(
            "这次采样里它的 CPU 为 {:.1}%，内存约 {}，磁盘读写约 {}/s，综合压力分 {:.1}。{}",
            process.cpu_usage,
            format_size(process.memory_bytes, BINARY),
            format_size(process.disk_read_bytes + process.disk_written_bytes, BINARY),
            process.resource_score,
            resource_pressure_text(process)
        )
    } else if contains_any(&normalized, &["不处理", "放着", "影响", "后果"]) {
        format!(
            "{}。如果暂时不处理，通常意味着它会继续保持当前的资源占用模式；如果这正是导致卡顿的元凶，卡顿现象可能持续或再次出现。",
            insight.reason
        )
    } else {
        format!(
            "基于当前进程名、路径、命令行和资源占用，我的保守判断是：{}。如果你想更具体一些，可以继续问“结束它会有什么影响？”、“它为什么占用这么高？”或“这个进程主要是做什么的？”。",
            insight.summary
        )
    };

    let answer = format!("{answer}{}", build_history_suffix(history));

    ProcessAiFollowUpAnswer {
        pid: process.pid,
        name: process.name.clone(),
        question: question.to_string(),
        answer,
        source: "local_rules".to_string(),
        remote_attempted: false,
        used_fallback: false,
        fallback_reason: None,
    }
}

fn local_risk_and_action(process: &ProcessRecord) -> (RiskLevel, ProcessSuggestedAction) {
    if process.is_critical || process.category == "system_critical" {
        return (RiskLevel::High, ProcessSuggestedAction::AvoidEnd);
    }

    match process.category.as_str() {
        "security" => (RiskLevel::High, ProcessSuggestedAction::AvoidEnd),
        "system_service" | "windows_component" => {
            (RiskLevel::Medium, ProcessSuggestedAction::Review)
        }
        "browser" | "development" | "virtualization" | "user_app" => {
            (RiskLevel::Medium, ProcessSuggestedAction::EndAfterSave)
        }
        "sync" => (RiskLevel::Medium, ProcessSuggestedAction::Review),
        "background_helper" => {
            if process.cpu_usage >= 20.0 || process.resource_score >= 80.0 {
                (RiskLevel::Low, ProcessSuggestedAction::SafeToEnd)
            } else {
                (RiskLevel::Medium, ProcessSuggestedAction::Review)
            }
        }
        _ => {
            if process.cpu_usage >= 35.0 || process.resource_score >= 90.0 {
                (RiskLevel::Medium, ProcessSuggestedAction::Review)
            } else {
                (RiskLevel::Low, ProcessSuggestedAction::SafeToEnd)
            }
        }
    }
}

fn local_summary(
    process: &ProcessRecord,
    risk: RiskLevel,
    suggested_action: ProcessSuggestedAction,
) -> String {
    let category_label = category_label(&process.category);
    let pressure = resource_pressure_text(process);
    let action_label = process_action_label(suggested_action);
    let risk_label = risk_label(risk);

    format!(
        "{}（PID {}）当前属于{}，{}。本地规则判断风险{}，建议{}。",
        process.name, process.pid, category_label, pressure, risk_label, action_label
    )
}

fn local_reason(
    process: &ProcessRecord,
    risk: RiskLevel,
    suggested_action: ProcessSuggestedAction,
) -> String {
    let cpu_text = format!("{:.1}%", process.cpu_usage);
    let memory_text = format_size(process.memory_bytes, BINARY);
    let disk_text = format_size(process.disk_read_bytes + process.disk_written_bytes, BINARY);
    let runtime_text = format_runtime(process.run_time_seconds);

    let base = format!(
        "进程分类：{}；CPU：{}；内存：{}；本次采样磁盘读写：{}；已运行约{}。",
        category_label(&process.category),
        cpu_text,
        memory_text,
        disk_text,
        runtime_text
    );

    let advice = match suggested_action {
        ProcessSuggestedAction::AvoidEnd => {
            "这类进程通常与系统稳定性、安全防护或关键后台服务有关，不建议在本工具里直接结束。"
        }
        ProcessSuggestedAction::EndAfterSave => {
            "这更像是用户正在使用或可能正在写入数据的应用，结束前应先保存工作，避免丢失未保存内容。"
        }
        ProcessSuggestedAction::Review => {
            "它不一定不能结束，但在关闭前应确认是否正在同步文件、更新程序、运行容器或提供后台能力。"
        }
        ProcessSuggestedAction::SafeToEnd => {
            "从当前名称、路径和资源模式看，更像是辅助进程、更新器或短时后台任务，通常可以在确认无依赖后临时结束。"
        }
    };

    let risk_note = match risk {
        RiskLevel::High => "高风险的意思是结束后可能影响系统稳定性、登录状态或安全防护。",
        RiskLevel::Medium => "中风险表示更适合人工确认后再操作。",
        RiskLevel::Low => "低风险表示通常只会中断临时后台任务，影响相对可控。",
    };

    format!("{base}{advice}{risk_note}")
}

fn resource_pressure_text(process: &ProcessRecord) -> String {
    let mut parts = Vec::new();
    if process.cpu_usage >= 20.0 {
        parts.push(format!("CPU 占用偏高（{:.1}%）", process.cpu_usage));
    }
    if process.memory_bytes >= 512 * 1024 * 1024 {
        parts.push(format!(
            "内存占用偏高（{}）",
            format_size(process.memory_bytes, BINARY)
        ));
    }
    let disk_total = process.disk_read_bytes + process.disk_written_bytes;
    if disk_total >= 20 * 1024 * 1024 {
        parts.push(format!(
            "磁盘活动较高（{}）",
            format_size(disk_total, BINARY)
        ));
    }

    if parts.is_empty() {
        "当前资源占用不算极端，但仍在监控列表中".to_string()
    } else {
        parts.join("，")
    }
}

fn category_label(category: &str) -> &'static str {
    match category {
        "system_critical" => "系统关键进程",
        "system_service" => "系统服务",
        "windows_component" => "Windows 组件",
        "security" => "安全防护进程",
        "browser" => "浏览器/网页进程",
        "development" => "开发工具或命令行进程",
        "virtualization" => "容器/虚拟化进程",
        "sync" => "同步或网盘进程",
        "background_helper" => "后台辅助进程",
        "user_app" => "用户应用进程",
        _ => "未知类型进程",
    }
}

fn process_action_label(action: ProcessSuggestedAction) -> &'static str {
    match action {
        ProcessSuggestedAction::SafeToEnd => "可以尝试结束",
        ProcessSuggestedAction::EndAfterSave => "先保存工作再结束",
        ProcessSuggestedAction::Review => "先查看用途再决定",
        ProcessSuggestedAction::AvoidEnd => "不建议结束",
    }
}

fn risk_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "较低",
        RiskLevel::Medium => "中等",
        RiskLevel::High => "较高",
    }
}

fn format_runtime(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds} 秒");
    }

    let minutes = seconds / 60;
    let remain_seconds = seconds % 60;
    if minutes < 60 {
        if remain_seconds == 0 {
            return format!("{minutes} 分钟");
        }
        return format!("{minutes} 分 {remain_seconds} 秒");
    }

    let hours = minutes / 60;
    let remain_minutes = minutes % 60;
    if remain_minutes == 0 {
        return format!("{hours} 小时");
    }
    format!("{hours} 小时 {remain_minutes} 分")
}

async fn request_remote_process_insight(
    process: &ProcessRecord,
    local_insight: &ProcessAiInsight,
    config: &AdvisorConfig,
    api_key: &str,
) -> Result<ProcessAiInsight> {
    let payload = json!({
        "pid": process.pid,
        "parent_pid": process.parent_pid,
        "name": process.name,
        "exe_path": process.exe_path,
        "command": process.command,
        "cpu_usage": process.cpu_usage,
        "memory_bytes": process.memory_bytes,
        "virtual_memory_bytes": process.virtual_memory_bytes,
        "disk_read_bytes": process.disk_read_bytes,
        "disk_written_bytes": process.disk_written_bytes,
        "run_time_seconds": process.run_time_seconds,
        "status": process.status,
        "category": process.category,
        "is_critical": process.is_critical,
        "resource_score": process.resource_score,
        "local_summary": local_insight.summary,
        "local_reason": local_insight.reason,
        "local_action": local_insight.suggested_action,
        "local_risk": local_insight.risk,
    });

    let content = send_chat_completion(
        config,
        api_key,
        "You are a Windows process diagnosis assistant. Reply with a JSON object only. Be conservative. Never recommend ending critical system or security processes. Explain in concise Chinese.",
        &format!(
            "请根据下面的 Windows 进程摘要，只返回一个 JSON 对象，格式为 {{\"summary\":\"中文结论\",\"suggested_action\":\"safe_to_end|end_after_save|review|avoid_end\",\"risk\":\"low|medium|high\",\"reason\":\"中文说明\"}}。不要输出 Markdown。不要假装知道进程内部业务，只能基于进程名、路径、命令行、资源占用和本地规则做保守判断。输入：{}",
            serde_json::to_string(&payload)?
        ),
    )
    .await?;

    let response = parse_process_insight_response(&content)?;
    if process.is_critical && response.suggested_action != ProcessSuggestedAction::AvoidEnd {
        return Ok(local_insight.clone());
    }

    Ok(ProcessAiInsight {
        pid: process.pid,
        name: process.name.clone(),
        source: format!("remote:{}", config.model),
        summary: non_empty_or(response.summary, local_insight.summary.clone()),
        suggested_action: response.suggested_action,
        risk: response.risk,
        reason: non_empty_or(response.reason, local_insight.reason.clone()),
        remote_attempted: true,
        used_fallback: false,
        fallback_reason: None,
    })
}

async fn request_remote_process_follow_up(
    process: &ProcessRecord,
    insight: &ProcessAiInsight,
    question: &str,
    history: &[ProcessAiFollowUpTurn],
    config: &AdvisorConfig,
    api_key: &str,
) -> Result<ProcessAiFollowUpAnswer> {
    let conversation_history: Vec<_> = history
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|turn| {
            json!({
                "question": turn.question,
                "answer": turn.answer,
            })
        })
        .collect();

    let payload = json!({
        "pid": process.pid,
        "parent_pid": process.parent_pid,
        "name": process.name,
        "exe_path": process.exe_path,
        "command": process.command,
        "cpu_usage": process.cpu_usage,
        "memory_bytes": process.memory_bytes,
        "virtual_memory_bytes": process.virtual_memory_bytes,
        "disk_read_bytes": process.disk_read_bytes,
        "disk_written_bytes": process.disk_written_bytes,
        "run_time_seconds": process.run_time_seconds,
        "status": process.status,
        "category": process.category,
        "is_critical": process.is_critical,
        "resource_score": process.resource_score,
        "current_summary": insight.summary,
        "current_reason": insight.reason,
        "current_action": insight.suggested_action,
        "current_risk": insight.risk,
        "conversation_history": conversation_history,
        "user_question": question,
    });

    let content = send_chat_completion(
        config,
        api_key,
        "You are a Windows process diagnosis assistant. Answer in concise Chinese. Be conservative. Never claim certainty you do not have. Never recommend ending critical system or security processes.",
        &format!(
            "请根据下面的 Windows 进程信息和已有诊断结果，回答用户的追问。只输出一个 JSON 对象，格式为 {{\"answer\":\"中文回答\"}}。不要输出 Markdown。不要假装知道进程内部业务，只能基于进程名、路径、命令行、资源占用和已有诊断做保守判断。输入：{}",
            serde_json::to_string(&payload)?
        ),
    )
    .await?;

    let response = parse_process_follow_up_response(&content)?;
    let answer = non_empty_or(
        response.answer,
        build_local_follow_up_answer(process, insight, question, history).answer,
    );

    Ok(ProcessAiFollowUpAnswer {
        pid: process.pid,
        name: process.name.clone(),
        question: question.to_string(),
        answer,
        source: format!("remote:{}", config.model),
        remote_attempted: true,
        used_fallback: false,
        fallback_reason: None,
    })
}

async fn send_chat_completion(
    config: &AdvisorConfig,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let client = Client::new();
    let response = client
        .post(build_compatible_endpoint(
            &config.base_url,
            "chat/completions",
        ))
        .bearer_auth(api_key)
        .json(&json!({
            "model": config.model,
            "temperature": 0.2,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ]
        }))
        .send()
        .await?
        .error_for_status()?;

    let body: ChatCompletionResponse = response.json().await?;
    body.choices
        .into_iter()
        .next()
        .map(|item| item.message.content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| anyhow!("AI 没有返回有效的进程诊断内容。"))
}

fn build_compatible_endpoint(base_url: &str, endpoint: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/{endpoint}")
    } else {
        format!("{trimmed}/v1/{endpoint}")
    }
}

fn parse_process_insight_response(content: &str) -> Result<ProcessInsightResponse> {
    let trimmed = content.trim();
    if let Ok(parsed) = serde_json::from_str::<ProcessInsightResponse>(trimmed) {
        return Ok(parsed);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_slice = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str::<ProcessInsightResponse>(json_slice) {
            return Ok(parsed);
        }
    }

    Err(anyhow!("AI 返回内容不是可解析的进程诊断 JSON。"))
}

fn parse_process_follow_up_response(content: &str) -> Result<ProcessFollowUpResponse> {
    let trimmed = content.trim();
    if let Ok(parsed) = serde_json::from_str::<ProcessFollowUpResponse>(trimmed) {
        return Ok(parsed);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_slice = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str::<ProcessFollowUpResponse>(json_slice) {
            return Ok(parsed);
        }
    }

    Err(anyhow!("AI 返回内容不是可解析的进程追问 JSON"))
}

fn non_empty_or(value: String, fallback: String) -> String {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim().to_string()
    }
}

fn build_history_suffix(history: &[ProcessAiFollowUpTurn]) -> String {
    match history.last() {
        Some(last) => format!(
            " 上一轮你问的是“{}”，当时的回答重点是：{}。",
            last.question.trim(),
            truncate_text(last.answer.trim(), 120)
        ),
        None => String::new(),
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        text.to_string()
    } else {
        format!(
            "{}...",
            chars.into_iter().take(max_chars).collect::<String>()
        )
    }
}

fn contains_any(question: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| question.contains(keyword))
}

#[derive(Debug, Deserialize)]
struct ProcessInsightResponse {
    summary: String,
    suggested_action: ProcessSuggestedAction,
    risk: RiskLevel,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ProcessFollowUpResponse {
    answer: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::{build_local_process_insight, classify_process_metadata, compute_resource_score};
    use crate::models::ProcessRecord;

    #[test]
    fn classifies_critical_system_process() {
        let (category, critical) = classify_process_metadata(
            "lsass.exe",
            Some(std::path::Path::new("C:/Windows/System32/lsass.exe")),
            &[],
            888,
        );
        assert_eq!(category, "system_critical");
        assert!(critical);
    }

    #[test]
    fn local_process_insight_keeps_critical_process_safe() {
        let process = ProcessRecord {
            pid: 888,
            parent_pid: Some(4),
            name: "lsass.exe".to_string(),
            exe_path: Some("C:/Windows/System32/lsass.exe".into()),
            command: Vec::new(),
            cpu_usage: 2.0,
            memory_bytes: 128 * 1024 * 1024,
            virtual_memory_bytes: 0,
            disk_read_bytes: 0,
            disk_written_bytes: 0,
            run_time_seconds: 600,
            status: "run".to_string(),
            category: "system_critical".to_string(),
            is_critical: true,
            resource_score: compute_resource_score(2.0, 128 * 1024 * 1024, 0, 0),
        };

        let insight = build_local_process_insight(&process);
        assert_eq!(format!("{:?}", insight.suggested_action), "AvoidEnd");
    }
}
