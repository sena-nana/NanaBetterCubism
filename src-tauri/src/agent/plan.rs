use crate::agent::store::PlanStep;
use crate::agent::{new_id, AgentError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanDocument {
    pub title: String,
    pub summary: String,
    pub steps: Vec<String>,
    pub diagram: String,
    pub acceptance: Vec<String>,
    pub assumptions: Vec<String>,
    pub risks: Vec<String>,
}

impl PlanDocument {
    pub fn validate(self) -> Result<Self, AgentError> {
        fn required(value: &str, field: &str) -> Result<(), AgentError> {
            if value.trim().is_empty() {
                Err(AgentError::new(
                    "invalid_plan",
                    format!("计划字段 {field} 不能为空。"),
                ))
            } else {
                Ok(())
            }
        }
        fn required_list(values: &[String], field: &str) -> Result<(), AgentError> {
            if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
                Err(AgentError::new(
                    "invalid_plan",
                    format!("计划字段 {field} 必须包含非空条目。"),
                ))
            } else {
                Ok(())
            }
        }

        required(&self.title, "title")?;
        required(&self.summary, "summary")?;
        required(&self.diagram, "diagram")?;
        required_list(&self.steps, "steps")?;
        required_list(&self.acceptance, "acceptance")?;
        required_list(&self.assumptions, "assumptions")?;
        required_list(&self.risks, "risks")?;
        if self.diagram.contains("```") || contains_html(&self.diagram) {
            return Err(AgentError::new(
                "invalid_plan",
                "diagram 只能包含 Mermaid 图源，不能包含围栏或 HTML。",
            ));
        }
        let lowercase_diagram = self.diagram.to_ascii_lowercase();
        if lowercase_diagram.contains("http://") || lowercase_diagram.contains("https://") {
            return Err(AgentError::new(
                "invalid_plan",
                "diagram 不能包含外部链接。",
            ));
        }
        Ok(self)
    }

    pub fn markdown(&self) -> String {
        fn list(values: &[String]) -> String {
            values
                .iter()
                .map(|value| format!("- {}", value.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        }
        let steps = self
            .steps
            .iter()
            .enumerate()
            .map(|(index, value)| format!("{}. {}", index + 1, value.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "# {}\n\n## 概要\n\n{}\n\n## 制作步骤\n\n{}\n\n## 结构图\n\n```mermaid\n{}\n```\n\n## 验收方案\n\n{}\n\n## 假设\n\n{}\n\n## 风险\n\n{}",
            self.title.trim(),
            self.summary.trim(),
            steps,
            self.diagram.trim(),
            list(&self.acceptance),
            list(&self.assumptions),
            list(&self.risks),
        )
    }

    pub fn todo_steps(&self, status: &str) -> Vec<PlanStep> {
        self.steps
            .iter()
            .map(|title| PlanStep {
                id: new_id(),
                title: title.trim().to_string(),
                status: status.into(),
            })
            .collect()
    }
}

fn contains_html(source: &str) -> bool {
    source.split('<').skip(1).any(|rest| {
        let candidate = rest.trim_start().trim_start_matches('/');
        if candidate.starts_with('!') {
            return candidate.contains('>');
        }
        let name_end = candidate
            .find(|character: char| !character.is_ascii_alphanumeric() && character != '-')
            .unwrap_or(candidate.len());
        if name_end == 0 {
            return false;
        }
        let suffix = &candidate[name_end..];
        suffix.starts_with('>')
            || suffix.starts_with("/>")
            || suffix
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
                && suffix.lines().next().is_some_and(|line| line.contains('>'))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanApprovalAction {
    pub action_id: String,
    pub conversation_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    Approve,
    Revise,
    Cancel,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecisionResult {
    ExecutionStarted,
    RevisionStarted,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct PendingPlanApproval {
    pub action: PlanApprovalAction,
    pub plan: PlanDocument,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> PlanDocument {
        PlanDocument {
            title: "参数整理".into(),
            summary: "先核对再调整。".into(),
            steps: vec!["检查参数".into(), "执行调整".into()],
            diagram: "flowchart TD\nA --> B".into(),
            acceptance: vec!["回读参数".into()],
            assumptions: vec!["Editor 已连接".into()],
            risks: vec!["版本能力差异".into()],
        }
    }

    #[test]
    fn validated_plan_renders_canonical_markdown_and_todos() {
        let plan = document().validate().unwrap();
        let markdown = plan.markdown();
        assert!(markdown.contains("```mermaid\nflowchart TD\nA --> B\n```"));
        let todos = plan.todo_steps("cancelled");
        assert_eq!(todos.len(), 2);
        assert!(todos.iter().all(|step| step.status == "cancelled"));
    }

    #[test]
    fn diagram_rejects_markup_and_external_links() {
        for diagram in [
            "```mermaid\nflowchart TD",
            "<script>",
            "HTTPS://example.com",
        ] {
            let mut plan = document();
            plan.diagram = diagram.into();
            assert!(plan.validate().is_err());
        }
        let mut comparison = document();
        comparison.diagram = "flowchart TD\nA[\"x < y\"] --> B".into();
        assert!(comparison.validate().is_ok());
    }
}
