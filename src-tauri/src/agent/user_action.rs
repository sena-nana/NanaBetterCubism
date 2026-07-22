use crate::agent::store::PendingQuestion;
use crate::agent::PlanApprovalAction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComputerPermissionDecision {
    Allow,
    Deny,
}

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
    PlanApproval {
        #[serde(flatten)]
        approval: PlanApprovalAction,
    },
    ComputerPermission {
        action_id: String,
        conversation_id: String,
        goal: String,
        window_title: String,
        includes_file_dialogs: bool,
    },
}

impl PendingUserAction {
    pub fn action_id(&self) -> &str {
        match self {
            Self::Question { action_id, .. } => action_id,
            Self::PlanApproval { approval } => &approval.action_id,
            Self::ComputerPermission { action_id, .. } => action_id,
        }
    }

    pub fn is_computer_permission(&self) -> bool {
        matches!(self, Self::ComputerPermission { .. })
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn computer_permission_exposes_only_user_facing_context() {
        let action = PendingUserAction::ComputerPermission {
            action_id: "permission".into(),
            conversation_id: "conversation".into(),
            goal: "调整控制点".into(),
            window_title: "Cubism Editor".into(),
            includes_file_dialogs: false,
        };
        let value = serde_json::to_value(action).unwrap();
        assert_eq!(value["kind"], "computer_permission");
        assert_eq!(value["actionId"], "permission");
        assert!(value.get("windowId").is_none());
        assert!(value.get("grantId").is_none());
    }
}
