use crate::ai_advisor::AdvisorConfig;
use crate::models::{
    MigrationActionStep, MigrationDocExcerpt, MigrationExecutionRecord, MigrationHistoricalCase,
    MigrationPlan, MigrationPlanReview,
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

const DOC_EXCERPT_LIMIT: usize = 1200;
const CASE_LIMIT: usize = 5;

pub async fn refine_plan_with_ai(
    base_plan: &MigrationPlan,
    history: &[MigrationExecutionRecord],
    config: &AdvisorConfig,
) -> Result<MigrationPlanReview> {
    let doc_excerpts = collect_doc_excerpts(base_plan).await;
    let historical_cases = collect_historical_cases(base_plan, history);

    let Some(api_key) = config
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(MigrationPlanReview {
            plan: base_plan.clone(),
            source: "local_rules".to_string(),
            remote_attempted: false,
            used_fallback: false,
            fallback_reason: None,
            doc_excerpts,
            historical_cases,
        });
    };

    match request_plan_refinement(base_plan, &doc_excerpts, &historical_cases, config, api_key)
        .await
    {
        Ok(refined) => Ok(MigrationPlanReview {
            plan: refined,
            source: format!("remote:{}", config.model),
            remote_attempted: true,
            used_fallback: false,
            fallback_reason: None,
            doc_excerpts,
            historical_cases,
        }),
        Err(error) => Ok(MigrationPlanReview {
            plan: base_plan.clone(),
            source: "local_rules".to_string(),
            remote_attempted: true,
            used_fallback: true,
            fallback_reason: Some(error.to_string()),
            doc_excerpts,
            historical_cases,
        }),
    }
}

async fn collect_doc_excerpts(plan: &MigrationPlan) -> Vec<MigrationDocExcerpt> {
    let client = Client::new();
    let mut results = Vec::new();

    for source in &plan.doc_sources {
        let excerpt = if let Some(uri) = &source.uri {
            match fetch_excerpt(&client, uri).await {
                Ok(text) => text,
                Err(_) => source.note.clone(),
            }
        } else {
            source.note.clone()
        };

        results.push(MigrationDocExcerpt {
            title: source.title.clone(),
            kind: source.kind,
            uri: source.uri.clone(),
            excerpt,
        });
    }

    results
}

async fn fetch_excerpt(client: &Client, uri: &str) -> Result<String> {
    let text = client
        .get(uri)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(compact_excerpt(&strip_html_tags(&text), DOC_EXCERPT_LIMIT))
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut inside_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                output.push(' ');
            }
            _ if !inside_tag => output.push(ch),
            _ => {}
        }
    }

    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compact_excerpt(input: &str, limit: usize) -> String {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() <= limit {
        normalized
    } else {
        format!("{}...", &normalized[..limit])
    }
}

fn collect_historical_cases(
    plan: &MigrationPlan,
    history: &[MigrationExecutionRecord],
) -> Vec<MigrationHistoricalCase> {
    let mut cases = history
        .iter()
        .filter(|item| item.plan_id == plan.id || item.title == plan.title)
        .map(|item| MigrationHistoricalCase {
            run_id: item.run_id.clone(),
            title: item.title.clone(),
            status: item.status,
            dry_run: item.dry_run,
            action_titles: item
                .logs
                .iter()
                .map(|log| log.detail.clone())
                .take(8)
                .collect(),
            failure_reason: item.failure_reason.clone(),
        })
        .collect::<Vec<_>>();

    if cases.is_empty() {
        cases = history
            .iter()
            .rev()
            .take(CASE_LIMIT)
            .map(|item| MigrationHistoricalCase {
                run_id: item.run_id.clone(),
                title: item.title.clone(),
                status: item.status,
                dry_run: item.dry_run,
                action_titles: item
                    .logs
                    .iter()
                    .map(|log| log.detail.clone())
                    .take(8)
                    .collect(),
                failure_reason: item.failure_reason.clone(),
            })
            .collect();
    }

    cases.truncate(CASE_LIMIT);
    cases
}

async fn request_plan_refinement(
    base_plan: &MigrationPlan,
    doc_excerpts: &[MigrationDocExcerpt],
    historical_cases: &[MigrationHistoricalCase],
    config: &AdvisorConfig,
    api_key: &str,
) -> Result<MigrationPlan> {
    let action_catalog = base_plan
        .actions
        .iter()
        .map(|action| {
            json!({
                "id": action.id,
                "kind": action.kind,
                "title": action.title,
                "detail": action.detail,
                "required": action.required,
                "enabledByDefault": action.enabled_by_default,
                "params": action.params,
            })
        })
        .collect::<Vec<_>>();

    let payload = json!({
        "plan": base_plan,
        "docExcerpts": doc_excerpts,
        "historicalCases": historical_cases,
        "actionCatalog": action_catalog,
    });

    let content = send_chat_completion(
        config,
        api_key,
        "You are a Windows application migration planner. You must return JSON only. Never invent executable actions outside the provided actionCatalog. You may only reorder actions, rewrite titles/details, adjust enabledByDefault for non-required actions, improve summary/rationale, and improve verification steps. Summary and rationale must be Chinese-simplified.",
        &format!(
            "请根据下列迁移计划、官方文档摘要和历史案例，对迁移计划进行结构化优化。你只能使用 actionCatalog 中已有的动作 id，不能生成新的危险动作。请只返回 JSON，格式为 {{\"summary\":\"...\",\"rationale\":\"...\",\"actionOverrides\":[{{\"id\":\"...\",\"title\":\"...\",\"detail\":\"...\",\"enabledByDefault\":true}}],\"recommendedActionIds\":[\"...\"],\"verificationSteps\":[{{\"title\":\"...\",\"detail\":\"...\",\"required\":true}}]}}。输入数据：{}",
            serde_json::to_string(&payload)?
        ),
    )
    .await?;

    let refinement = parse_refinement_response(&content)?;
    Ok(apply_refinement(base_plan, refinement))
}

