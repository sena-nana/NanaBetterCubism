use crate::agent::computer_control::ComputerApproval;
use crate::agent::store::PendingQuestion;
use crate::agent::PlanApprovalAction;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PendingUserAction {
    Question {
        action_id: String,
        conversation_id: String,
        question: String,
        options: Vec<String>,
    },
    ComputerApproval {
        #[serde(flatten)]
        approval: ComputerApproval,
    },
    PlanApproval {
        #[serde(flatten)]
        approval: PlanApprovalAction,
    },
}

impl PendingUserAction {
    pub fn action_id(&self) -> &str {
        match self {
            Self::Question { action_id, .. } => action_id,
            Self::ComputerApproval { approval } => &approval.action_id,
            Self::PlanApproval { approval } => &approval.action_id,
        }
    }
}

impl From<PlanApprovalAction> for PendingUserAction {
    fn from(approval: PlanApprovalAction) -> Self {
        Self::PlanApproval { approval }
    }
}

impl From<PendingQuestion> for PendingUserAction {
    fn from(value: PendingQuestion) -> Self {
        Self::Question {
            action_id: value.action_id,
            conversation_id: value.conversation_id,
            question: value.question,
            options: value.options,
        }
    }
}

impl From<ComputerApproval> for PendingUserAction {
    fn from(approval: ComputerApproval) -> Self {
        Self::ComputerApproval { approval }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computer_approval_uses_the_unified_frontend_contract() {
        let action = PendingUserAction::from(ComputerApproval {
            action_id: "approval".into(),
            conversation_id: "conversation".into(),
            goal: "调整控制点".into(),
            reason: "没有可用 API".into(),
            target_window_title: "Cubism Editor".into(),
            steps: Vec::new(),
            allowed_actions: Vec::new(),
            includes_file_dialogs: false,
            impact: "将注入输入".into(),
            cannot_undo: true,
            expires_at: "2026-07-15T00:00:00Z".into(),
        });
        let value = serde_json::to_value(action).unwrap();
        assert_eq!(value["kind"], "computer_approval");
        assert_eq!(value["actionId"], "approval");
        assert_eq!(value["targetWindowTitle"], "Cubism Editor");
        assert_eq!(value["cannotUndo"], true);
    }

    #[test]
    fn plan_approval_uses_the_unified_frontend_contract() {
        let action = PendingUserAction::from(PlanApprovalAction {
            action_id: "plan".into(),
            conversation_id: "conversation".into(),
            title: "参数计划".into(),
        });
        let value = serde_json::to_value(action).unwrap();
        assert_eq!(value["kind"], "plan_approval");
        assert_eq!(value["actionId"], "plan");
        assert_eq!(value["conversationId"], "conversation");
    }
}
