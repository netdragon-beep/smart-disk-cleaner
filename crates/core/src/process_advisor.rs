use crate::ai_advisor::AdvisorConfig;
use crate::models::{ProcessAiInsight, ProcessRecord, ProcessSuggestedAction, RiskLevel};
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

fn non_empty_or(value: String, fallback: String) -> String {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim().to_string()
    }
}

#[derive(Debug, Deserialize)]
struct ProcessInsightResponse {
    summary: String,
    suggested_action: ProcessSuggestedAction,
    risk: RiskLevel,
    reason: String,
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