fn apply_refinement(
    base_plan: &MigrationPlan,
    refinement: PlanRefinementResponse,
) -> MigrationPlan {
    let mut actions = base_plan.actions.clone();

    for override_item in refinement.action_overrides {
        if let Some(action) = actions.iter_mut().find(|item| item.id == override_item.id) {
            if let Some(title) = override_item.title {
                action.title = title;
            }
            if let Some(detail) = override_item.detail {
                action.detail = detail;
            }
            if let Some(enabled) = override_item.enabled_by_default {
                if !action.required {
                    action.enabled_by_default = enabled;
                }
            }
        }
    }

    if !refinement.recommended_action_ids.is_empty() {
        for action in &mut actions {
            if !action.required {
                action.enabled_by_default = refinement.recommended_action_ids.contains(&action.id);
            }
        }
    }

    MigrationPlan {
        summary: if refinement.summary.trim().is_empty() {
            base_plan.summary.clone()
        } else {
            refinement.summary
        },
        rationale: if refinement.rationale.trim().is_empty() {
            base_plan.rationale.clone()
        } else {
            refinement.rationale
        },
        actions,
        verification_steps: if refinement.verification_steps.is_empty() {
            base_plan.verification_steps.clone()
        } else {
            refinement.verification_steps
        },
        ..base_plan.clone()
    }
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
            "temperature": 0.1,
            "response_format": { "type": "json_object" },
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
        .ok_or_else(|| anyhow!("AI 没有返回有效内容。"))
}

fn build_compatible_endpoint(base_url: &str, endpoint: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/{endpoint}")
    } else {
        format!("{trimmed}/v1/{endpoint}")
    }
}

fn parse_refinement_response(content: &str) -> Result<PlanRefinementResponse> {
    let trimmed = content.trim();
    if let Ok(parsed) = serde_json::from_str::<PlanRefinementResponse>(trimmed) {
        return Ok(parsed);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_slice = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str::<PlanRefinementResponse>(json_slice) {
            return Ok(parsed);
        }
    }

    Err(anyhow!("AI 返回内容不是可解析的 JSON。"))
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActionOverride {
    id: String,
    title: Option<String>,
    detail: Option<String>,
    enabled_by_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanRefinementResponse {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    action_overrides: Vec<ActionOverride>,
    #[serde(default)]
    recommended_action_ids: Vec<String>,
    #[serde(default)]
    verification_steps: Vec<MigrationActionStep>,
}

#[cfg(test)]
mod tests {
    use super::{apply_refinement, compact_excerpt, strip_html_tags, PlanRefinementResponse};
    use crate::models::{
        MigrationAction, MigrationActionKind, MigrationCategory, MigrationDocSource,
        MigrationDocSourceKind, MigrationObjectKind, MigrationPlan, MigrationSupportLevel,
        RiskLevel,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn sample_plan() -> MigrationPlan {
        MigrationPlan {
            id: "plan-1".to_string(),
            title: "测试计划".to_string(),
            category: MigrationCategory::WechatData,
            object_kind: MigrationObjectKind::AppData,
            support_level: MigrationSupportLevel::Guided,
            risk: RiskLevel::Medium,
            estimated_size: 10,
            source_path: PathBuf::from("C:/A"),
            recommended_target_dir: PathBuf::from("D:/B"),
            recommended_target_path: PathBuf::from("D:/B/A"),
            summary: "原始摘要".to_string(),
            rationale: "原始原因".to_string(),
            tags: vec!["tag".to_string()],
            doc_sources: vec![MigrationDocSource {
                title: "doc".to_string(),
                kind: MigrationDocSourceKind::LocalRule,
                uri: None,
                note: "note".to_string(),
            }],
            actions: vec![MigrationAction {
                id: "a1".to_string(),
                kind: MigrationActionKind::MovePath,
                title: "移动".to_string(),
                detail: "detail".to_string(),
                required: false,
                enabled_by_default: false,
                params: json!({}),
            }],
            verification_steps: Vec::new(),
        }
    }

    #[test]
    fn strips_html() {
        assert_eq!(
            strip_html_tags("<p>Hello <b>World</b></p>").trim(),
            "Hello World"
        );
    }

    #[test]
    fn compacts_excerpt() {
        assert!(compact_excerpt("a b c d e", 3).ends_with("..."));
    }

    #[test]
    fn applies_recommended_action_ids() {
        let refined = apply_refinement(
            &sample_plan(),
            PlanRefinementResponse {
                summary: "新摘要".to_string(),
                rationale: "新原因".to_string(),
                action_overrides: Vec::new(),
                recommended_action_ids: vec!["a1".to_string()],
                verification_steps: Vec::new(),
            },
        );
        assert_eq!(refined.summary, "新摘要");
        assert!(refined.actions[0].enabled_by_default);
    }
}
